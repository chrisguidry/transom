//! `transom serve` as a process: starting, answering, and stopping.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::process::{Child, ChildStderr, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;

fn redis_url() -> String {
    // unwrap_or rather than a closure, so the same code runs whether or not
    // the variable is set, and coverage is the same here and in CI.
    std::env::var("TRANSOM_TEST_REDIS_URL").unwrap_or("redis://127.0.0.1:6379".to_owned())
}

fn transom(environment: &[(&str, &str)]) -> Child {
    Command::new(env!("CARGO_BIN_EXE_transom"))
        .arg("serve")
        .env("NO_COLOR", "1")
        .envs(environment.iter().copied())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap()
}

/// A running server and the address it reported in its log.
struct Server {
    child: Child,
    address: SocketAddr,
}

fn start() -> Server {
    start_with(&[])
}

fn start_with(environment: &[(&str, &str)]) -> Server {
    let redis_url = redis_url();
    let mut settings = vec![
        ("TRANSOM_LISTEN", "127.0.0.1:0"),
        ("TRANSOM_REDIS_URL", redis_url.as_str()),
    ];
    settings.extend_from_slice(environment);
    let mut child = transom(&settings);
    let mut lines = BufReader::new(child.stderr.take().unwrap()).lines();
    let address = lines
        .by_ref()
        .map(Result::unwrap)
        .find_map(|line| {
            line.split_once("address=")
                .map(|(_, rest)| rest.split_whitespace().next().unwrap().parse().unwrap())
        })
        .unwrap();
    // The rest of the log is read and dropped, so the server's writes to
    // it keep succeeding while it answers requests.
    std::thread::spawn(move || lines.for_each(drop));
    Server { child, address }
}

fn get(address: SocketAddr, path: &str) -> String {
    let mut stream = TcpStream::connect(address).unwrap();
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: transom\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    response
}

fn stop(server: Server, signal: Signal) -> ExitStatus {
    let Server { mut child, .. } = server;
    kill(Pid::from_raw(child.id().try_into().unwrap()), signal).unwrap();
    child.wait().unwrap()
}

#[test]
fn answers_the_health_check() {
    let server = start();
    let response = get(server.address, "/healthz");
    stop(server, Signal::SIGTERM);
    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
}

#[test]
fn pings_the_watchdog_url() {
    let switch = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/ping/abc", switch.local_addr().unwrap());
    let server = start_with(&[("TRANSOM_WATCHDOG_URL", &url)]);
    let (mut ping, _) = switch.accept().unwrap();
    let mut request_line = String::new();
    BufReader::new(&mut ping)
        .read_line(&mut request_line)
        .unwrap();
    write!(ping, "HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n").unwrap();
    stop(server, Signal::SIGTERM);
    assert_eq!(request_line, "GET /ping/abc HTTP/1.1\r\n");
}

/// A token issuer, serving on a runtime of its own while the test runs.
fn issuer() -> (tokio::runtime::Runtime, test_issuer::TestIssuer) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let issuer = runtime.block_on(test_issuer::TestIssuer::start());
    (runtime, issuer)
}

/// Opens `@chris`'s live feed as `token`, and returns its response's
/// headers once they arrive, and the connection.
fn follow(address: SocketAddr, token: &str) -> (Vec<String>, BufReader<TcpStream>) {
    let mut stream = TcpStream::connect(address).unwrap();
    write!(
        stream,
        "GET /@chris/exchanges HTTP/1.1\r\nHost: transom\r\nAuthorization: Bearer {token}\r\n\
         Accept: text/event-stream\r\n\r\n"
    )
    .unwrap();
    let mut feed = BufReader::new(stream);
    let headers = feed
        .by_ref()
        .lines()
        .map(Result::unwrap)
        .take_while(|line| !line.is_empty())
        .collect();
    (headers, feed)
}

#[test]
fn a_live_feed_ends_and_the_server_exits_promptly_on_sigterm() {
    let (_runtime, issuer) = issuer();
    let prefix = format!("test-{}", ulid::Ulid::generate());
    let server = start_with(&[
        ("TRANSOM_REDIS_PREFIX", &prefix),
        ("TRANSOM_ISSUERS__TEST__URL", &issuer.url),
        ("TRANSOM_ISSUERS__TEST__AUDIENCE", test_issuer::AUDIENCE),
    ]);
    let token = issuer.token("chris");
    sign_up(server.address, &token);
    let (headers, mut feed) = follow(server.address, &token);
    let asked = Instant::now();
    let status = stop(server, Signal::SIGTERM);
    feed.read_to_end(&mut Vec::new()).unwrap();
    let stopped_within = asked.elapsed();
    assert_eq!(headers[0], "HTTP/1.1 200 OK", "{headers:?}");
    assert!(status.success());
    assert!(
        stopped_within < Duration::from_secs(3),
        "{stopped_within:?}"
    );
}

/// Makes `@chris` the individual `token` proves.
fn sign_up(address: SocketAddr, token: &str) {
    let mut stream = TcpStream::connect(address).unwrap();
    write!(
        stream,
        "PUT /@chris HTTP/1.1\r\nHost: transom\r\nAuthorization: Bearer {token}\r\n\
         Content-Length: 0\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    assert!(response.starts_with("HTTP/1.1 201"), "{response}");
}

#[test]
fn exits_cleanly_on_sigterm() {
    assert!(stop(start(), Signal::SIGTERM).success());
}

#[test]
fn exits_cleanly_on_sigint() {
    assert!(stop(start(), Signal::SIGINT).success());
}

fn failure(environment: &[(&str, &str)]) -> (ExitStatus, String) {
    let mut child = transom(environment);
    let mut log = String::new();
    let mut stderr: ChildStderr = child.stderr.take().unwrap();
    stderr.read_to_string(&mut log).unwrap();
    (child.wait().unwrap(), log)
}

#[test]
fn fails_on_an_invalid_redis_url() {
    let (status, log) = failure(&[
        ("TRANSOM_LISTEN", "127.0.0.1:0"),
        ("TRANSOM_REDIS_URL", "not-a-url"),
    ]);
    assert!(!status.success());
    assert!(log.contains("transom stopped"), "{log}");
}

#[test]
fn fails_when_the_address_is_taken() {
    let taken = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = taken.local_addr().unwrap().to_string();
    let (status, log) = failure(&[("TRANSOM_LISTEN", &address)]);
    assert!(!status.success());
    assert!(log.contains("transom stopped"), "{log}");
}

#[test]
fn fails_on_invalid_settings() {
    let (status, log) = failure(&[("TRANSOM_LISTEN", "not-an-address")]);
    assert!(!status.success());
    assert!(log.contains("transom stopped"), "{log}");
}

#[test]
fn fails_on_an_issuer_it_cannot_set_up() {
    let (status, log) = failure(&[
        ("TRANSOM_LISTEN", "127.0.0.1:0"),
        (
            "TRANSOM_ISSUERS__CLUSTER__URL",
            "https://kubernetes.default.svc",
        ),
        ("TRANSOM_ISSUERS__CLUSTER__AUDIENCE", "transom"),
        ("TRANSOM_ISSUERS__CLUSTER__CA_FILE", "/nonexistent/ca.crt"),
    ]);
    assert!(!status.success());
    assert!(log.contains("transom stopped"), "{log}");
}
