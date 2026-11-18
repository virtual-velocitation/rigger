//! Table B2: a tree held by the Linux keeper, the built binary re-run as `rigger-keeper`
//! (`crates/rigger-process/src/child_tree/linux.rs`). The handle answers the main's pid and its
//! exact status; an end, a finish, a drop and a TERM to the keeper each leave nothing the tree
//! started running; a finish after the main's exit first waits, to a bound, for the session's
//! stop-failure hook; a wait for the main's output ends, once the main exits, what it left
//! holding that output; a daemon one session started ends with it under the keeper, so a sibling
//! session waiting on its answer sees the answers end and its next request fail, and runs on, while
//! a watched session's exited main leaves it running; the keeper holds none of the session's
//! streams; and a keeper that is gone or deaf is reported, never trusted, and a main a gone keeper
//! left is reported, never signalled. Every process a row starts is read through the FIFO witness
//! (`common::witness`), and the test signals none of them.

use crate::common::fixtures::fixture_script;
use crate::common::rigger_bin;
use crate::common::witness::{make_fifo, Reading, Witness};
use rigger::child_tree::{line_channel, Containment, EndReport, ReapedChild, TreeCommand, Why};
use rigger::os_process;
use rigger::reap::END_BOUND;
use std::io::{self, BufRead, BufReader};
use std::ops::Range;
use std::path::PathBuf;
use std::process::{ExitStatus, Stdio};
use std::sync::mpsc::RecvTimeoutError;
use std::time::{Duration, Instant};

/// The keeper the built binary serves.
fn keeper() -> Containment {
    Containment::Keeper(rigger_bin())
}

/// The fixture shell `body` (`Witness::script`) as a tree contained as `how`, its input and output
/// piped, read through `witness`.
fn command(witness: &Witness, how: &Containment, body: &str) -> TreeCommand {
    let mut command = TreeCommand::new("sh", how).expect("a contained command");
    command
        .command_mut()
        .arg("-c")
        .arg(Witness::script(body))
        .envs(witness.env())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped());
    command
}

/// The fixture shell `body` started under the keeper.
fn kept(witness: &Witness, body: &str) -> ReapedChild {
    command(witness, &keeper(), body)
        .spawn()
        .expect("spawn the kept tree")
}

#[test]
fn a_kept_tree_answers_its_mains_pid_and_the_main_runs_under_the_keeper() {
    let mut witness = Witness::new();
    let mut tree = kept(&witness, "echo $$; up; held");
    let mut stdout = BufReader::new(tree.take_stdout().expect("the main's output"));
    let mut line = String::new();
    stdout.read_line(&mut line).expect("read the main's pid");
    assert!(witness.wait_up(1), "the main is up");

    let main = tree.id();

    assert_eq!(line.trim(), main.to_string(), "id() is the main's pid");
    let parent = os_process::parent(main).expect("the main's parent");
    assert!(
        os_process::command(parent).starts_with("rigger-keeper "),
        "the main's parent is the keeper: {}",
        os_process::command(parent)
    );
    assert_eq!(
        (tree.end(), witness.read()),
        (EndReport::default(), Reading::Ended)
    );
}

#[test]
fn a_kept_spawn_of_a_missing_program_fails_as_not_found() {
    let spawned = TreeCommand::new("/nonexistent/rigger-no-such-program", &keeper())
        .expect("a contained command")
        .spawn();

    match spawned {
        Ok(_) => panic!("a missing program spawned"),
        Err(error) => assert_eq!(error.kind(), io::ErrorKind::NotFound, "{error}"),
    }
}

/// Waits on `tree`'s main on a thread of its own, answering the main's status and the tree, or
/// `None` when the wait has not answered within [`END_BOUND`].
fn waited(tree: ReapedChild) -> Option<(io::Result<ExitStatus>, ReapedChild)> {
    let (answer, answered) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut tree = tree;
        let status = tree.wait();
        let _ = answer.send((status, tree));
    });
    answered.recv_timeout(END_BOUND).ok()
}

/// The kept fixture `body`, ended by the handle's end once its main is up when `end` is set and
/// waited on otherwise, a wait closing the main's input first as `Child::wait` does, answers the
/// main's exit `code` and ending `signal`, and leaves nothing running.
fn answers_the_mains_status(body: &str, end: bool, code: Option<i32>, signal: Option<i32>) {
    use std::os::unix::process::ExitStatusExt;
    let mut witness = Witness::new();
    let mut tree = kept(&witness, body);

    let status = if end {
        assert!(witness.wait_up(1), "the main is up");
        tree.end();
        tree.try_wait()
            .expect("read the main's status")
            .expect("the ended main's status")
    } else {
        let (status, waited_on) = waited(tree).expect("the wait answers within its bound");
        tree = waited_on;
        status.expect("wait on the main")
    };

    assert_eq!((status.code(), status.signal()), (code, signal));
    drop(tree);
    assert_eq!(witness.read(), Reading::Ended);
}

