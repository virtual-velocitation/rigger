//! The message a check fails with, for a test that drives the check's refusal arm and asserts
//! its exact text.

/// The message `call` fails with: its `panic!` or failed assertion's formatted text. Fails when
/// `call` returns instead, or fails with a payload that is not a formatted message.
pub fn failure_message<T>(call: impl FnOnce() -> T + std::panic::UnwindSafe) -> String {
    let failure = std::panic::catch_unwind(call)
        .err()
        .expect("the check reads the input instead of failing");
    failure
        .downcast_ref::<String>()
        .cloned()
        .expect("the check fails with a formatted message")
}
