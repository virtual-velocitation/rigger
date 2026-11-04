//! The keep-awake adapter (spec 105): the host's facility behind
//! [`rigger_domain::sleep_hold::SleepHold`], which keeps the machine from sleeping while a run
//! drives. Only the composition root names [`OsSleepHold`].

use rigger_domain::sleep_hold::{SleepGuard, SleepHold, Unavailable};

/// The keep-awake facility of the operating system this binary runs on. No native adapter is
/// wired yet, so every acquire answers unavailable naming the OS.
pub struct OsSleepHold;

impl SleepHold for OsSleepHold {
    fn acquire(&self, _reason: &str) -> Result<SleepGuard, Unavailable> {
        Err(Unavailable(format!(
            "no keep-awake adapter for {}",
            std::env::consts::OS
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// With no native adapter wired, the host's acquire answers unavailable naming the OS.
    #[test]
    fn the_hosts_acquire_answers_unavailable_naming_the_os() {
        assert_eq!(
            OsSleepHold.acquire("rigger run is driving a run").err(),
            Some(Unavailable(format!(
                "no keep-awake adapter for {}",
                std::env::consts::OS
            )))
        );
    }
}