rigger::test_cases! {
    a_kept_main_that_exits_answers_its_exit_code:
        answers_the_mains_status("exit 3", false, Some(3), None);
    a_kept_main_the_end_ends_answers_the_terminating_signal:
        answers_the_mains_status("up; held", true, None, Some(15));
    a_kept_main_reading_its_input_answers_once_the_wait_closes_it:
        answers_the_mains_status("read -r unused; exit 4", false, Some(4), None);
}

/// Ends a tree through the handle's end.
fn by_the_end(mut tree: ReapedChild) -> EndReport {
    tree.end()
}

/// Ends a tree through the host's ordinary-end call, for no spawn.
fn by_the_finish(tree: ReapedChild) -> EndReport {
    tree.finish(None)
}

/// The kept fixture `body`, with `up` of its processes up and then `before` done to it, is ended
/// by `ending`, which leaves nothing running, reports `sent` processes besides the main it sent its
/// end to, and reports nothing left: a process that ignores the end's request to exit is ended without
/// asking once the grace passes.
fn ends_the_tree(
    body: &str,
    up: usize,
    before: fn(&mut ReapedChild, &mut Witness),
    ending: fn(ReapedChild) -> EndReport,
    sent: usize,
) {
    let mut witness = Witness::new();
    let mut tree = kept(&witness, body);
    assert!(witness.wait_up(up), "the fixture's {up} processes are up");
    before(&mut tree, &mut witness);

    let report = ending(tree);

    assert_eq!(
        (
            witness.read(),
            report.sent.len(),
            report.residue,
            report.skipped,
            report.faults
        ),
        (Reading::Ended, sent, vec![], vec![], vec![]),
    );
}

/// Waits for the main's exit, and reads that what it left still runs: the keeper ends nothing on
/// the main's exit alone.
fn after_the_main_exits(tree: &mut ReapedChild, witness: &mut Witness) {
    tree.wait().expect("wait on the main");
    assert_eq!(
        witness.read(),
        Reading::Held,
        "what the main left runs on past its exit"
    );
}

/// Closes the main's input, and waits for the process the main starts at its input's end to be
/// up, so the end that follows has it to end.
fn after_the_input_closes(tree: &mut ReapedChild, witness: &mut Witness) {
    drop(tree.take_stdin());
    assert!(witness.wait_up(2), "the main starts its holder");
}

// Each end names the one process besides the main it was sent to.
rigger::test_cases! {
    a_kept_end_ends_a_running_tree:
        ends_the_tree("holder; up; held", 2, |_, _| {}, by_the_end, 1);
    a_kept_end_ends_what_an_exited_main_left:
        ends_the_tree("holder; up", 2, after_the_main_exits, by_the_end, 1);
    a_kept_end_ends_what_the_main_starts_as_it_exits_at_the_end_of_its_input:
        ends_the_tree(
            "up; read -r unused; holder; exit 0",
            1,
            after_the_input_closes,
            by_the_end,
            1,
        );
    a_kept_end_ends_a_process_started_in_another_directory:
        ends_the_tree("cd / && holder; up; held", 2, |_, _| {}, by_the_end, 1);
    a_kept_end_ends_a_process_in_a_session_of_its_own:
        ends_the_tree("holder setsid; up; held", 2, |_, _| {}, by_the_end, 1);
    a_kept_end_ends_a_process_that_ignores_the_request_to_exit:
        ends_the_tree("( trap '' TERM; up; held ) & up; held", 2, |_, _| {}, by_the_end, 1);
    a_kept_end_ends_a_main_that_ignores_the_request_to_exit:
        ends_the_tree("trap '' TERM; holder; up; held", 2, |_, _| {}, by_the_end, 1);
    a_kept_finish_ends_a_running_tree:
        ends_the_tree("holder; up; held", 2, |_, _| {}, by_the_finish, 1);
    a_kept_finish_ends_what_an_exited_main_left:
        ends_the_tree("holder; up", 2, after_the_main_exits, by_the_finish, 1);
}

/// The spawn the writer rows finish their tree for. A writer carries the stop-failure hook's words
/// for it as its shell's arguments, as the hook does.
const SPAWN: &str = "u-late-writer";

