# wristband

Consent-gated, domain-scoped reader for the user's own browser session cookies.

## Purpose

`wristband` lets a CLI tool (e.g. `blackmoon --grant-cookie-access`) read the
current user's own browser cookies for a specific, named set of domains, in
order to reuse a session the user already has open. It has a strict security
posture: cookies are filtered by an explicit allow-list **before** decryption,
and the library has no path that can return cookies for domains the caller did
not name.

## Enshrined invariants

These invariants are structural, not merely policy:

| # | Invariant |
|---|-----------|
| INV-1 | The only read entry point requires a non-empty `&[Domain]`. There is no "read all" function. |
| INV-1b | Only registrable domains (eTLD+1 or deeper) are accepted in the allow-list. Public suffixes and bare TLDs are rejected at parse time. Matching is subdomain-downward only — `example.com` matches `auth.example.com` but never `com` or any sibling registrable domain. |
| INV-2 | Host matching happens **before** any decryption. Rows whose host does not match the allow-list are never decrypted. |
| INV-3 | No public API returns an unfiltered cookie collection. |
| INV-4 | Consent is the responsibility of the caller (the CLI flag `--grant-cookie-access`), never of this library. The library accepts the allow-list and trusts the caller obtained consent. |
| INV-5 | Offline and read-only. The library makes no network calls. It reads cookie store files by copying them before opening (browsers lock the live file). |
| INV-6 | Conformance tests assert that every host in every returned cookie is a member of the caller's allow-list. |
| INV-7 | Library-pure. No `clap`, `anyhow`, interactive prompting, or logging of decrypted cookie material. `#![forbid(unsafe_code)]` holds unconditionally crate-wide — no unsafe code anywhere. Windows DPAPI decryption is performed via a PowerShell subprocess; keyring access via subprocess tools — no FFI. |

## Non-goals

See `SECURITY.md`.

## Known limitations

Cookies that cannot be decrypted are skipped, never returned. Notably, macOS
Chrome/Chromium App-Bound Encryption (`v11`) is not yet decrypted (only `v10`),
so a current Chrome may yield fewer cookies for an allowed domain. See
`SECURITY.md` → *Known limitations*.

## Usage

`wristband` is a library crate. It is not a standalone binary. The intended
consumer is `blackmoon`, which gates access behind `--grant-cookie-access` and
presents the operator with a domain summary before proceeding.

## License

CC0-1.0
