//! The Linux keeper: a process that holds a tree as its child subreaper, so every orphan of the tree
//! is its child, and ends the tree when its driver asks, until no child is left.
//!
//! The keeper and its driver speak in [`Record`]s, one a line, on the keeper's status pipe.

use crate::reap::Leftover;

/// One line the keeper writes on its status pipe.
#[cfg_attr(not(test), allow(dead_code))] // read by the keeper and its driver once they exist
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Record {
    /// The keeper spawned the tree's main, whose pid this is.
    Spawned(u32),
    /// The keeper could not spawn the main, for this OS error number.
    Failed(i32),
    /// The keeper cannot hold a tree on this system, for this OS error number.
    Unavailable(i32),
    /// The keeper's probe holds a tree on this system.
    Probed,
    /// The main exited, with this raw wait status.
    Exited(i32),
    /// A process the keeper's end signalled is still running past its bound.
    Residue(Leftover),
    /// A process the keeper's end found and never signalled.
    Skipped(Leftover),
    /// The keeper's end left no process of the tree running but those its residue and skipped
    /// records named.
    Ended,
}

#[cfg_attr(not(test), allow(dead_code))] // read by the keeper and its driver once they exist
impl Record {
    /// The record as one line, without its line end: its tag, then its fields, each after one
    /// space. A leftover's program is its last field, everything after its pid's space, with each
    /// line end in it written as a space.
    pub(super) fn encode(&self) -> String {
        match self {
            Record::Spawned(pid) => format!("spawned {pid}"),
            Record::Failed(errno) => format!("failed {errno}"),
            Record::Unavailable(errno) => format!("unavailable {errno}"),
            Record::Probed => "ok".to_string(),
            Record::Exited(raw) => format!("exited {raw}"),
            Record::Residue(left) => format!("residue {}", Record::fields(left)),
            Record::Skipped(left) => format!("skipped {}", Record::fields(left)),
            Record::Ended => "ended".to_string(),
        }
    }

    /// The record `line` encodes, without its line end; `None` for a line that is no record.
    pub(super) fn decode(line: &str) -> Option<Record> {
        let (tag, rest) = match line.split_once(' ') {
            Some((tag, rest)) => (tag, Some(rest)),
            None => (line, None),
        };
        match (tag, rest) {
            ("ok", None) => Some(Record::Probed),
            ("ended", None) => Some(Record::Ended),
            ("spawned", Some(pid)) => digits(pid).map(Record::Spawned),
            ("failed", Some(errno)) => errno.parse().ok().map(Record::Failed),
            ("unavailable", Some(errno)) => errno.parse().ok().map(Record::Unavailable),
            ("exited", Some(raw)) => raw.parse().ok().map(Record::Exited),
            ("residue", Some(rest)) => Record::leftover(rest).map(Record::Residue),
            ("skipped", Some(rest)) => Record::leftover(rest).map(Record::Skipped),
            _ => None,
        }
    }

    /// A leftover's fields: its pid, a space, and its program on one line.
    fn fields(left: &Leftover) -> String {
        format!("{} {}", left.pid, left.program.replace(['\n', '\r'], " "))
    }

    /// The leftover `fields` names ([`Record::fields`]); `None` unless they are a pid, a space
    /// and a program.
    fn leftover(fields: &str) -> Option<Leftover> {
        let (pid, program) = fields.split_once(' ')?;
        Some(Leftover {
            pid: digits(pid)?,
            program: program.to_string(),
            stopped: false,
        })
    }
}

/// The number `field` names in decimal digits alone, no sign.
#[cfg_attr(not(test), allow(dead_code))] // read by the keeper and its driver once they exist
fn digits(field: &str) -> Option<u32> {
    if field.is_empty() || !field.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    field.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `record` decodes from its own line.
    fn round_trips(record: Record) {
        assert_eq!(Record::decode(&record.encode()), Some(record));
    }

    /// A leftover named `program`, as the keeper reports one.
    fn leftover(pid: u32, program: &str) -> Leftover {
        Leftover {
            pid,
            program: program.to_string(),
            stopped: false,
        }
    }

    crate::test_cases! {
        a_spawn_record_round_trips: round_trips(Record::Spawned(4242));
        a_spawn_failure_round_trips: round_trips(Record::Failed(2));
        an_unavailable_record_round_trips: round_trips(Record::Unavailable(1));
        a_probe_answer_round_trips: round_trips(Record::Probed);
        an_exit_status_round_trips: round_trips(Record::Exited(15));
        a_residue_record_round_trips: round_trips(Record::Residue(leftover(7, "sh -c sleep 30")));
        a_skipped_record_round_trips: round_trips(Record::Skipped(leftover(9, " sh  -c x ")));
        a_leftover_with_no_program_round_trips: round_trips(Record::Residue(leftover(11, "")));
        an_end_record_round_trips: round_trips(Record::Ended);
    }

    #[test]
    fn a_program_with_line_ends_is_written_on_one_line() {
        let line = Record::Residue(leftover(7, "sh -c 'a\nb\rc'")).encode();

        assert!(!line.contains(['\n', '\r']), "one line: {line:?}");
        assert_eq!(
            Record::decode(&line),
            Some(Record::Residue(leftover(7, "sh -c 'a b c'")))
        );
    }

    /// `line` decodes to no record.
    fn refused(line: &str) {
        assert_eq!(Record::decode(line), None, "{line:?}");
    }

    crate::test_cases! {
        an_empty_line_is_no_record: refused("");
        an_unknown_tag_is_no_record: refused("launched 12");
        a_spawn_record_with_no_pid_is_no_record: refused("spawned");
        a_spawn_record_with_a_negative_pid_is_no_record: refused("spawned -1");
        a_spawn_record_with_a_word_for_its_pid_is_no_record: refused("spawned twelve");
        a_spawn_record_with_two_pids_is_no_record: refused("spawned 12 13");
        an_exit_record_with_no_status_is_no_record: refused("exited");
        a_probe_answer_with_more_is_no_record: refused("ok now");
        an_end_record_with_more_is_no_record: refused("ended 1");
        a_residue_record_with_no_pid_is_no_record: refused("residue");
        a_residue_record_with_a_word_for_its_pid_is_no_record: refused("residue sh -c x");
        a_tag_in_capitals_is_no_record: refused("SPAWNED 12");
    }
}