/// Whether `line` is the stop-failure hook of [`SPAWN`], the one writer a finish for it waits for.
fn the_hook(line: &str) -> bool {
    rigger::hooks::is_stop_failure_hook(line, SPAWN)
}

/// The kept fixture `body`, with `up` of its processes up and, where `exited`, its main exited,
/// finished for [`SPAWN`]: once the finish answers, `ups` of the fixture's processes have written
/// their up byte, the finish took a time within `took` and reports `sent` processes it sent its end
/// to, and nothing is left running or reported left.
fn finishes_for_the_spawn(
    body: &str,
    up: usize,
    exited: bool,
    ups: usize,
    took: Range<Duration>,
    sent: usize,
) {
    let mut witness = Witness::new();
    let mut tree = kept(&witness, body);
    assert!(witness.wait_up(up), "the fixture's {up} processes are up");
    if exited {
        tree.wait().expect("wait on the main");
    }
    let started = Instant::now();

    let report = tree.finish(Some(&the_hook));

    let elapsed = started.elapsed();
    assert_eq!(
        (
            witness.read(),
            witness.up(),
            report.sent.len(),
            report.residue,
            report.skipped,
            report.faults
        ),
        (Reading::Ended, ups, sent, vec![], vec![], vec![])
    );
    assert!(took.contains(&elapsed), "the finish took {elapsed:?}");
}

// A held writer never exits on its own, so a finish that waited for it would take the whole
// bound.
rigger::test_cases! {
    // The writer writes its up byte a second after it starts, then exits: the byte is read only
    // if the finish waited for it, and the finish has nothing left to end.
    a_kept_finish_waits_for_a_writer_its_exited_main_left:
        finishes_for_the_spawn(
            "sh -c 'sleep 1; printf u >&9' hook stop-failure --spawn u-late-writer --class x & up",
            1,
            true,
            2,
            Duration::ZERO..END_BOUND,
            0,
        );
    a_kept_finish_ends_a_writer_still_running_at_its_bound:
        finishes_for_the_spawn(
            "sh -c \"$witness_up; $witness_held\" hook stop-failure --spawn u-late-writer --class x \
             & up",
            2,
            true,
            2,
            END_BOUND..END_BOUND * 2,
            1,
        );
    a_kept_finish_waits_for_no_writer_while_its_main_runs:
        finishes_for_the_spawn(
            "sh -c \"$witness_up; $witness_held\" hook stop-failure --spawn u-late-writer --class x \
             & up; held",
            2,
            false,
            2,
            Duration::ZERO..END_BOUND,
            1,
        );
    a_kept_finish_waits_for_no_hook_of_another_spawn:
        finishes_for_the_spawn(
            "sh -c \"$witness_up; $witness_held\" hook stop-failure --spawn u-other --class x & up",
            2,
            true,
            2,
            Duration::ZERO..END_BOUND,
            1,
        );
    // The spawn's own MCP server names the spawn too, and is no writer.
    a_kept_finish_waits_for_no_mcp_server_of_its_spawn:
        finishes_for_the_spawn(
            "sh -c \"$witness_up; $witness_held\" rigger mcp --spawn u-late-writer & up",
            2,
            true,
            2,
            Duration::ZERO..END_BOUND,
            1,
        );
}

// The holder holds the main's output pipe and never exits on its own, so the wait answers only if
// the end of the tree ended it.
#[test]
fn a_kept_wait_for_the_output_ends_what_the_main_left_holding_it() {
    let mut witness = Witness::new();
    let tree = kept(&witness, "holder; echo out; up");
    assert!(witness.wait_up(2), "the holder and the main are up");
    let (answer, answered) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = answer.send(tree.wait_with_output());
    });

    let waited = answered.recv_timeout(END_BOUND);

    assert_eq!(witness.read(), Reading::Ended, "the holder is ended");
    let (output, report) = waited
        .expect("the wait answers within its bound")
        .expect("wait for the output");
    assert_eq!(
        (
            output.status.code(),
            String::from_utf8_lossy(&output.stdout).into_owned(),
            report.sent.len(),
            report.residue,
            report.skipped
        ),
        (Some(0), "out\n".to_string(), 1, vec![], vec![])
    );
}

/// The lines `lines` carries until none comes for a second.
fn lines_until_quiet(lines: &std::sync::mpsc::Receiver<io::Result<String>>) -> Vec<String> {
    let mut read = Vec::new();
    while let Ok(line) = lines.recv_timeout(Duration::from_secs(1)) {
        read.push(line.expect("a line of text"));
    }
    read
}

