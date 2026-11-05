//! The keep-awake port (spec 105): a process driving a run asks the operating system to keep the
//! machine from sleeping for as long as it drives. [`SleepHold::acquire`] takes the hold and
//! returns the [`SleepGuard`] whose drop releases it; an adapter that cannot take one answers
//! [`Unavailable`] with its reason, which never fails the run. One adapter per OS family sits in
//! the process ring and, of the production code outside it, only the composition root names it. A
//! process that ends holding a guard loses the hold with it, since the OS ties each facility to
//! the process that took it, save a child that inherited the hold while the acquire ran: a caller
//! acquires before it starts any thread that spawns a child.

/// The keep-awake facility of the host the run drives on.
pub trait SleepHold {
    /// Keep the machine awake for `reason`, the text the OS shows for the hold, until the returned
    /// guard drops; or the reason the facility is unavailable.
    fn acquire(&self, reason: &str) -> Result<SleepGuard, Unavailable>;
}

/// A held keep-awake hold, released once, when the guard drops.
pub struct SleepGuard {
    release: Option<Box<dyn FnOnce()>>,
}

impl SleepGuard {
    /// A guard whose drop runs `release`, the adapter's step that gives the hold back.
    pub fn new(release: impl FnOnce() + 'static) -> Self {
        SleepGuard {
            release: Some(Box::new(release)),
        }
    }
}

impl Drop for SleepGuard {
    fn drop(&mut self) {
        if let Some(release) = self.release.take() {
            release();
        }
    }
}

/// Why the keep-awake facility is unavailable: the adapter's reason, which the driver prints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unavailable(pub String);

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    /// A guard runs its release exactly once, when it drops, and never while it stands.
    #[test]
    fn a_guard_releases_its_hold_once_as_it_drops() {
        let releases = Rc::new(Cell::new(0));
        let counted = Rc::clone(&releases);
        let guard = SleepGuard::new(move || counted.set(counted.get() + 1));
        assert_eq!(releases.get(), 0, "releases while the guard stands");
        drop(guard);
        assert_eq!(releases.get(), 1, "releases once the guard dropped");
    }
}
