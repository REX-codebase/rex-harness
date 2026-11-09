//! The loopback listener that receives the browser's redirect.
//!
//! OpenAI's contract: the redirect URI is an HTTP loopback callback on
//! `127.0.0.1` (never `localhost`), `http://127.0.0.1:PORT/auth/callback`;
//! the listener is started *before* the browser opens; only the port may vary
//! between sign-ins; and the exact same URI, port included, goes in the
//! authorization request and the code exchange.
//!
//! This listener serves one purpose and refuses everything else:
//!
//! * it binds `127.0.0.1` only, so nothing off the machine can reach it;
//! * only `GET /auth/callback` is answered, plus one single-use launch path
//!   (below); any other path (a favicon fetch, a probe) gets a 404 and the
//!   wait continues;
//! * the `Host` header must be the loopback address and port we bound, which
//!   closes DNS-rebinding;
//! * the `state` is checked before anything else in the request is trusted. A
//!   callback whose state is wrong (a stale tab, a forged request) gets a 400
//!   and the wait continues, so it cannot end or poison the attempt;
//! * no byte of the request is ever reflected into a response.
//!
//! The launch path exists so the authorization URL, which can carry an
//! ID-token hint on a repeat sign-in, never has to be handed to the browser
//! launcher on a command line, where any local user could read it. The
//! launcher gets `http://127.0.0.1:PORT/auth/start/<random>` instead; the
//! listener answers that once with a redirect to the real URL and forgets it.

use std::io::{ErrorKind, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

pub const CALLBACK_PATH: &str = "/auth/callback";
const LAUNCH_PREFIX: &str = "/auth/start/";
/// The port OpenAI's docs use in their examples. If it is taken, the OS
/// picks another; the docs allow the port to vary.
pub const PREFERRED_PORT: u16 = 1455;
/// Longest request head read. A callback is a few hundred bytes.
const MAX_REQUEST_BYTES: usize = 8 * 1024;
/// How long one connection may take to send its request.
const READ_TIMEOUT: Duration = Duration::from_secs(3);
/// Connections handled at once; more are dropped.
const MAX_HANDLERS: usize = 16;

/// What the redirect carried, once its state checked out.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Callback {
    pub code: Option<String>,
    /// An OAuth `error` code, e.g. `access_denied`.
    pub error: Option<String>,
    /// The issued client id (a new registration's callback carries it).
    pub client_id: Option<String>,
    /// The granted scopes, space separated.
    pub scope: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WaitError {
    Timeout,
    Cancelled,
    Io(String),
}

/// The one redirect a launch path may serve: its token and the target URL.
type Launch = Arc<Mutex<Option<(String, String)>>>;

pub struct Listener {
    inner: TcpListener,
    port: u16,
    read_timeout: Duration,
    launch: Launch,
}

impl Listener {
    /// Bind `127.0.0.1`, on [`PREFERRED_PORT`] if it is free and otherwise on
    /// a port the OS chooses.
    pub fn bind() -> std::io::Result<Self> {
        Self::bind_preferring(PREFERRED_PORT)
    }

    pub fn bind_preferring(port: u16) -> std::io::Result<Self> {
        let inner = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port)))
            .or_else(|_| TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))))?;
        inner.set_nonblocking(true)?;
        let port = inner.local_addr()?.port();
        Ok(Self {
            inner,
            port,
            read_timeout: READ_TIMEOUT,
            launch: Arc::new(Mutex::new(None)),
        })
    }

    /// Arrange for `target` to be served once, at an unguessable path, and
    /// return the loopback URL that reaches it. Setting it again replaces
    /// the earlier one.
    pub fn set_launch(&self, target: &str) -> std::io::Result<String> {
        use ring::rand::{SecureRandom, SystemRandom};
        if target.bytes().any(|b| b < 0x20 || b == 0x7f) {
            return Err(std::io::Error::other("the launch URL has control bytes"));
        }
        let mut raw = [0u8; 18];
        SystemRandom::new()
            .fill(&mut raw)
            .map_err(|_| std::io::Error::other("no secure random source"))?;
        let token: String = raw.iter().map(|b| format!("{b:02x}")).collect();
        if let Ok(mut slot) = self.launch.lock() {
            *slot = Some((token.clone(), target.to_string()));
        }
        Ok(format!(
            "http://127.0.0.1:{}{LAUNCH_PREFIX}{token}",
            self.port
        ))
    }

    #[cfg(test)]
    pub fn port(&self) -> u16 {
        self.port
    }

    /// The redirect URI to send in the authorization request and the code
    /// exchange. Never `localhost`.
    pub fn redirect_uri(&self) -> String {
        format!("http://127.0.0.1:{}{}", self.port, CALLBACK_PATH)
    }

    /// Wait for the redirect whose `state` is `expected_state`.
    ///
    /// Each connection is handled on its own short-lived thread and a valid
    /// callback comes back over a channel, so one idle or slow connection (a
    /// browser pre-connect, or anything else on this machine) cannot delay
    /// the real redirect. The number of handlers in flight is capped.
    pub fn wait(
        &self,
        expected_state: &str,
        timeout: Duration,
        cancel: &AtomicBool,
    ) -> Result<Callback, WaitError> {
        let deadline = Instant::now() + timeout;
        let (tx, rx) = mpsc::channel::<Callback>();
        let in_flight = Arc::new(AtomicUsize::new(0));
        let state: Arc<str> = Arc::from(expected_state);
        let (port, read_timeout) = (self.port, self.read_timeout);
        let launch = self.launch.clone();
        loop {
            if cancel.load(Ordering::SeqCst) {
                return Err(WaitError::Cancelled);
            }
            if let Ok(cb) = rx.try_recv() {
                return Ok(cb);
            }
            if Instant::now() >= deadline {
                return Err(WaitError::Timeout);
            }
            match self.inner.accept() {
                Ok((stream, _)) => {
                    if in_flight.load(Ordering::SeqCst) >= MAX_HANDLERS {
                        continue; // dropped: too many at once
                    }
                    in_flight.fetch_add(1, Ordering::SeqCst);
                    let (tx, state, in_flight) = (tx.clone(), state.clone(), in_flight.clone());
                    let launch = launch.clone();
                    std::thread::spawn(move || {
                        if let Some(cb) =
                            serve_connection(stream, port, &state, read_timeout, &launch)
                        {
                            let _ = tx.send(cb);
                        }
                        in_flight.fetch_sub(1, Ordering::SeqCst);
                    });
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => {
                    if let Ok(cb) = rx.recv_timeout(Duration::from_millis(25)) {
                        return Ok(cb);
                    }
                }
                Err(e) => return Err(WaitError::Io(e.to_string())),
            }
        }
    }
}

