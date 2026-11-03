//! `transom serve` as a process: starting, answering, and stopping.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::process::{Child, ChildStderr, Command, ExitStatus, Stdio};

use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;

fn redis_url() -> String {
    // unwrap_or rather than a closure, so the same code runs whether or not
    // the variable is set, and coverage is the same here and in CI.
    std::env::var("TRANSOM_TEST_REDIS_URL").unwrap_or("redis://127.0.0.1:6379".to_owned())
}

fn transom(listen: &str, redis_url: &str) -> Child {
    Command::new(env!("CARGO_BIN_EXE_transom"))
        .args(["serve", "--listen", listen, "--redis-url", redis_url])
        .env("NO_COLOR", "1")
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
    let mut child = transom("127.0.0.1:0", &redis_url());
    let mut lines = BufReader::new(child.stderr.take().unwrap()).lines();
    let address = lines
        .by_ref()
        .map(Result::unwrap)
        .find_map(|line| {
            line.split_once("address=")
                .map(|(_, rest)| rest.split_whitespace().next().unwrap().parse().unwrap())
        })
        .unwrap();
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
fn exits_cleanly_on_sigterm() {
    assert!(stop(start(), Signal::SIGTERM).success());
}

#[test]
fn exits_cleanly_on_sigint() {
    assert!(stop(start(), Signal::SIGINT).success());
}

fn failure(listen: &str, redis_url: &str) -> (ExitStatus, String) {
    let mut child = transom(listen, redis_url);
    let mut log = String::new();
    let mut stderr: ChildStderr = child.stderr.take().unwrap();
    stderr.read_to_string(&mut log).unwrap();
    (child.wait().unwrap(), log)
}

#[test]
fn fails_on_an_invalid_redis_url() {
    let (status, log) = failure("127.0.0.1:0", "not-a-url");
    assert!(!status.success());
    assert!(log.contains("transom stopped"), "{log}");
}

#[test]
fn fails_when_the_address_is_taken() {
    let taken = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = taken.local_addr().unwrap().to_string();
    let (status, log) = failure(&address, &redis_url());
    assert!(!status.success());
    assert!(log.contains("transom stopped"), "{log}");
}
