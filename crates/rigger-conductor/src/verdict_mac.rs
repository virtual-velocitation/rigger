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
