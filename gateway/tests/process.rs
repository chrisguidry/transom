//! `transom-gateway` as a process: starting with and without credentials,
//! answering, and stopping.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, ChildStderr, Command, ExitStatus, Stdio};

use aws_lc_rs::encoding::{AsDer, Pkcs8V1Der};
use aws_lc_rs::rsa::{KeyPair, KeySize};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;

fn gateway(environment: &[(&str, &str)]) -> Child {
    Command::new(env!("CARGO_BIN_EXE_transom-gateway"))
        .env("NO_COLOR", "1")
        .envs(environment.iter().copied())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap()
}

/// A running gateway, the address it reported in its log, and the lines
/// it logged before that.
struct Gateway {
    child: Child,
    address: SocketAddr,
    log: String,
}

fn start(credentials: &str) -> Gateway {
    let mut child = gateway(&[
        ("TRANSOM_GATEWAY_LISTEN", "127.0.0.1:0"),
        ("TRANSOM_GATEWAY_CREDENTIALS", credentials),
    ]);
    let mut log = String::new();
    let mut lines = BufReader::new(child.stderr.take().unwrap()).lines();
    let address = lines
        .by_ref()
        .map(Result::unwrap)
        .find_map(|line| {
            log.push_str(&line);
            line.split_once("address=")
                .map(|(_, rest)| rest.split_whitespace().next().unwrap().parse().unwrap())
        })
        .unwrap();
    // The gateway logs each request, and one whose log line meets a closed
    // pipe is never answered, so the rest of the log is read and dropped.
    std::thread::spawn(move || lines.for_each(drop));
    Gateway {
        child,
        address,
        log,
    }
}

/// Sends `method path` with `body`, returning the whole response.
fn send(address: SocketAddr, method: &str, path: &str, body: &str) -> String {
    let mut stream = TcpStream::connect(address).unwrap();
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: gateway\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    response
}

fn stop(gateway: Gateway, signal: Signal) -> ExitStatus {
    let Gateway { mut child, .. } = gateway;
    kill(Pid::from_raw(child.id().try_into().unwrap()), signal).unwrap();
    child.wait().unwrap()
}

/// A file of `contents` in the temporary directory, named for `name`.
fn file(name: &str, contents: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "transom-gateway-{}-{name}.json",
        std::process::id()
    ));
    std::fs::write(&path, contents).unwrap();
    path
}

/// A service account key file, as Google writes one, for a new key.
fn key_file() -> String {
    let key = KeyPair::generate(KeySize::Rsa2048).unwrap();
    let der = AsDer::<Pkcs8V1Der>::as_der(&key).unwrap();
    let pem = format!(
        "-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----\n",
        STANDARD.encode(der.as_ref())
    );
    serde_json::json!({
        "project_id": "transom-tests",
        "private_key_id": "test-key",
        "private_key": pem,
        "client_email": "gateway@transom-tests.iam.gserviceaccount.com",
        "token_uri": "http://127.0.0.1:1/token",
    })
    .to_string()
}

const NO_CREDENTIALS: &str = "/nonexistent/service-account.json";

#[test]
fn answers_the_health_check() {
    let gateway = start(NO_CREDENTIALS);
    let response = send(gateway.address, "GET", "/healthz", "");
    stop(gateway, Signal::SIGTERM);
    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
}

#[test]
fn refuses_wake_ups_without_credentials() {
    let gateway = start(NO_CREDENTIALS);
    let response = send(gateway.address, "POST", "/wake", r#"{"token": "a-phone"}"#);
    let log = gateway.log.clone();
    stop(gateway, Signal::SIGTERM);
    assert!(
        response.starts_with("HTTP/1.1 503 Service Unavailable"),
        "{response}"
    );
    assert!(log.contains("no Firebase credentials"), "{log}");
}

#[test]
fn names_the_project_of_its_credentials() {
    let key = file("key", &key_file());
    let gateway = start(key.to_str().unwrap());
    let log = gateway.log.clone();
    stop(gateway, Signal::SIGTERM);
    std::fs::remove_file(key).unwrap();
    assert!(log.contains("project=\"transom-tests\""), "{log}");
}

#[test]
fn exits_cleanly_on_sigterm() {
    assert!(stop(start(NO_CREDENTIALS), Signal::SIGTERM).success());
}

#[test]
fn exits_cleanly_on_sigint() {
    assert!(stop(start(NO_CREDENTIALS), Signal::SIGINT).success());
}

fn failure(environment: &[(&str, &str)]) -> (ExitStatus, String) {
    let mut child = gateway(environment);
    let mut log = String::new();
    let mut stderr: ChildStderr = child.stderr.take().unwrap();
    stderr.read_to_string(&mut log).unwrap();
    (child.wait().unwrap(), log)
}

#[test]
fn fails_on_invalid_settings() {
    let (status, log) = failure(&[("TRANSOM_GATEWAY_LISTEN", "not-an-address")]);
    assert!(!status.success());
    assert!(log.contains("the gateway stopped"), "{log}");
}

#[test]
fn fails_on_a_file_that_is_not_a_key() {
    let key = file("broken", "{}");
    let (status, log) = failure(&[
        ("TRANSOM_GATEWAY_LISTEN", "127.0.0.1:0"),
        ("TRANSOM_GATEWAY_CREDENTIALS", key.to_str().unwrap()),
    ]);
    std::fs::remove_file(key).unwrap();
    assert!(!status.success());
    assert!(log.contains("is not a service account key"), "{log}");
}

#[test]
fn fails_when_the_address_is_taken() {
    let taken = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = taken.local_addr().unwrap().to_string();
    let (status, log) = failure(&[
        ("TRANSOM_GATEWAY_LISTEN", &address),
        ("TRANSOM_GATEWAY_CREDENTIALS", NO_CREDENTIALS),
    ]);
    assert!(!status.success());
    assert!(log.contains("the gateway stopped"), "{log}");
}
