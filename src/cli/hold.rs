//! `rigger hold --reason <text>` and `rigger release` (spec 105 RELEASE): the operator's own hold
//! on the current run, which no probe ends, and the release that ends any hold. Both append
//! through the run's folding store, on the compare-and-append every hold writer shares.

use super::*;

/// `rigger hold --reason <text>`: hold the current run for the operator, the text the hold's
/// action, an empty text included, carrying the open hold's latest model forward. Refuses,
/// appending nothing, a malformed argv and a run that is not live: none started, an empty store
/// included, or one that is done. The run is read before the append, so a run
/// turning done between the two takes the hold, which the next mint releases.
pub(crate) fn cmd_hold(args: &[String]) -> Res {
    let reason = match args {
        [flag, reason] if flag == "--reason" => reason,
        _ => return Err("hold: usage: rigger hold --reason <text>".into()),
    };
    on_the_run(|store| {
        let (events, run_id) = runscope::read::read_current_run(store, conductor::STREAM)?;
        if run_id.is_empty() {
            return Err("hold: no run is live: no run has started".into());
        }
        if ledger::project(&events)?.done() {
            return Err(format!("hold: no run is live: run {run_id} is done").into());
        }
        runscope_store::hold_by_operator(store, reason)?;
        println!("held run {run_id} for the operator; `rigger release` ends the hold");
        Ok(())
    })
}

/// `rigger release`: end the current run's open hold, whichever writer declared it, as a
/// `RunReleased` by the operator; a run with no hold open takes nothing and is told so.
pub(crate) fn cmd_release(args: &[String]) -> Res {
    if !args.is_empty() {
        return Err("release: usage: rigger release".into());
    }
    on_the_run(|store| {
        if runscope_store::release_hold(store, runscope::HOLD_BY_OPERATOR)? {
            println!("released the current run's hold");
        } else {
            println!("nothing to release: no hold is open");
        }
        Ok(())
    })
}

/// `act` over this project's run stream, read through untouched and appended through a folding
/// store, whose graph is opened only after the append (spec 101).
fn on_the_run(act: impl FnOnce(&dyn EventStore) -> Res) -> Res {
    let (loc, selection) = require_store_dir()?;
    let backend = resolve_store(&selection, &loc.file("events.db"))?;
    let store = Namespaced::new(backend.as_ref(), &loc.identity());
    act(&rigger::ingest::FoldingStore::new(
        &store,
        Some(|| loc.graph(&selection)),
        &stderr_line,
    ))
}
