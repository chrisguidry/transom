//! `test-redis` as a process: printing a URL, and removing its container
//! when its input closes or it is killed.

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// A running `test-redis` and the URL it printed.
struct Holder {
    child: Child,
    url: String,
}

fn start(environment: &[(&str, &str)]) -> Holder {
    let mut child = Command::new(env!("CARGO_BIN_EXE_test-redis"))
        .env_remove(test_redis::URL_VARIABLE)
        .envs(environment.iter().copied())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut url = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut url)
        .unwrap();
    Holder {
        child,
        url: url.trim().to_owned(),
    }
}

/// The containers that the process `pid` started, by the names
/// `Redis::start` gives them.
fn containers_of(pid: u32) -> String {
    let listed = Command::new("docker")
        .args(["ps", "--all", "--quiet", "--filter"])
        .arg(format!("name=^transom-test-redis-{pid}-"))
        .output()
        .unwrap();
    String::from_utf8(listed.stdout).unwrap()
}

/// Waits up to ten seconds for every container of `pid` to be removed,
/// since a process that exits leaves Docker to remove its container after.
fn await_no_containers_of(pid: u32) -> String {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut listed = containers_of(pid);
    while !listed.is_empty() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
        listed = containers_of(pid);
    }
    listed
}

fn ping(url: &str) -> String {
    let mut connection = redis::Client::open(url).unwrap().get_connection().unwrap();
    redis::cmd("PING").query(&mut connection).unwrap()
}

#[test]
fn prints_the_url_of_a_redis_that_answers() {
    let mut holder = start(&[]);
    assert_eq!(ping(&holder.url), "PONG");
    drop(holder.child.stdin.take());
    holder.child.wait().unwrap();
}

#[test]
fn closing_its_input_removes_the_container() {
    let mut holder = start(&[]);
    let pid = holder.child.id();
    assert_ne!(containers_of(pid), "");
    drop(holder.child.stdin.take());
    assert!(holder.child.wait().unwrap().success());
    assert_eq!(await_no_containers_of(pid), "");
}

#[test]
fn killing_it_removes_the_container() {
    let mut holder = start(&[]);
    let pid = holder.child.id();
    holder.child.kill().unwrap();
    holder.child.wait().unwrap();
    assert_eq!(await_no_containers_of(pid), "");
}

#[test]
fn prints_the_url_it_is_given_and_starts_nothing() {
    let mut holder = start(&[(test_redis::URL_VARIABLE, "redis://192.0.2.1:6379")]);
    assert_eq!(holder.url, "redis://192.0.2.1:6379");
    assert_eq!(containers_of(holder.child.id()), "");
    drop(holder.child.stdin.take());
    assert!(holder.child.wait().unwrap().success());
}
