//! Table B2: a tree held by the Linux keeper, the built binary re-run as `rigger-keeper`
//! (`crates/rigger-process/src/child_tree/linux.rs`). The handle answers the main's pid and its
//! exact status; an end, a drop and a TERM to the keeper each leave nothing the tree started
//! running; a release after the main's exit leaves what the main left running; the keeper holds
//! none of the session's streams; and a keeper that is gone or deaf is reported, never trusted.
//! Every process a row starts is read through the FIFO witness (`common::witness`), and the test
//! signals none of them.

use crate::common::fixtures::fixture_script;
use crate::common::rigger_bin;
use crate::common::witness::{Reading, Witness};
use rigger::child_tree::{line_channel, Containment, EndReport, ReapedChild, TreeCommand, Why};
use rigger::os_process;
use rigger::reap::END_BOUND;
use std::io::{self, BufRead, BufReader};
use std::path::PathBuf;
use std::process::Stdio;
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

/// The kept fixture `body`, ended by the handle's end once its main is up when `end` is set and
/// waited on otherwise, answers the main's exit `code` and ending `signal`, and leaves nothing
/// running.
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
        tree.wait().expect("wait on the main")
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
}

/// The kept fixture `body`, with `up` of its processes up and then `before` done to it, is ended
/// by the handle's end, which leaves nothing running and reports nothing.
fn ends_the_tree(body: &str, up: usize, before: fn(&mut ReapedChild, &mut Witness)) {
    let mut witness = Witness::new();
    let mut tree = kept(&witness, body);
    assert!(witness.wait_up(up), "the fixture's {up} processes are up");
    before(&mut tree, &mut witness);

    let report = tree.end();

    assert_eq!(
        (witness.read(), report),
        (Reading::Ended, EndReport::default())
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

/// Closes the main's input.
fn after_the_input_closes(tree: &mut ReapedChild, _witness: &mut Witness) {
    drop(tree.take_stdin());
}

rigger::test_cases! {
    a_kept_end_ends_a_running_tree:
        ends_the_tree("holder; up; held", 2, |_, _| {});
    a_kept_end_ends_what_an_exited_main_left:
        ends_the_tree("holder; up", 2, after_the_main_exits);
    a_kept_end_ends_what_the_main_starts_as_it_exits_at_the_end_of_its_input:
        ends_the_tree("up; read -r unused; holder; exit 0", 1, after_the_input_closes);
    a_kept_end_ends_a_process_started_in_another_directory:
        ends_the_tree("cd / && holder; up; held", 2, |_, _| {});
    a_kept_end_ends_a_process_in_a_session_of_its_own:
        ends_the_tree("setsid sh -c 'printf u >&9; read -r unused <&8' & up; held", 2, |_, _| {});
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
fn a_kept_tree_released_after_its_main_exits_leaves_what_the_main_left_running() {
    let mut witness = Witness::new();
    let mut tree = kept(&witness, "holder; up");
    assert!(witness.wait_up(2), "the holder and the main are up");
    tree.wait().expect("wait on the main");

    tree.release();

    assert_eq!(
        witness.read(),
        Reading::Held,
        "the holder the main left runs on"
    );
    assert_eq!(witness.release(), Reading::Ended);
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
/// itself, once its output reaches its end with the tree neither ended nor released: no process
/// besides the main holds the main's output.
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

/// The fixture `tests/fixtures/fake-keeper.sh` in the keeper's place, misbehaving as `mode`.
fn faked(witness: &Witness, mode: &str) -> ReapedChild {
    let fake = Containment::Keeper(PathBuf::from(fixture_script("fake-keeper.sh")));
    let mut command = command(witness, &fake, "exit 0");
    command.command_mut().env("RIGGER_TEST_KEEPER", mode);
    command.spawn().expect("spawn under the fake keeper")
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