/// Session A, contained as `how`, starts the stand-in daemon (`daemon` in
/// `tests/fixtures/witness.sh`) and exits a second later; session B, kept, uses it as `client`,
/// and is waiting on an answer when A finishes. B then says `seen` and runs on, never signalled;
/// A's finish reports `sent` processes it sent its end to and `skipped` it never signalled, and leaves A's
/// witness reading `left`.
fn a_sibling_using_a_daemon_the_session_started(
    how: Containment,
    seen: &[&str],
    sent: usize,
    skipped: usize,
    left: Reading,
) {
    let channel = tempfile::tempdir().expect("a directory for the daemon's channel");
    make_fifo(&channel.path().join("requests"));
    make_fifo(&channel.path().join("answers"));
    let daemon = ("RIGGER_TEST_DAEMON", channel.path());
    let (mut witness_a, mut witness_b) = (Witness::new(), Witness::new());
    let mut a = command(&witness_a, &how, "daemon; up; sleep 1");
    a.command_mut().env(daemon.0, daemon.1);
    let mut a = a.spawn().expect("spawn session A");
    assert!(witness_a.wait_up(1), "A's main is up");
    let mut b = command(&witness_b, &keeper(), "client; up; held");
    b.command_mut().env(daemon.0, daemon.1);
    let mut b = b.spawn().expect("spawn session B");
    let (lines, _reader) = line_channel(b.take_stdout().expect("B's output"));
    assert_eq!(
        (lines_until_quiet(&lines), witness_a.wait_up(2)),
        (
            vec![
                "client: answered: answer first".to_string(),
                "client: waiting".to_string()
            ],
            true
        ),
        "B uses the daemon A started, and waits on its answer to `hold`"
    );
    a.wait().expect("wait on A's main");

    let report = a.finish(Some(&the_hook));

    assert_eq!(
        (
            lines_until_quiet(&lines),
            report.sent.len(),
            report.residue,
            report.skipped.len(),
            witness_a.read()
        ),
        (
            seen.iter().map(|line| line.to_string()).collect::<Vec<_>>(),
            sent,
            vec![],
            skipped,
            left
        )
    );
    assert!(
        matches!(b.try_wait(), Ok(None)),
        "B's main runs on past A's end"
    );
    assert_eq!(witness_b.read(), Reading::Held, "B's processes run on");
    assert_eq!(
        (b.end(), witness_b.read()),
        (EndReport::default(), Reading::Ended)
    );
}

rigger::test_cases! {
    a_sibling_sees_a_kept_sessions_daemon_end_with_it:
        a_sibling_using_a_daemon_the_session_started(
            keeper(),
            &["client: the answers ended", "client: a request failed"],
            1,
            0,
            Reading::Ended,
        );
    a_sibling_keeps_a_watched_sessions_daemon_its_exited_main_left:
        a_sibling_using_a_daemon_the_session_started(
            Containment::Watch(Why::Chosen),
            &[],
            0,
            1,
            Reading::Held,
        );
}

#[test]
fn a_dropped_kept_tree_is_ended() {
    let mut witness = Witness::new();
    let tree = kept(&witness, "holder; up; held");
    assert!(witness.wait_up(2), "the holder and the main are up");

    drop(tree);

    assert_eq!(witness.read(), Reading::Ended);
}

#[test]
fn a_term_to_the_keeper_from_the_working_directory_sweep_ends_the_whole_tree() {
    let root = tempfile::tempdir().expect("an authorized root");
    let work = root.path().join("work");
    std::fs::create_dir(&work).expect("the session's working directory");
    let mut witness = Witness::new();
    let mut command = command(&witness, &keeper(), "( cd / && up && held ) & up; held");
    command.command_mut().current_dir(&work);
    let mut tree = command.spawn().expect("spawn the kept tree");
    assert!(
        witness.wait_up(2),
        "the main and the process outside its directory are up"
    );

    rigger::reap::reap_processes_rooted_under(&work, root.path());

    tree.wait()
        .expect("the main's status, once the keeper has ended the tree");
    assert_eq!(
        witness.read(),
        Reading::Ended,
        "the keeper ended the process the sweep could not see"
    );
}

/// The containment the probe of `keeper` answers is the keeper when `kept`, and a watch naming the
/// failed probe otherwise.
fn probes_as(keeper: PathBuf, kept: bool) {
    let detected = Containment::detect(&keeper);

    if kept {
        assert_eq!(detected, Containment::Keeper(keeper));
    } else {
        assert!(
            matches!(detected, Containment::Watch(Why::ProbeFailed(_))),
            "{detected:?}"
        );
    }
}

