//! [`Domain`] — a validated, registrable-or-deeper host for the cookie allow-list.
//!
//! A [`Domain`] can only be constructed through [`Domain::explicit`], which enforces
//! that the value is a *registrable domain* (eTLD+1) or a deeper subdomain. Public
//! suffixes, bare TLDs, wildcards, and scheme-bearing strings are all rejected at the
//! boundary, so callers can never accidentally name a zone.

use crate::error::WristbandError;

/// A validated host that is a registrable domain (eTLD+1) or deeper subdomain.
///
/// The inner value is always lowercased. Construction is only possible via
/// [`Domain::explicit`], which enforces the public-suffix constraint.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Domain(String);

impl Domain {
    /// Construct a [`Domain`] from an arbitrary string, enforcing the safety invariant.
    ///
    /// # Errors
    ///
    /// Returns [`WristbandError::InvalidDomain`] if the input is empty, contains a
    /// wildcard (`*`), a scheme (`://`), a slash (`/`), whitespace, has a leading or
    /// trailing dot, has consecutive dots (empty labels), contains characters outside
    /// the hostname charset `[a-z0-9.-]`, or is a single label (no dot).
    ///
    /// Returns [`WristbandError::PublicSuffix`] if the input is a public suffix or eTLD
    /// (e.g. `com`, `co.uk`, `github.io`) — strings that would span an entire zone.
    pub fn explicit(input: &str) -> Result<Self, WristbandError> {
        let lower = input.to_lowercase();

        // Syntactic guards — return InvalidDomain
        if lower.is_empty() {
            return Err(WristbandError::InvalidDomain(lower));
        }
        if lower.contains('*') {
            return Err(WristbandError::InvalidDomain(lower));
        }
        if lower.contains("://") {
            return Err(WristbandError::InvalidDomain(lower));
        }
        if lower.contains('/') {
            return Err(WristbandError::InvalidDomain(lower));
        }
        if lower.chars().any(char::is_whitespace) {
            return Err(WristbandError::InvalidDomain(lower));
        }
        // Leading/trailing dot or consecutive dots (empty labels)
        if lower.starts_with('.') || lower.ends_with('.') || lower.contains("..") {
            return Err(WristbandError::InvalidDomain(lower));
        }
        // Charset guard: only [a-z0-9.-] are valid in a hostname after lowercasing.
        // Browser cookie hosts are ASCII/punycode; IDNs in punycode form pass this check.
        if !lower
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.' || b == b'-')
        {
            return Err(WristbandError::InvalidDomain(lower));
        }
        // Single-label (no dot) — no registrable domain possible
        if !lower.contains('.') {
            return Err(WristbandError::InvalidDomain(lower));
        }

        // Public Suffix List checks — return PublicSuffix for eTLD rejections
        // Reject if the input has no registrable domain at all
        if psl::domain_str(&lower).is_none() {
            return Err(WristbandError::PublicSuffix(lower));
        }
        // Reject if the input IS exactly its own public suffix (e.g. "co.uk", "github.io")
        if psl::suffix_str(&lower) == Some(lower.as_str()) {
            return Err(WristbandError::PublicSuffix(lower));
        }

        Ok(Self(lower))
    }

    /// Returns the validated host string (always lowercased).
    #[must_use]
    pub fn host(&self) -> &str {
        &self.0
    }
}

/// Returns `true` if `cookie_host` is in the allow-list or is a subdomain of an
/// allowed host.
///
/// Matching is **downward-only**: an allowed `astro.com` matches `astro.com` and
/// `*.astro.com`, but never a sibling zone, a parent zone, or a superstring that
/// merely ends in `astro.com` (e.g. `evilastro.com`).
///
/// A leading-dot cookie host (e.g. `.astro.com`) is normalised by stripping the dot
/// before comparison.
#[must_use]
pub fn host_matches(cookie_host: &str, allow: &[Domain]) -> bool {
    let normalised = cookie_host
        .to_lowercase()
        .trim_start_matches('.')
        .to_owned();

    allow.iter().any(|d| {
        let host = d.host();
        normalised == host || normalised.ends_with(&format!(".{host}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_accepts_registrable_and_subdomains() {
        assert!(Domain::explicit("astro.com").is_ok());
        assert!(Domain::explicit("clerk.astro.com").is_ok());
        assert!(Domain::explicit("app.foo.co.uk").is_ok()); // eTLD+1 under a multi-part suffix
    }

    #[test]
    fn explicit_rejects_wildcards_tlds_and_public_suffixes() {
        assert!(Domain::explicit("").is_err());
        assert!(Domain::explicit("*").is_err());
        assert!(Domain::explicit("*.astro.com").is_err());
        assert!(Domain::explicit("com").is_err()); // bare TLD
        assert!(Domain::explicit("co.uk").is_err()); // public suffix (eTLD)
        assert!(Domain::explicit("github.io").is_err()); // public suffix
        assert!(Domain::explicit("localhost").is_err()); // single label / no registrable domain
        assert!(Domain::explicit("http://astro.com").is_err());
    }

    #[test]
    fn explicit_rejects_malformed_hostnames() {
        assert!(Domain::explicit("astro.com.").is_err()); // trailing dot
        assert!(Domain::explicit(".astro.com").is_err()); // leading dot
        assert!(Domain::explicit("...astro.com").is_err()); // multiple leading dots
        assert!(Domain::explicit("astro..com").is_err()); // empty label
        assert!(Domain::explicit("astro\0.com").is_err()); // null byte
        assert!(Domain::explicit("astro com").is_err()); // whitespace
        assert!(Domain::explicit("astro.com").is_ok()); // still accepts valid
        assert!(Domain::explicit("clerk.astro.com").is_ok());
    }

    #[test]
    fn host_matches_rejects_prefix_lookalike() {
        let allow = [Domain::explicit("astro.com").unwrap()];
        assert!(!host_matches("xastro.com", &allow)); // boundary not fooled
        assert!(host_matches("x.astro.com", &allow)); // real subdomain ok
    }

    #[test]
    fn host_matching_is_suffix_safe_and_downward_only() {
        let allow = [Domain::explicit("astro.com").unwrap()];
        assert!(host_matches("astro.com", &allow));
        assert!(host_matches(".astro.com", &allow)); // leading-dot cookie host
        assert!(host_matches("www.astro.com", &allow));
        assert!(!host_matches("com", &allow)); // never matches up to the zone
        assert!(!host_matches("evilastro.com", &allow)); // not a subdomain
        assert!(!host_matches("other.com", &allow)); // sibling zone
        assert!(!host_matches("astro.com.evil.net", &allow));
    }
}
