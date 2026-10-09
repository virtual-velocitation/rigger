//! The run's LOGGED GENERATIONS (spec 107): a per-process memo of the log's latest generation of
//! each identity the run's ingest sink has asked about, so the sink asks the store's group lookup
//! for an identity once and answers its later batches from here.
//!
//! It is a memo of the log, never the ledger: the ledger is the log itself, and nothing here
//! outlives the process. Only the log side is remembered - the graph's side is read on every
//! batch, which is what lets a long-lived run restore an identity a rebuild left behind.
//!
//! THE MEMO CARRIES THE PROOF OF ITS OWN CURRENCY. A memo answer suppresses a recording only when
//! the log cannot have advanced for its identity since the answer was taken, so the memo holds
//! the run stream's LEDGER HEAD it is current at - the revision of the stream's newest ledger
//! entry, a head read and never a replay - and answers only while the store's head is still that
//! one. No store appends a derived event, so a ledger entry is the only recording that moves an
//! identity's latest generation, and a head that has not moved proves no identity moved. A head
//! that moved - another process recorded an entry - empties the memo, so every identity is asked
//! of the group lookup again; a head that cannot be read leaves the memo unused, and the lookup
//! answers. An entry this process appends moves the head too: the memo follows it only when the
//! stream's entries above the head it held are that entry alone.

use crate::eventstore::{Event, Position, Revision};
use std::collections::HashMap;
use std::sync::Mutex;

/// The log's latest generation of each identity this process has learned, none where the log
/// held no recording of it, and the ledger head those answers are current at.
#[derive(Default)]
pub(crate) struct LoggedGenerations {
    memo: Mutex<Memo>,
}

#[derive(Default)]
struct Memo {
    /// The revision of the run stream's newest ledger entry every answer below is current at -
    /// `Some(None)` for a stream holding none - or `None` before the head is first read.
    head: Option<Option<Revision>>,
    latest: HashMap<String, Option<String>>,
}

impl Memo {
    /// Take `head` as the head the memo is current at, emptying it when that is not the head it
    /// held.
    fn current_at(&mut self, head: Option<Revision>) {
        if self.head != Some(head) {
            self.latest.clear();
            self.head = Some(head);
        }
    }

    /// Forget every answer and the head they were current at.
    fn forget(&mut self) {
        self.latest.clear();
        self.head = None;
    }
}

impl LoggedGenerations {
    /// The log's latest generation of `identity`: the memo's answer when this process already
    /// learned one and the ledger head `head` reads is still the one it was learned at, else
    /// `lookup`'s, remembered when it answers - an identity the log holds no recording of
    /// included - and not when it fails, so a failed lookup is asked again.
    ///
    /// The head is read BEFORE the lookup, so an entry appended between the two moves the head
    /// past the one the answer is remembered at, and the next batch asks again. A head that
    /// cannot be read answers nothing from the memo and remembers nothing: the lookup answers.
    ///
    /// The memo is not held across `head` or `lookup`: two stages asking about one identity at
    /// once may both ask the store.
    pub(crate) fn latest<E>(
        &self,
        identity: &str,
        head: impl FnOnce() -> Result<Option<Revision>, E>,
        lookup: impl FnOnce() -> Result<Option<String>, E>,
    ) -> Result<Option<String>, E> {
        let Ok(head) = head() else {
            return lookup();
        };
        {
            let mut memo = self.memo.lock().unwrap();
            memo.current_at(head);
            if let Some(known) = memo.latest.get(identity) {
                return Ok(known.clone());
            }
        }
        let looked_up = lookup()?;
        let mut memo = self.memo.lock().unwrap();
        if memo.head == Some(head) {
            memo.latest.insert(identity.to_string(), looked_up.clone());
        }
        Ok(looked_up)
    }