/// Handle one connection. `Some` only for a valid callback.
fn serve_connection(
    mut stream: TcpStream,
    port: u16,
    expected_state: &str,
    read_timeout: Duration,
    launch: &Launch,
) -> Option<Callback> {
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(read_timeout));
    let head = read_head(&mut stream)?;
    let Some(request) = parse_request(&head) else {
        respond(&mut stream, 400, "Bad request");
        return None;
    };
    if request.method != "GET" {
        respond(&mut stream, 405, "Method not allowed");
        return None;
    }
    let expected_host = format!("127.0.0.1:{port}");
    let host_ok = request
        .host
        .as_deref()
        .is_some_and(|h| h.eq_ignore_ascii_case(&expected_host));
    if let Some(token) = request.path.strip_prefix(LAUNCH_PREFIX) {
        // one use: the redirect is taken out of the slot before it is sent
        let target = match launch.lock() {
            Ok(mut slot) if host_ok => match slot.as_ref() {
                Some((expected, _)) if constant_time_eq(token.as_bytes(), expected.as_bytes()) => {
                    slot.take().map(|(_, url)| url)
                }
                _ => None,
            },
            _ => None,
        };
        match target {
            Some(url) => redirect(&mut stream, &url),
            None => respond(&mut stream, 404, "Not found"),
        }
        return None;
    }
    if request.path != CALLBACK_PATH {
        respond(&mut stream, 404, "Not found");
        return None;
    }
    if !host_ok {
        respond(&mut stream, 400, "Bad request");
        return None;
    }
    let get = |k: &str| {
        request
            .query
            .iter()
            .find(|(key, _)| key == k)
            .map(|(_, v)| v.clone())
    };
    // state first: until it matches, nothing else in this request counts
    if !get("state").is_some_and(|s| constant_time_eq(s.as_bytes(), expected_state.as_bytes())) {
        respond(
            &mut stream,
            400,
            "This sign-in link is not for the current attempt.",
        );
        return None;
    }
    respond(
        &mut stream,
        200,
        "You can close this tab and return to REX.",
    );
    Some(Callback {
        code: get("code"),
        error: get("error"),
        client_id: get("client_id"),
        scope: get("scope"),
    })
}