rigger::test_cases! {
    the_probe_of_the_built_binary_answers_the_keeper: probes_as(rigger_bin(), true);
    the_probe_of_a_program_that_serves_no_keeper_answers_a_watch:
        probes_as(PathBuf::from("/bin/true"), false);
    the_probe_of_a_missing_program_answers_a_watch:
        probes_as(PathBuf::from("/nonexistent/rigger-no-such-keeper"), false);
}

/// The descriptors from 3 to 12 the main of a tree contained as `how` holds, read by the main
/// itself, once its output reaches its end with the tree not yet ended: no process besides the
/// main holds the main's output.
fn the_mains_descriptors(how: &Containment) -> String {
    let witness = Witness::new();
    let mut tree = command(
        &witness,
        how,
        "for n in 3 4 5 6 7 8 9 10 11 12; do [ -e /proc/$$/fd/$n ] && printf '%s ' $n; done; echo",
    )
    .spawn()
    .expect("spawn the tree");
    let (lines, _reader) = line_channel(tree.take_stdout().expect("the main's output"));
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut read = Vec::new();
    loop {
        match lines.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(line) => read.push(line.expect("a line of text")),
            Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => {
                panic!("the main's output is still held after its exit ({how:?}): {read:?}")
            }
        }
    }
    read.join("\n")
}

#[test]
fn a_kept_main_holds_the_descriptors_a_watched_main_holds_and_the_keeper_holds_no_stream() {
    assert_eq!(
        the_mains_descriptors(&keeper()),
        the_mains_descriptors(&Containment::Watch(Why::Chosen))
    );
}

/// The fixture `tests/fixtures/fake-keeper.sh` in the keeper's place, misbehaving as `mode`, its
/// command's environment also given `env`.
fn faked_with(witness: &Witness, mode: &str, env: &[(&str, String)]) -> ReapedChild {
    let fake = Containment::Keeper(PathBuf::from(fixture_script("fake-keeper.sh")));
    let mut command = command(witness, &fake, "exit 0");
    command
        .command_mut()
        .env("RIGGER_TEST_KEEPER", mode)
        .envs(env.iter().map(|(name, value)| (name, value)));
    command.spawn().expect("spawn under the fake keeper")
}

/// The fake keeper misbehaving as `mode` ([`faked_with`]).
fn faked(witness: &Witness, mode: &str) -> ReapedChild {
    faked_with(witness, mode, &[])
}

#[test]
fn a_keeper_gone_without_the_mains_status_is_an_error_never_a_status() {
    let witness = Witness::new();
    let mut tree = faked(&witness, "gone");

    let read = loop {
        match tree.try_wait() {
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            read => break read,
        }
    };

    match read {
        Err(error) => assert!(error.to_string().contains("keeper"), "{error}"),
        Ok(status) => panic!("a keeper gone without its records answered {status:?}"),
    }
    assert!(tree.wait().is_err(), "a wait answers the same error");
}

#[test]
fn a_keeper_gone_while_its_main_runs_reports_the_main_skipped_and_signals_nothing() {
    let mut witness = Witness::new();
    let mut main = std::process::Command::new("sh")
        .arg("-c")
        .arg(Witness::script("up; held"))
        .envs(witness.env())
        .spawn()
        .expect("spawn the main the keeper leaves");
    assert!(witness.wait_up(1), "the main is up");
    let mut tree = faked_with(
        &witness,
        "lost",
        &[("RIGGER_TEST_MAIN", main.id().to_string())],
    );

    let report = tree.end();

    let skipped: Vec<u32> = report.skipped.iter().map(|leftover| leftover.pid).collect();
    assert_eq!(
        (report.residue, skipped, witness.read()),
        (vec![], vec![main.id()], Reading::Held),
        "the main the keeper left runs on, reported skipped"
    );
    assert_eq!(witness.release(), Reading::Ended);
    main.wait().expect("reap the main");
}

#[test]
fn a_keeper_that_does_not_end_its_tree_within_its_bound_is_ended_with_it_by_the_driver() {
    let mut witness = Witness::new();
    let mut tree = faked(&witness, "deaf");
    assert!(witness.wait_up(2), "the fake keeper and its holder are up");
    let started = Instant::now();

    tree.end();

    let elapsed = started.elapsed();
    assert_eq!(witness.read(), Reading::Ended);
    assert!(
        END_BOUND <= elapsed && elapsed < END_BOUND * 2,
        "the driver waits out the keeper's bound, then ends the tree: {elapsed:?}"
    );
}