    /// Take `generation` as the log's latest of `identity`: the generation of the entry this
    /// process just appended for it at `placed`, the position the store issued it (`None` when
    /// the store placed nothing).
    ///
    /// `above` reads the run stream's ledger entries from a revision on: handed the revision
    /// past the memo's head, it answers what was appended since. When that is this entry alone
    /// the memo is current at the entry's revision and remembers its generation; anything else -
    /// another process's entry, a failed read, an entry not placed - empties the memo, so every
    /// identity is asked of the group lookup again.
    pub(crate) fn record<E>(
        &self,
        identity: &str,
        generation: &str,
        placed: Option<Position>,
        above: impl FnOnce(Revision) -> Result<Vec<Event>, E>,
    ) {
        let held = self.memo.lock().unwrap().head;
        let appended = match (held, placed) {
            (Some(head), Some(_)) => above(head.map_or(0, |revision| revision + 1)).ok(),
            _ => None,
        };
        let mut memo = self.memo.lock().unwrap();
        match appended.as_deref() {
            Some([only]) if Some(only.position) == placed && memo.head == held => {
                memo.head = Some(Some(only.revision));
                memo.latest
                    .insert(identity.to_string(), Some(generation.to_string()));
            }
            _ => memo.forget(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::LoggedGenerations;
    use crate::eventstore::{Event, Revision};
    use std::cell::Cell;

    /// A ledger entry the store placed at `position`, revision `revision`.
    fn placed(position: u64, revision: Revision) -> Event {
        let mut entry = Event::new("GenerationIngested", Vec::new());
        entry.position = position;
        entry.revision = revision;
        entry
    }

    /// Ask `memo` for `gc/a.rs` with the ledger head answering `head`, counting the lookups in
    /// `asked`; the lookup answers `h1`.
    fn ask(
        memo: &LoggedGenerations,
        head: Result<Option<Revision>, ()>,
        asked: &Cell<usize>,
    ) -> Option<String> {
        memo.latest(
            "gc/a.rs",
            || head,
            || {
                asked.set(asked.get() + 1);
                Ok(Some("h1".to_string()))
            },
        )
        .unwrap()
    }

    /// A head that stands answers from the memo; a head that moved, or one that cannot be read,
    /// sends the identity to the lookup again, and an unread head remembers nothing.
    #[test]
    fn the_memo_answers_only_while_the_ledger_head_it_was_taken_at_stands() {
        let memo = LoggedGenerations::default();
        let asked = Cell::new(0);

        let answers = [
            ask(&memo, Ok(Some(3)), &asked),
            ask(&memo, Ok(Some(3)), &asked),
            ask(&memo, Ok(Some(4)), &asked),
            ask(&memo, Err(()), &asked),
            ask(&memo, Err(()), &asked),
            ask(&memo, Ok(Some(4)), &asked),
        ];

        assert_eq!(
            answers,
            [
                Some("h1".to_string()),
                Some("h1".to_string()),
                Some("h1".to_string()),
                Some("h1".to_string()),
                Some("h1".to_string()),
                Some("h1".to_string())
            ]
        );
        assert_eq!(
            asked.get(),
            4,
            "asked at heads 3, 4, unread, unread; remembered at 3 and 4"
        );
    }

    /// An entry this process appended is followed when the entries above the memo's head are it
    /// alone, so the memo answers its generation at the entry's revision; another entry among
    /// them, or a tail that cannot be read, empties the memo.
    #[test]
    fn the_memo_follows_its_own_entry_only_when_no_other_entry_was_appended() {
        let followed = |above: Result<Vec<Event>, ()>, head_after: Revision| {
            let memo = LoggedGenerations::default();
            let asked = Cell::new(0);
            ask(&memo, Ok(Some(3)), &asked);
            memo.record("gc/a.rs", "h2", Some(40), |from| {
                assert_eq!(
                    from, 4,
                    "the tail is read from the revision past the memo's head"
                );
                above
            });
            (ask(&memo, Ok(Some(head_after)), &asked), asked.get())
        };

        assert_eq!(
            [
                followed(Ok(vec![placed(40, 5)]), 5),
                followed(Ok(vec![placed(39, 4), placed(40, 5)]), 5),
                followed(Err(()), 5),
            ],
            [
                (Some("h2".to_string()), 1),
                (Some("h1".to_string()), 2),
                (Some("h1".to_string()), 2),
            ]
        );
    }
}
