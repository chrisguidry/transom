//! A Redis started in the test's own process: answering, kept apart from
//! every other, and removed when it is dropped.

use std::process::Command;

use test_redis::Redis;

/// Sets a key at `url` and reads it back.
fn round_trip(url: &str) -> String {
    let mut connection = redis::Client::open(url).unwrap().get_connection().unwrap();
    redis::cmd("SET")
        .arg("test-redis:greeting")
        .arg("hello")
        .exec(&mut connection)
        .unwrap();
    redis::cmd("GET")
        .arg("test-redis:greeting")
        .query(&mut connection)
        .unwrap()
}

#[test]
fn a_started_redis_stores_what_it_is_given() {
    let redis = Redis::start();
    assert_eq!(round_trip(redis.url()), "hello");
}

#[test]
fn each_redis_has_a_port_of_its_own() {
    let (first, second) = (Redis::start(), Redis::start());
    assert_ne!(first.url(), second.url());
}

#[test]
fn dropping_a_redis_removes_its_container() {
    let redis = Redis::start();
    let name = format!("^{}$", redis.container());
    drop(redis);
    let listed = Command::new("docker")
        .args(["ps", "--all", "--quiet", "--filter"])
        .arg(format!("name={name}"))
        .output()
        .unwrap();
    assert_eq!(String::from_utf8(listed.stdout).unwrap(), "");
}

#[test]
fn the_shared_url_is_the_same_on_every_call() {
    let url = test_redis::url();
    assert_eq!(test_redis::url(), url);
    assert_eq!(round_trip(&url), "hello");
}
