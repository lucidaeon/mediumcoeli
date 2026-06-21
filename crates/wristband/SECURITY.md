# wristband — Security stance

## Legitimate use

`wristband` exists for one purpose: to let the **owner** of a browser profile
read their own session cookies for a **named, finite set of domains**, so that
a CLI tool can authenticate to services those cookies already grant access to.

## Non-goals

- **Not a credential harvester.** There is no "read all cookies" API. The
  allow-list is mandatory and cannot be empty.
- **Never reads cookies for domains the caller did not name.** The allow-list
  is checked before any decryption (INV-2). Rows that do not match are silently
  skipped, not returned, not logged.
- **Never exfiltrates.** The library makes no network calls (INV-5). It has no
  HTTP client dependency. Output is returned to the in-process caller only.
- **No TLD or wildcard globbing.** Only registrable domains (eTLD+1 or deeper)
  are accepted in the allow-list (INV-1b). Passing `com` or `.` is a hard error.
- **No all-cookies path.** There is no function, feature flag, or environment
  variable that bypasses the allow-list filter.

## Threat model

`wristband` is designed to be safe for the following deployment:

- The binary runs as the current user, on their own machine.
- The operator controls the allow-list (via CLI flag `--grant-cookie-access` in
  `blackmoon`); the library trusts that consent was obtained (INV-4).
- The cookie store is opened read-only, via a copy (INV-5), so the live browser
  file is never modified or locked by this tool.

`wristband` does **not** protect against:

- A malicious caller passing a broad allow-list (e.g. every domain the user has
  a cookie for). Domain selection is the caller's responsibility.
- Memory-scraping attacks against the calling process after cookies are
  decrypted and returned. Cookies live in memory only as long as the caller
  holds them.
- An attacker who already has read access to the user's home directory (who
  could read the cookie store directly).

## Invariants

| # | Statement |
|---|-----------|
| INV-1 | The only read entry point requires a non-empty `&[Domain]`. |
| INV-1b | Allow-list entries must be registrable domains (eTLD+1 or deeper). Public suffixes and bare TLDs are rejected. Matching is subdomain-downward only. |
| INV-2 | Host filtering happens before decryption. |
| INV-3 | No public API returns an unfiltered cookie collection. |
| INV-4 | Consent is the caller's responsibility; the library does not prompt. |
| INV-5 | Offline, read-only, copy-before-read. No network I/O. |
| INV-6 | Conformance tests prove: for every returned cookie, its host is a member of the input allow-list. |
| INV-7 | Library-pure: no clap, anyhow, prompting, or logging of cookie material. `#![forbid(unsafe_code)]` holds unconditionally crate-wide — no unsafe code anywhere. Windows DPAPI decryption is performed via a PowerShell subprocess; keyring access via subprocess tools — no FFI. |

## Known limitations

Decryption degrades gracefully (INV per the threat model): a cookie that
cannot be decrypted is skipped, never returned and never logged. Notably:

- **macOS App-Bound Encryption (`v11`)**: recent Chrome/Chromium builds wrap
  some cookies with App-Bound Encryption (the `v11` scheme) instead of the
  classic Keychain-derived key (`v10`). wristband currently decrypts only
  `v10` on macOS; `v11` rows are silently skipped. A user on a current Chrome
  may therefore see fewer cookies (or none) for an allowed domain. This is a
  capability gap, not a safety gap — no wrong data is returned.
- **Linux/Windows** decrypt both `v10` and `v11`; any individual cookie that
  fails to decrypt (bad keyring entry, GCM tag mismatch, etc.) is skipped.