fn read_head(stream: &mut TcpStream) -> Option<String> {
    let mut buf = Vec::with_capacity(512);
    let mut chunk = [0u8; 512];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
                if buf.len() > MAX_REQUEST_BYTES {
                    respond(stream, 431, "Request too large");
                    return None;
                }
            }
            Err(_) => return None,
        }
    }
    String::from_utf8(buf).ok()
}

struct Request {
    method: String,
    path: String,
    query: Vec<(String, String)>,
    host: Option<String>,
}

fn parse_request(head: &str) -> Option<Request> {
    let mut lines = head.split("\r\n");
    let mut first = lines.next()?.split(' ');
    let method = first.next()?.to_string();
    let target = first.next()?;
    if !first.next()?.starts_with("HTTP/") {
        return None;
    }
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let host = lines.take_while(|l| !l.is_empty()).find_map(|l| {
        let (name, value) = l.split_once(':')?;
        name.eq_ignore_ascii_case("host")
            .then(|| value.trim().to_string())
    });
    Some(Request {
        method,
        path: path.to_string(),
        query: url::form_urlencoded::parse(query.as_bytes())
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect(),
        host,
    })
}

/// A fixed, static page: nothing from the request is reflected.
fn respond(stream: &mut TcpStream, status: u16, message: &str) {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        431 => "Request Header Fields Too Large",
        _ => "Error",
    };
    let body = format!(
        "<!doctype html><meta charset=utf-8><title>REX</title><body style=\"font-family:system-ui;margin:3rem\"><p>{message}</p>"
    );
    let _ = write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.flush();
}

/// The single redirect a launch path serves. The target was built by the
/// sign-in itself and checked for control bytes when it was set.
fn redirect(stream: &mut TcpStream, location: &str) {
    let _ = write!(
        stream,
        "HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\nConnection: close\r\n\r\n"
    );
    let _ = stream.flush();
}

