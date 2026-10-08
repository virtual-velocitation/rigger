//! The run's LOGGED GENERATIONS (spec 107): a per-process memo of the log's latest generation of
//! each identity the run's ingest sink has asked about, so the sink asks the store's group lookup
//! for an identity once and answers its later batches from here.
//!
//! It is a memo of the log, never the ledger: the ledger is the log itself, and nothing here
//! outlives the process. Only the log side is remembered - the graph's side is read on every
//! batch, which is what lets a long-lived run restore an identity a rebuild left behind.
//!
//! An entry another process records stales the memo. Over a graph that does not owe its rebuild
//! that costs at most one re-recording: the entry this process then records sets the memo to
//! its own generation. Over a graph that owes its rebuild the sink answers from the memo alone, so
//! a batch at the generation the stale memo holds records nothing: an entry another process
//! recorded can leave a later generation unrecorded by this process.

use std::collections::HashMap;
use std::sync::Mutex;

/// The log's latest generation of each identity this process has learned, none where the log
/// held no recording of it.
#[derive(Default)]
pub(crate) struct LoggedGenerations {
    latest: Mutex<HashMap<String, Option<String>>>,
}

impl LoggedGenerations {
    /// The log's latest generation of `identity`: the memo's answer when this process already
    /// learned one, else `lookup`'s, remembered when it answers - an identity the log holds no
    /// recording of included - and not when it fails, so a failed lookup is asked again.
    ///
    /// The memo is not held across `lookup`: two stages asking about one identity at once may
    /// both ask the store.
    pub(crate) fn latest<E>(
        &self,
        identity: &str,
        lookup: impl FnOnce() -> Result<Option<String>, E>,
    ) -> Result<Option<String>, E> {
        let known = self.latest.lock().unwrap().get(identity).cloned();
        if let Some(known) = known {
            return Ok(known);
        }
        let looked_up = lookup()?;
        self.learn(identity, looked_up.clone());
        Ok(looked_up)
    }

    /// Take `generation` as the log's latest of `identity`: the generation of the entry this
    /// process just recorded for it.
    pub(crate) fn record(&self, identity: &str, generation: &str) {
        self.learn(identity, Some(generation.to_string()));
    }

    fn learn(&self, identity: &str, generation: Option<String>) {
        self.latest
            .lock()
            .unwrap()
            .insert(identity.to_string(), generation);
    }
}
