//! The real-driver observation point shared by the integration suites that drive a full
//! `conductor::run` through the REAL `driver::cli::Driver`: included by path
//! (`#[path = "common/real_driver_spy.rs"] mod real_driver_spy;`) by each suite that needs it.

use std::path::Path;
use std::sync::Mutex;

use serde_json::Value;

use rigger::conductor::{AgentDriver, AgentResult, Error, SpawnOpts};
use rigger::config::AgentDef;
use rigger::driver::cli;

/// A driver that delegates EVERY spawn to the REAL `driver::cli::Driver` (spec 65's
/// second injection site, unmodified production code) and records only the raw stdout
/// it produced. This is an OBSERVATION point, not a substitute implementation: every
/// subprocess this test drives is the actual `Command` the shipped driver builds,
/// spawned for real, with `SpawnOpts.env` applied by the real `Driver::spawn` - nothing
/// about the spawn itself is faked.
pub struct RealDriverSpy {
    inner: cli::Driver,
    outputs: Mutex<Vec<String>>,
}

impl RealDriverSpy {
    pub fn new(bin: &Path) -> Self {
        RealDriverSpy {
            inner: cli::Driver {
                bin: bin.to_string_lossy().into_owned(),
                ..cli::Driver::default()
            },
            outputs: Mutex::new(Vec::new()),
        }
    }

    pub fn outputs(&self) -> Vec<String> {
        self.outputs.lock().unwrap().clone()
    }
}

impl AgentDriver for RealDriverSpy {
    fn spawn(
        &self,
        agent: &AgentDef,
        prompt: &str,
        opts: &SpawnOpts,
        emit: &dyn Fn(&str, Value) -> Result<(), Error>,
    ) -> Result<AgentResult, Error> {
        let result = self.inner.spawn(agent, prompt, opts, emit)?;
        self.outputs.lock().unwrap().push(result.output.clone());
        Ok(result)
    }
}
