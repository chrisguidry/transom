//! `test-issuer` as a process: starting, answering as its flags say, and
//! stopping.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command, ExitStatus, Stdio};

use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;
use serde_json::Value;

/// A running issuer and the URL it printed.
struct Issuer {
    child: Child,
    url: String,
}

fn start(flags: &[&str]) -> Issuer {
    let mut child = Command::new(env!("CARGO_BIN_EXE_test-issuer"))
        .args(flags)
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut url = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut url)
        .unwrap();
    Issuer {
        child,
        url: url.trim().to_owned(),
    }
}

/// The body of the response to `GET path`, sent to `address`.
fn get(address: &str, path: &str) -> String {
    let mut stream = TcpStream::connect(address).unwrap();
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    response.split_once("\r\n\r\n").unwrap().1.to_owned()
}

fn stop(issuer: Issuer, signal: Signal) -> ExitStatus {
    let Issuer { mut child, .. } = issuer;
    kill(Pid::from_raw(child.id().try_into().unwrap()), signal).unwrap();
    child.wait().unwrap()
}

fn address(url: &str) -> &str {
    url.strip_prefix("http://").unwrap()
}

#[test]
fn serves_discovery_at_the_url_it_prints() {
    let issuer = start(&[]);
    let discovery = get(address(&issuer.url), "/.well-known/openid-configuration");
    let discovery: Value = serde_json::from_str(&discovery).unwrap();
    let url = issuer.url.clone();
    stop(issuer, Signal::SIGTERM);
    assert_eq!(discovery["issuer"], url);
    assert_eq!(
        discovery["authorization_endpoint"],
        format!("{url}/authorize")
    );
}

#[test]
fn names_the_url_it_is_told() {
    // A port that was free a moment ago, so the issuer can listen on an
    // address the test knows while naming itself by another.
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let listen = format!("127.0.0.1:{port}");
    let url = format!("http://localhost:{port}");
    let issuer = start(&["--listen", &listen, "--url", &url]);
    let printed = issuer.url.clone();
    let token = get(&listen, "/access-token");
    stop(issuer, Signal::SIGTERM);
    let claims: Value = jsonwebtoken::dangerous::insecure_decode_claims(&token).unwrap();
    assert_eq!(printed, url);
    assert_eq!(claims["iss"], url);
}

#[test]
fn signs_access_tokens_as_its_flags_say() {
    let issuer = start(&[
        "--audience",
        "transom",
        "--client-id",
        "an-app",
        "--client-secret",
        "secret",
        "--subject",
        "chris-subject",
        "--name",
        "chris",
    ]);
    let token = get(address(&issuer.url), "/access-token");
    stop(issuer, Signal::SIGTERM);
    let claims: Value = jsonwebtoken::dangerous::insecure_decode_claims(&token).unwrap();
    assert_eq!(claims["aud"], "transom");
    assert_eq!(claims["sub"], "chris-subject");
    assert_eq!(claims["preferred_username"], "chris");
}

#[test]
fn exits_cleanly_on_sigterm() {
    assert!(stop(start(&[]), Signal::SIGTERM).success());
}

#[test]
fn exits_cleanly_on_sigint() {
    assert!(stop(start(&[]), Signal::SIGINT).success());
}
