---
id: fail-open-auth-guard
defect_class: fail-open-guard
planted: true
anchor: src/auth.rs
expected_verdict: reject
expected_tier: adversary
---
An authorization guard that fails OPEN: when token verification ERRORS (a
malformed token, an unreachable key server) it returns `true` and ADMITS the
request. A security guard must fail CLOSED - any error is a deny. The
`unwrap_or(true)` turns every verification failure into an allow.

```rust
/// Whether `token` authorizes access to `resource`.
pub fn is_authorized(token: &str, resource: &str) -> bool {
    // BUG: verification errors resolve to `true` (allow). A transient verifier
    // failure - or a deliberately malformed token that trips an error path -
    // is admitted. The guard must fail CLOSED: `.unwrap_or(false)`.
    verify(token, resource).unwrap_or(true)
}

fn verify(token: &str, resource: &str) -> Result<bool, VerifyError> {
    let claims = decode(token)?; // errors on a malformed/expired token
    Ok(claims.scopes.iter().any(|s| s == resource))
}
```
