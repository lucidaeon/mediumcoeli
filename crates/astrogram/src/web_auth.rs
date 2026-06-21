//! Shared web-authentication helpers: HTTP-401 detection and the generic
//! credential fall-through combinator used by every web module.

use reqwest::StatusCode;

/// True when a `reqwest` error carries an HTTP **401** or **403** status —
/// the signal that a credential was rejected and the caller should fall
/// through to the next one. Returns `false` for transport errors (timeouts,
/// DNS, connection refused) and all other statuses.
#[must_use]
pub fn is_unauthorized(e: &reqwest::Error) -> bool {
    matches!(
        e.status(),
        Some(StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN)
    )
}

/// Outcome when no attempt in a credential chain succeeds.
#[derive(Debug)]
pub enum ChainError<E> {
    /// The chain was empty — no credentials were supplied at all.
    Empty,
    /// Every attempt failed; carries the **last** error encountered (either
    /// the last auth failure after falling through, or the first non-auth
    /// failure that stopped the chain).
    AllFailed(E),
}

impl<E: std::fmt::Display> std::fmt::Display for ChainError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => f.write_str("no credentials supplied"),
            Self::AllFailed(e) => write!(f, "all credentials failed: {e}"),
        }
    }
}

impl<E: std::fmt::Display + std::fmt::Debug> std::error::Error for ChainError<E> {}

/// Try each attempt in order until one succeeds.
///
/// - On the first `Ok(s)`, returns `Ok((s, index))` — `index` is the
///   position of the attempt that authenticated.
/// - On an `Err` for which `is_auth_failure` is `true`, advances to the next
///   attempt (the credential was stale/rejected).
/// - On an `Err` for which `is_auth_failure` is `false` (a network or parse
///   error, not a credential problem), returns `Err(ChainError::AllFailed(e))`
///   immediately without advancing — a transient blip must not burn the chain.
/// - If every attempt is an auth failure, returns the **last** one.
/// - If `attempts` is empty, returns `Err(ChainError::Empty)`.
///
/// # Errors
/// Returns [`ChainError`] as described above.
pub fn try_chain<S, E, F>(
    attempts: Vec<F>,
    is_auth_failure: impl Fn(&E) -> bool,
) -> Result<(S, usize), ChainError<E>>
where
    F: FnOnce() -> Result<S, E>,
{
    let mut last: Option<E> = None;
    for (idx, attempt) in attempts.into_iter().enumerate() {
        match attempt() {
            Ok(s) => return Ok((s, idx)),
            Err(e) if is_auth_failure(&e) => last = Some(e),
            Err(e) => return Err(ChainError::AllFailed(e)),
        }
    }
    match last {
        Some(e) => Err(ChainError::AllFailed(e)),
        None => Err(ChainError::Empty),
    }
}

#[cfg(test)]
mod tests {

    #[derive(Debug, PartialEq)]
    struct FakeErr {
        auth: bool,
        tag: u8,
    }

    fn auth_failed(tag: u8) -> FakeErr {
        FakeErr { auth: true, tag }
    }
    fn other(tag: u8) -> FakeErr {
        FakeErr { auth: false, tag }
    }

    #[test]
    fn empty_chain_is_empty_error() {
        let attempts: Vec<fn() -> Result<u32, FakeErr>> = vec![];
        let r = super::try_chain(attempts, |e: &FakeErr| e.auth);
        assert!(matches!(r, Err(super::ChainError::Empty)));
    }

    #[test]
    fn first_success_wins_with_index_zero() {
        let attempts: Vec<Box<dyn FnOnce() -> Result<u32, FakeErr>>> =
            vec![Box::new(|| Ok(10)), Box::new(|| Ok(20))];
        let (val, idx) = super::try_chain(attempts, |e: &FakeErr| e.auth).expect("first ok");
        assert_eq!((val, idx), (10, 0));
    }

    #[test]
    fn auth_failure_falls_through_to_next_success() {
        let attempts: Vec<Box<dyn FnOnce() -> Result<u32, FakeErr>>> =
            vec![Box::new(|| Err(auth_failed(1))), Box::new(|| Ok(20))];
        let (val, idx) = super::try_chain(attempts, |e: &FakeErr| e.auth).expect("second ok");
        assert_eq!((val, idx), (20, 1));
    }

    #[test]
    fn non_auth_failure_stops_immediately() {
        // The second attempt would succeed, but the first fails with a
        // non-auth error, so the chain must NOT advance.
        let attempts: Vec<Box<dyn FnOnce() -> Result<u32, FakeErr>>> =
            vec![Box::new(|| Err(other(7))), Box::new(|| Ok(20))];
        match super::try_chain(attempts, |e: &FakeErr| e.auth) {
            Err(super::ChainError::AllFailed(e)) => assert_eq!(e, other(7)),
            other => panic!("expected immediate AllFailed(other(7)), got {other:?}"),
        }
    }

    #[test]
    fn all_auth_failures_returns_last_error() {
        let attempts: Vec<Box<dyn FnOnce() -> Result<u32, FakeErr>>> = vec![
            Box::new(|| Err(auth_failed(1))),
            Box::new(|| Err(auth_failed(2))),
        ];
        match super::try_chain(attempts, |e: &FakeErr| e.auth) {
            Err(super::ChainError::AllFailed(e)) => assert_eq!(e, auth_failed(2)),
            other => panic!("expected AllFailed(auth_failed(2)), got {other:?}"),
        }
    }

    #[test]
    fn is_unauthorized_true_for_real_401() {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        // Dependency-free local server: respond 401 once, on a thread.
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let addr = listener.local_addr().expect("addr");
        let handle = std::thread::spawn(move || {
            if let Ok((mut sock, _)) = listener.accept() {
                let mut buf = [0u8; 1024];
                let _ = sock.read(&mut buf);
                let _ = sock.write_all(
                    b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                );
            }
        });

        let err = reqwest::blocking::Client::new()
            .get(format!("http://{addr}/"))
            .send()
            .expect("connect")
            .error_for_status()
            .expect_err("401 must be an error");
        assert!(
            super::is_unauthorized(&err),
            "401 reqwest error must be unauthorized"
        );
        handle.join().ok();
    }
}