/// Compare without stopping at the first difference.
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// Send `raw` to the listener from another thread and return the reply.
    fn send(port: u16, raw: String) -> std::thread::JoinHandle<String> {
        std::thread::spawn(move || {
            let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
            s.write_all(raw.as_bytes()).unwrap();
            let mut out = String::new();
            let _ = s.read_to_string(&mut out);
            out
        })
    }

    fn get(port: u16, target: &str) -> String {
        format!("GET {target} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n")
    }

    fn wait_for(l: &Listener, state: &str) -> Result<Callback, WaitError> {
        l.wait(state, Duration::from_secs(3), &AtomicBool::new(false))
    }

    #[test]
    fn the_redirect_uri_is_the_exact_loopback_form_openai_requires() {
        let l = Listener::bind_preferring(0).unwrap();
        let uri = l.redirect_uri();
        assert_eq!(uri, format!("http://127.0.0.1:{}/auth/callback", l.port()));
        assert!(!uri.contains("localhost"));
    }

    #[test]
    fn a_valid_callback_is_returned_with_its_fields() {
        let l = Listener::bind_preferring(0).unwrap();
        let port = l.port();
        let t = send(port, get(port, "/auth/callback?code=abc%20d&state=S1&client_id=oaiapp_x&scope=openid+offline_access"));
        let cb = wait_for(&l, "S1").unwrap();
        assert_eq!(cb.code.as_deref(), Some("abc d"));
        assert_eq!(cb.client_id.as_deref(), Some("oaiapp_x"));
        assert_eq!(cb.scope.as_deref(), Some("openid offline_access"));
        assert!(cb.error.is_none());
        let reply = t.join().unwrap();
        assert!(reply.starts_with("HTTP/1.1 200"));
        assert!(reply.contains("Cache-Control: no-store"));
    }

    #[test]
    fn the_launch_path_redirects_once_and_only_to_the_holder_of_its_token() {
        let l = Listener::bind_preferring(0).unwrap();
        let port = l.port();
        let target = "https://auth.openai.com/api/accounts/authorize?id_token_hint=SECRET-HINT";
        let launch = l.set_launch(target).unwrap();
        assert!(launch.starts_with(&format!("http://127.0.0.1:{port}/auth/start/")));
        assert!(
            !launch.contains("SECRET-HINT") && !launch.contains("auth.openai.com"),
            "nothing of the real URL is in what the launcher is given"
        );
        let path = launch
            .strip_prefix(&format!("http://127.0.0.1:{port}"))
            .unwrap();
        let state = "S-LAUNCH";
        let keep = Arc::new(AtomicBool::new(false));
        let stop = keep.clone();
        let probes = {
            let path = path.to_string();
            std::thread::spawn(move || {
                // a wrong token and a wrong host get nothing
                let wrong = send(port, get(port, "/auth/start/not-the-token"))
                    .join()
                    .unwrap();
                let rebind = send(
                    port,
                    format!(
                        "GET {path} HTTP/1.1\r\nHost: evil.example\r\nConnection: close\r\n\r\n"
                    ),
                )
                .join()
                .unwrap();
                // the right request is redirected exactly once
                let first = send(port, get(port, &path)).join().unwrap();
                let second = send(port, get(port, &path)).join().unwrap();
                let done = send(
                    port,
                    get(port, &format!("/auth/callback?code=c&state={state}")),
                );
                let _ = done.join();
                stop.store(true, Ordering::SeqCst);
                (wrong, rebind, first, second)
            })
        };
        let cb = wait_for(&l, state).unwrap();
        assert_eq!(cb.code.as_deref(), Some("c"));
        let (wrong, rebind, first, second) = probes.join().unwrap();
        assert!(wrong.starts_with("HTTP/1.1 404"), "{wrong}");
        assert!(rebind.starts_with("HTTP/1.1 404"), "{rebind}");
        assert!(first.starts_with("HTTP/1.1 302"), "{first}");
        assert!(
            first.contains(&format!("Location: {target}\r\n")),
            "{first}"
        );
        assert!(first.contains("Cache-Control: no-store"));
        assert!(second.starts_with("HTTP/1.1 404"), "single use: {second}");
        assert!(keep.load(Ordering::SeqCst));
    }

    #[test]
    fn a_launch_target_with_control_bytes_is_refused() {
        let l = Listener::bind_preferring(0).unwrap();
        assert!(l
            .set_launch("https://auth.openai.com/\r\nSet-Cookie: x=1")
            .is_err());
        assert!(l.set_launch("https://auth.openai.com/\u{7f}").is_err());
    }

    #[test]
    fn a_declined_consent_comes_back_as_an_error_code() {
        let l = Listener::bind_preferring(0).unwrap();
        let port = l.port();
        let _t = send(
            port,
            get(port, "/auth/callback?error=access_denied&state=S1"),
        );
        let cb = wait_for(&l, "S1").unwrap();
        assert_eq!(cb.error.as_deref(), Some("access_denied"));
        assert!(cb.code.is_none());
    }

    #[test]
    fn a_wrong_state_is_refused_and_does_not_end_the_wait() {
        let l = Listener::bind_preferring(0).unwrap();
        let port = l.port();
        let forged = send(port, get(port, "/auth/callback?code=FORGED&state=other"));
        let honest = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(150));
            send(port, get(port, "/auth/callback?code=REAL&state=S1"))
                .join()
                .unwrap()
        });
        let cb = wait_for(&l, "S1").unwrap();
        assert_eq!(
            cb.code.as_deref(),
            Some("REAL"),
            "the forged request must not win"
        );
        assert!(forged.join().unwrap().starts_with("HTTP/1.1 400"));
        honest.join().unwrap();
    }

    #[test]
    fn other_paths_and_methods_are_refused_and_the_wait_continues() {
        let l = Listener::bind_preferring(0).unwrap();
        let port = l.port();
        let fav = send(port, get(port, "/favicon.ico"));
        let wrong_path = send(port, get(port, "/callback?code=X&state=S1"));
        let post = send(
            port,
            format!("POST /auth/callback?state=S1 HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n"),
        );
        let good = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(250));
            send(port, get(port, "/auth/callback?code=OK&state=S1"))
                .join()
                .unwrap()
        });
        assert_eq!(wait_for(&l, "S1").unwrap().code.as_deref(), Some("OK"));
        assert!(fav.join().unwrap().starts_with("HTTP/1.1 404"));
        // `/callback` must not match `/auth/callback`, as the docs warn
        assert!(wrong_path.join().unwrap().starts_with("HTTP/1.1 404"));
        assert!(post.join().unwrap().starts_with("HTTP/1.1 405"));
        good.join().unwrap();
    }

    #[test]
    fn a_request_for_another_host_name_is_refused_against_dns_rebinding() {
        let l = Listener::bind_preferring(0).unwrap();
        let port = l.port();
        let rebound = send(
            port,
            format!(
                "GET /auth/callback?code=X&state=S1 HTTP/1.1\r\nHost: evil.example:{port}\r\n\r\n"
            ),
        );
        let none = send(
            port,
            "GET /auth/callback?code=X&state=S1 HTTP/1.1\r\n\r\n".to_string(),
        );
        let good = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(250));
            send(port, get(port, "/auth/callback?code=OK&state=S1"))
                .join()
                .unwrap()
        });
        assert_eq!(wait_for(&l, "S1").unwrap().code.as_deref(), Some("OK"));
        assert!(rebound.join().unwrap().starts_with("HTTP/1.1 400"));
        assert!(none.join().unwrap().starts_with("HTTP/1.1 400"));
        good.join().unwrap();
    }

    #[test]
    fn nothing_from_the_request_is_reflected_into_the_page() {
        let l = Listener::bind_preferring(0).unwrap();
        let port = l.port();
        let t = send(
            port,
            get(
                port,
                "/auth/callback?state=%3Cscript%3Ealert(1)%3C/script%3E",
            ),
        );
        let _ = l.wait("S1", Duration::from_millis(300), &AtomicBool::new(false));
        let reply = t.join().unwrap();
        assert!(
            !reply.contains("script") && !reply.contains("alert"),
            "{reply}"
        );
    }

    #[test]
    fn an_idle_connection_does_not_delay_the_real_redirect() {
        let l = Listener::bind_preferring(0).unwrap();
        let port = l.port();
        // a connection that never sends a byte (a browser pre-connect, or
        // anything else on this machine) arrives first
        let _idle = TcpStream::connect(("127.0.0.1", port)).unwrap();
        let real = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            send(port, get(port, "/auth/callback?code=REAL&state=S1"))
                .join()
                .unwrap()
        });
        let start = Instant::now();
        let cb = wait_for(&l, "S1").unwrap();
        assert_eq!(cb.code.as_deref(), Some("REAL"));
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "the idle connection held the listener for {:?}",
            start.elapsed()
        );
        real.join().unwrap();
    }

    #[test]
    fn an_oversized_or_malformed_request_is_dropped_without_a_panic() {
        let mut l = Listener::bind_preferring(0).unwrap();
        l.read_timeout = Duration::from_millis(300);
        let port = l.port();
        let big = send(
            port,
            format!("GET /auth/callback?x={} HTTP/1.1\r\n", "A".repeat(20_000)),
        );
        let junk = send(port, "not http at all\r\n\r\n".to_string());
        let empty = send(port, String::new());
        let r = l.wait("S1", Duration::from_millis(600), &AtomicBool::new(false));
        assert_eq!(r, Err(WaitError::Timeout));
        assert!(big.join().unwrap().contains("431"));
        assert!(junk.join().unwrap().starts_with("HTTP/1.1 400"));
        let _ = empty.join();
    }

    #[test]
    fn the_wait_times_out_and_can_be_cancelled() {
        let l = Listener::bind_preferring(0).unwrap();
        assert_eq!(
            l.wait("S", Duration::from_millis(80), &AtomicBool::new(false)),
            Err(WaitError::Timeout)
        );
        let cancel = Arc::new(AtomicBool::new(false));
        let c2 = cancel.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            c2.store(true, Ordering::SeqCst);
        });
        assert_eq!(
            l.wait("S", Duration::from_secs(30), &cancel),
            Err(WaitError::Cancelled)
        );
    }

    #[test]
    fn a_busy_preferred_port_falls_back_to_another() {
        let first = Listener::bind_preferring(0).unwrap();
        let second = Listener::bind_preferring(first.port()).unwrap();
        assert_ne!(first.port(), second.port());
    }

    #[test]
    fn comparison_does_not_accept_a_prefix_or_a_different_length() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"abcd"));
        assert!(!constant_time_eq(b"", b"a"));
        assert!(constant_time_eq(b"", b""));
    }
}
