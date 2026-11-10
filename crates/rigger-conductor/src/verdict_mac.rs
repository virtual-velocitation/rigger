//! The per-run verdict key (signed verdicts): a gate verdict counts only when the driver can
//! verify that it ran the gate itself. Each driver process makes one random key when it starts
//! and holds it only in memory, never on disk, in the environment, in an argument, in a log or in
//! a child, so a restart makes a new key. Every gate verdict the driver records carries an
//! HMAC-SHA256 under that key over the fields that decide what the verdict proves: the replay key,
//! the run, the unit, the gate, the SHA-256 of the gate's command, the gated commit, pass or fail,
//! and the input digest. A row whose MAC verifies under the live key was recorded by this driver
//! process; any other row (unsigned, edited, or signed by an earlier process) proves nothing and
//! its gate runs again.

use ring::{digest, hmac, rand};

use rigger_domain::agent::Error;

/// The live driver process's verdict key. It has no serialized form and its `Debug` shows no
/// bytes, so it cannot reach a row, a file or a child by accident.
pub struct VerdictKey(hmac::Key);

impl std::fmt::Debug for VerdictKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("VerdictKey(<redacted>)")
    }
}

/// The fields a verdict's MAC covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VerdictFields<'a> {
    pub replay_key: &'a str,
    pub run: &'a str,
    pub unit: &'a str,
    pub gate: &'a str,
    pub command_digest: &'a str,
    pub commit: &'a str,
    pub pass: bool,
    pub input_digest: &'a str,
}

impl VerdictFields<'_> {
    /// The signed message: the fields as one JSON array, so no field can bleed into the next.
    fn message(&self) -> Vec<u8> {
        serde_json::to_vec(&(
            self.replay_key,
            self.run,
            self.unit,
            self.gate,
            self.command_digest,
            self.commit,
            self.pass,
            self.input_digest,
        ))
        .unwrap_or_default()
    }
}

impl VerdictKey {
    /// A fresh key from the OS random source: 32 random bytes, zeroed once the key holds them.
    pub fn generate() -> Result<VerdictKey, Error> {
        let mut bytes = [0u8; 32];
        rand::SecureRandom::fill(&rand::SystemRandom::new(), &mut bytes)
            .map_err(|_| Error("verdict key: the OS random source failed".into()))?;
        let key = VerdictKey(hmac::Key::new(hmac::HMAC_SHA256, &bytes));
        bytes.fill(0);
        Ok(key)
    }

    /// A key from known bytes, for tests that sign a row as a given driver would.
    #[cfg(test)]
    pub(crate) fn from_bytes(bytes: &[u8]) -> VerdictKey {
        VerdictKey(hmac::Key::new(hmac::HMAC_SHA256, bytes))
    }

    /// The MAC of `fields` under this key, as lowercase hex.
    pub fn sign(&self, fields: &VerdictFields) -> String {
        hex(hmac::sign(&self.0, &fields.message()).as_ref())
    }

    /// Whether `mac` is this key's MAC of `fields`, compared in constant time.
    pub fn verifies(&self, fields: &VerdictFields, mac: &str) -> bool {
        unhex(mac).is_some_and(|tag| hmac::verify(&self.0, &fields.message(), &tag).is_ok())
    }
}

/// The SHA-256 of a gate's command, as lowercase hex.
pub fn command_digest(command: &str) -> String {
    hex(digest::digest(&digest::SHA256, command.as_bytes()).as_ref())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(text: &str) -> Option<Vec<u8>> {
    if text.is_empty() || text.len() % 2 != 0 {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(text.get(i..i + 2)?, 16).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &[u8] = b"verdict-mac-test-key-0123456789a";

    fn fields() -> VerdictFields<'static> {
        VerdictFields {
            replay_key: "s/gate:g#0",
            run: "run-1",
            unit: "s",
            gate: "g",
            command_digest: "cd",
            commit: "abc123",
            pass: true,
            input_digest: "0123456789abcdef:abc123",
        }
    }

    /// A MAC VERIFIES ONLY ITS OWN FIELDS UNDER ITS OWN KEY: signed under one key, the MAC
    /// verifies the fields it signed under that key; any one field changed, another key, or a
    /// MAC that is empty or not hex fails.
    #[test]
    fn a_mac_verifies_only_the_fields_it_signed_under_the_key_that_signed_them() {
        let key = VerdictKey::from_bytes(KEY);
        let mac = key.sign(&fields());
        type Edit = fn(&mut VerdictFields<'static>);
        let cases: [(&str, Edit, bool); 9] = [
            ("unchanged", |_| {}, true),
            ("replay key", |f| f.replay_key = "s/gate:g#1", false),
            ("run", |f| f.run = "run-2", false),
            ("unit", |f| f.unit = "t", false),
            ("gate", |f| f.gate = "h", false),
            ("command digest", |f| f.command_digest = "other", false),
            ("commit", |f| f.commit = "def456", false),
            ("pass", |f| f.pass = false, false),
            ("input digest", |f| f.input_digest = "", false),
        ];
        for (why, edit, verifies) in cases {
            let mut f = fields();
            edit(&mut f);
            assert_eq!(key.verifies(&f, &mac), verifies, "{why}");
        }
        let other = VerdictKey::from_bytes(b"another-key");
        assert!(!other.verifies(&fields(), &mac), "another key");
        assert!(!key.verifies(&fields(), ""), "an empty MAC");
        assert!(!key.verifies(&fields(), "not hex"), "a MAC that is not hex");
    }

    /// EVERY DRIVER PROCESS MAKES ITS OWN KEY: two generated keys sign the same fields
    /// differently, and neither verifies the other's MAC; a key's Debug shows no bytes.
    #[test]
    fn two_generated_keys_sign_differently_and_a_keys_debug_is_redacted() {
        let a = VerdictKey::generate().unwrap();
        let b = VerdictKey::generate().unwrap();
        let (mac_a, mac_b) = (a.sign(&fields()), b.sign(&fields()));
        assert_ne!(mac_a, mac_b);
        assert!(a.verifies(&fields(), &mac_a) && !b.verifies(&fields(), &mac_a));
        assert_eq!(format!("{a:?}"), "VerdictKey(<redacted>)");
    }

    /// THE COMMAND DIGEST IS SHA-256: the hex of the command's SHA-256.
    #[test]
    fn the_command_digest_is_the_sha256_hex_of_the_command() {
        assert_eq!(
            command_digest("true"),
            "b5bea41b6c623f7c09f1bf24dcae58ebab3c0cdd90ad966bc43a45b44867e12b"
        );
    }
}
