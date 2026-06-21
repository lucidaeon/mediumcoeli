//! The filter-before-decrypt gate — the single, only producer of [`Cookie`]
//! values.
//!
//! # Invariant (INV-2)
//!
//! Host matching is performed **before** the decrypt closure is ever called.
//! A row whose host is not in the allow-list is silently skipped; the closure
//! is **never** invoked for it. This is what prevents a compromised or
//! malformed cookie database from exfiltrating the plaintext of cookies the
//! caller did not ask for.
//!
//! # Why a closure?
//!
//! Chromium-family browsers use OS-level key material to decrypt cookie values.
//! Accepting a caller-supplied `decrypt` closure lets the gate remain pure and
//! testable without any platform dependency. The caller (backend) owns the key;
//! the gate owns the allow-list filter ordering.

use crate::cookie::{Cookie, RawRow};
use crate::domain::{Domain, host_matches};

/// Apply the allow-list filter and — only for matching rows — attempt
/// decryption.
// Future backends will call gate(); allow dead_code until they land.
#[allow(dead_code)]
///
/// For each row in `rows`:
/// 1. If the row's host does **not** match any entry in `allow`, skip it.
///    The `decrypt` closure is **never called** for rejected rows (INV-2).
/// 2. Otherwise, resolve the plaintext value: use `row.plaintext_value` if
///    present, otherwise call `decrypt(&row)`.
/// 3. If the value resolves to `None` (decryption failed), skip the row.
/// 4. Construct a [`Cookie`] and add it to the output.
///
/// This is the **only** function that constructs [`Cookie`] values (INV-3).
pub(crate) fn gate<F>(rows: Vec<RawRow>, allow: &[Domain], decrypt: F) -> Vec<Cookie>
where
    F: Fn(&RawRow) -> Option<String>,
{
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        // INV-2: filter FIRST, decrypt NEVER for non-matching rows.
        if !host_matches(&row.host, allow) {
            continue;
        }
        // Resolve value: prefer plaintext (Firefox/Safari), else decrypt.
        let value = match row.plaintext_value.clone() {
            Some(v) => v,
            None => match decrypt(&row) {
                Some(v) => v,
                None => continue, // decryption failed — skip row
            },
        };
        out.push(Cookie {
            host: row.host.trim_start_matches('.').to_lowercase(),
            name: row.name,
            value,
            path: row.path,
            secure: row.secure,
            expires_unix: row.expires_unix,
        });
    }
    out
}

// ---------------------------------------------------------------------------
// Internal tests — the gate spy + proptest
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    /// Build a minimal `RawRow` with an encrypted value.
    fn raw(host: &str, name: &str, enc: &[u8]) -> RawRow {
        RawRow {
            host: host.to_owned(),
            name: name.to_owned(),
            path: "/".to_owned(),
            secure: false,
            expires_unix: None,
            encrypted_value: enc.to_vec(),
            plaintext_value: None,
        }
    }

    // -----------------------------------------------------------------------
    // INV-2 / INV-3 / INV-6: decrypt spy
    // -----------------------------------------------------------------------

    #[test]
    fn gate_never_decrypts_non_allowed_hosts_and_output_is_subset() {
        let allow = [Domain::explicit("astro.com").unwrap()];
        let rows = vec![
            raw("astro.com", "cid", b"ENC"),      // allowed, encrypted
            raw("evil.net", "steal", b"ENC"),     // NOT allowed
            raw("www.astro.com", "sess", b"ENC"), // allowed subdomain
        ];
        let calls = Cell::new(0u32);
        let seen_evil = Cell::new(false);
        let out = gate(rows, &allow, |r| {
            calls.set(calls.get() + 1);
            if r.host.contains("evil") {
                seen_evil.set(true);
            }
            Some(format!("dec:{}", r.name))
        });
        // INV-2: decrypt called only for the two allowed rows, never for evil.net
        assert_eq!(calls.get(), 2);
        assert!(
            !seen_evil.get(),
            "decrypt must never run on a non-allowed host"
        );
        // INV-3/INV-6: every output host is within the allow-list
        assert!(
            out.iter().all(|c| crate::host_matches(&c.host, &allow)),
            "output host outside allow-list"
        );
        assert!(
            out.iter().all(|c| c.host != "evil.net"),
            "evil.net appeared in output"
        );
    }

    #[test]
    fn gate_plaintext_rows_bypass_decrypt_closure() {
        let allow = [Domain::explicit("example.com").unwrap()];
        let row = RawRow {
            host: "example.com".to_owned(),
            name: "token".to_owned(),
            path: "/".to_owned(),
            secure: true,
            expires_unix: Some(9_999_999_999),
            encrypted_value: vec![],
            plaintext_value: Some("plainval".to_owned()),
        };
        let decrypt_called = Cell::new(false);
        let out = gate(vec![row], &allow, |_| {
            decrypt_called.set(true);
            Some("should-not-be-used".to_owned())
        });
        assert!(
            !decrypt_called.get(),
            "decrypt must not be called for plaintext rows"
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].value, "plainval");
    }

    #[test]
    fn gate_skips_rows_where_decrypt_returns_none() {
        let allow = [Domain::explicit("example.com").unwrap()];
        let rows = vec![
            raw("example.com", "good", b"OK"),
            raw("example.com", "bad", b"FAIL"),
        ];
        let out = gate(rows, &allow, |r| {
            if r.name == "good" {
                Some("value".to_owned())
            } else {
                None // simulate decryption failure
            }
        });
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].name, "good");
    }

    // -----------------------------------------------------------------------
    // INV-6 property test: output hosts ⊆ allow-list for any input
    // -----------------------------------------------------------------------

    use proptest::prelude::*;

    proptest! {
        #[test]
        fn output_hosts_always_within_allow_list(
            // Pick 0–4 rows from a small fixed set of hosts
            row_hosts in proptest::collection::vec(
                proptest::sample::select(vec![
                    "astro.com",
                    "www.astro.com",
                    "evil.net",
                    "sub.evil.net",
                    "example.org",
                    "foo.example.org",
                    "other.io",
                ]),
                0..=5,
            ),
            // Pick 1–3 allowed domains from a subset (always non-empty)
            allow_hosts in proptest::collection::vec(
                proptest::sample::select(vec![
                    "astro.com",
                    "example.org",
                    "other.io",
                ]),
                1..=3,
            ),
        ) {
            let allow: Vec<Domain> = allow_hosts
                .into_iter()
                .map(|h: &str| Domain::explicit(h).unwrap())
                .collect();

            let rows: Vec<RawRow> = row_hosts
                .into_iter()
                .enumerate()
                .map(|(i, h)| raw(h, &format!("n{i}"), b"V"))
                .collect();

            let out = gate(rows, &allow, |r| Some(format!("v:{}", r.name)));

            for cookie in &out {
                prop_assert!(
                    crate::host_matches(&cookie.host, &allow),
                    "cookie host {} not in allow-list",
                    cookie.host
                );
            }
        }
    }
}
