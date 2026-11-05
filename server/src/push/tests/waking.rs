//! Waking phones through the wake-up gateway: each arrival, end, and read
//! wakes the phones that should hear of it, and the gateway's answers
//! decide whether a wake-up is tried again, dropped, or forgets the phone.

use std::fmt::Write;

use aws_lc_rs::digest::{SHA256, digest};
use serde_json::{Value, json};

use super::devices::phone;
use super::gateway::Gateway;
use super::{Browser, JSON, Pair, devices, individual, paired, subscribe};
use crate::app::testing::{TestApp, decide, request};
use crate::exchanges::records::now;
use crate::push::wake_due;

const SECOND: i64 = 1000;
const MINUTE: i64 = 60 * SECOND;

/// Registers `@{name}`'s phone, as `token` signs in, to wake by `fcm`.
async fn register(app: &TestApp, name: &str, token: &str, fcm: &str) {
    let path = format!("/@{name}/devices/phone");
    request(app, "PUT", &path, token, &[JSON], phone(fcm)).await;
}

/// `@chris`, whose phone wakes by `fcm`, on a transom that wakes phones
/// through a gateway of its own.
async fn waking_by(fcm: &str) -> (Pair, Gateway) {
    let gateway = Gateway::start().await;
    let pair = paired(TestApp::waking_through(&gateway.url).await).await;
    register(&pair.app, "chris", &pair.chris, fcm).await;
    (pair, gateway)
}

/// Sends `@chris` a question from `@review-bot`, and returns its id.
async fn ask(pair: &Pair) -> String {
    let body = json!({ "summary": "Deploy?", "choices": ["Ship it", "Wait"] }).to_string();
    let (_, _, text) = request(&pair.app, "POST", "/@chris", &pair.bot, &[JSON], body).await;
    serde_json::from_str::<Value>(&text).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

async fn wake(pair: &Pair, at: i64) -> usize {
    wake_due(&pair.app.app, at).await.unwrap()
}

#[tokio::test]
async fn an_arrival_wakes_the_phone() {
    let (pair, gateway) = waking_by("a-phone").await;
    ask(&pair).await;
    assert_eq!(wake(&pair, now()).await, 1);
    assert_eq!(gateway.woken(), ["a-phone"]);
}

#[tokio::test]
async fn an_ending_wakes_the_phone() {
    let (pair, gateway) = waking_by("a-phone").await;
    let id = ask(&pair).await;
    wake(&pair, now()).await;
    let reply = format!("/exchanges/{id}/reply");
    let choice = r#"{"choice": "Wait"}"#;
    request(&pair.app, "PUT", &reply, &pair.chris, &[JSON], choice).await;
    wake(&pair, now()).await;
    assert_eq!(gateway.woken(), ["a-phone", "a-phone"]);
}

#[tokio::test]
async fn a_read_wakes_the_phone() {
    let (pair, gateway) = waking_by("a-phone").await;
    let id = ask(&pair).await;
    wake(&pair, now()).await;
    let read = format!("/exchanges/{id}/@chris");
    request(&pair.app, "PUT", &read, &pair.chris, &[], "").await;
    wake(&pair, now()).await;
    assert_eq!(gateway.woken(), ["a-phone", "a-phone"]);
}

#[tokio::test]
async fn each_repeat_of_an_urgent_message_wakes_the_phone_again() {
    let (pair, gateway) = waking_by("a-phone").await;
    let body = json!({ "summary": "Disk full", "urgency": "urgent" }).to_string();
    request(&pair.app, "POST", "/@chris", &pair.bot, &[JSON], body).await;
    let sent = now();
    wake(&pair, sent).await;
    crate::exchanges::repeat_due(&pair.app.app, sent + 5 * MINUTE)
        .await
        .unwrap();
    wake(&pair, sent + 5 * MINUTE).await;
    assert_eq!(gateway.woken(), ["a-phone", "a-phone"]);
}

#[tokio::test]
async fn a_burst_of_events_wakes_the_phone_once() {
    let (pair, gateway) = waking_by("a-phone").await;
    ask(&pair).await;
    ask(&pair).await;
    ask(&pair).await;
    assert_eq!(wake(&pair, now()).await, 1);
    assert_eq!(gateway.woken(), ["a-phone"]);
}

#[tokio::test]
async fn a_phone_fcm_no_longer_delivers_to_is_forgotten() {
    let (pair, _gateway) = waking_by("unregistered").await;
    ask(&pair).await;
    wake(&pair, now()).await;
    assert_eq!(devices(&pair).await["devices"], json!([]));
}

#[tokio::test]
async fn a_busy_fcm_is_tried_again_when_it_says() {
    let (pair, gateway) = waking_by("busy").await;
    ask(&pair).await;
    let start = now();
    let attempted = [
        wake(&pair, start).await,
        wake(&pair, start + 29 * SECOND).await,
        wake(&pair, start + 30 * SECOND).await,
    ];
    assert_eq!(attempted, [1, 0, 1]);
    assert_eq!(gateway.woken(), ["busy", "busy"]);
}

/// Asks `@chris` a question and wakes his phone for it, at `at`.
async fn ask_and_wake(pair: &Pair, at: i64) -> usize {
    ask(pair).await;
    wake(pair, at).await
}

#[tokio::test]
async fn a_phone_woken_too_often_is_tried_again_when_the_gateway_says() {
    let (pair, gateway) = waking_by("a-phone").await;
    // A second ahead, so each question asked after this is already due.
    let start = now() + SECOND;
    let attempted = [
        ask_and_wake(&pair, start).await,
        ask_and_wake(&pair, start).await,
        ask_and_wake(&pair, start).await,
        ask_and_wake(&pair, start).await,
        ask_and_wake(&pair, start).await,
        ask_and_wake(&pair, start).await,
        wake(&pair, start + 10 * SECOND).await,
        wake(&pair, start + 15 * SECOND).await,
    ];
    assert_eq!(attempted, [1, 1, 1, 1, 1, 1, 0, 1]);
    assert_eq!(gateway.woken().len(), 5);
}

#[tokio::test]
async fn a_refused_wake_up_is_dropped() {
    let (pair, gateway) = waking_by("foreign").await;
    ask(&pair).await;
    let attempted = [wake(&pair, now()).await, wake(&pair, now() + MINUTE).await];
    assert_eq!(attempted, [1, 0]);
    assert_eq!(gateway.woken(), ["foreign"]);
}

#[tokio::test]
async fn a_refusal_keeps_the_phone() {
    let (pair, _gateway) = waking_by("foreign").await;
    ask(&pair).await;
    wake(&pair, now()).await;
    assert_eq!(devices(&pair).await["devices"][0]["id"], "phone");
}

/// `@chris`, whose phone wakes by `a-phone`, on a transom whose gateway
/// setting names `wake_gateway`.
async fn waking_through(wake_gateway: &str) -> Pair {
    let pair = paired(TestApp::waking_through(wake_gateway).await).await;
    register(&pair.app, "chris", &pair.chris, "a-phone").await;
    pair
}

#[tokio::test]
async fn a_not_found_from_something_other_than_the_gateway_keeps_the_phone() {
    let gateway = Gateway::start().await;
    let pair = waking_through(&gateway.elsewhere()).await;
    ask(&pair).await;
    let attempted = [wake(&pair, now()).await, wake(&pair, now() + MINUTE).await];
    assert_eq!(attempted, [1, 0]);
    assert_eq!(devices(&pair).await["devices"][0]["id"], "phone");
}

#[tokio::test]
async fn an_unreachable_gateway_is_tried_again() {
    let pair = waking_through("http://127.0.0.1:1/wake").await;
    ask(&pair).await;
    let attempted = [
        wake(&pair, now()).await,
        wake(&pair, now()).await,
        wake(&pair, now() + MINUTE).await,
    ];
    assert_eq!(attempted, [1, 0, 1]);
}

#[tokio::test]
async fn a_wake_up_is_dropped_after_an_hour_of_trying() {
    let pair = waking_through("http://127.0.0.1:1/wake").await;
    ask(&pair).await;
    let attempted = [
        wake(&pair, now() + 61 * MINUTE).await,
        wake(&pair, now() + 120 * MINUTE).await,
    ];
    assert_eq!(attempted, [1, 0]);
}

#[tokio::test]
async fn a_removed_phone_is_not_woken() {
    let (pair, gateway) = waking_by("a-phone").await;
    ask(&pair).await;
    let path = "/@chris/devices/phone";
    request(&pair.app, "DELETE", path, &pair.chris, &[], "").await;
    wake(&pair, now()).await;
    assert_eq!(gateway.woken(), Vec::<String>::new());
}

#[tokio::test]
async fn nothing_is_sent_for_an_identity_without_fcm_devices() {
    let gateway = Gateway::start().await;
    let pair = paired(TestApp::waking_through(&gateway.url).await).await;
    subscribe(&pair, &Browser::at("https://push.example.com/send/abc")).await;
    ask(&pair).await;
    assert_eq!(wake(&pair, now()).await, 0);
    assert_eq!(gateway.woken(), Vec::<String>::new());
}

#[tokio::test]
async fn a_wake_up_another_server_claimed_is_skipped() {
    let (pair, _gateway) = waking_by("a-phone").await;
    let store = &pair.app.app.store;
    let mut connection = store.connection().await.unwrap();
    let () = redis::AsyncCommands::zadd(&mut connection, store.key("wakes"), "elsewhere", 0)
        .await
        .unwrap();
    assert_eq!(wake(&pair, now()).await, 0);
}

/// The installation IDs woken when `sender`, one of `@review-bot`,
/// `@chris`, and `@spouse`, sends to `@household`, which has `@chris` and
/// `@spouse` as members and allowed `@review-bot`, and each member's
/// phone wakes by its name.
async fn woken_when_sent_by(sender: &str) -> Vec<String> {
    let gateway = Gateway::start().await;
    let app = TestApp::waking_through(&gateway.url).await;
    let bot = individual(&app, "review-bot").await;
    let chris = individual(&app, "chris").await;
    let spouse = individual(&app, "spouse").await;
    request(&app, "PUT", "/@household", &chris, &[], "").await;
    request(&app, "PUT", "/@household/members/@spouse", &chris, &[], "").await;
    decide(&app, &spouse, "spouse", "join").await;
    request(&app, "POST", "/@household", &bot, &[], "hello").await;
    decide(&app, &chris, "household", "allow").await;
    register(&app, "chris", &chris, "chris").await;
    register(&app, "spouse", &spouse, "spouse").await;
    let token = [("review-bot", bot), ("chris", chris), ("spouse", spouse)]
        .into_iter()
        .find_map(|(name, token)| (name == sender).then_some(token))
        .unwrap();
    request(&app, "POST", "/@household", &token, &[], "Dinner?").await;
    wake_due(&app.app, now()).await.unwrap();
    let mut woken = gateway.woken();
    woken.sort();
    woken
}

#[tokio::test]
async fn an_arrival_for_a_group_wakes_each_members_phone() {
    assert_eq!(woken_when_sent_by("review-bot").await, ["chris", "spouse"]);
}

#[tokio::test]
async fn the_senders_own_phone_is_not_woken() {
    assert_eq!(woken_when_sent_by("chris").await, ["spouse"]);
}

/// The name the gateway's logs give the phone that wakes by `fcm`: the
/// first 6 octets of the token's SHA-256, in hex.
fn fingerprint(fcm: &str) -> String {
    let mut name = String::new();
    for byte in &digest(&SHA256, fcm.as_bytes()).as_ref()[..6] {
        write!(name, "{byte:02x}").unwrap();
    }
    name
}

#[tokio::test]
async fn a_wake_up_logs_the_fingerprint_of_its_token_but_never_the_token() {
    let fcm = "cX9q2Lw3TQ:APA91bHk3Lm9-Zq_xYw2Rt7UvBnM0pQsTuVwXyZ";
    let (pair, _gateway) = waking_by(fcm).await;
    ask(&pair).await;
    let logs = crate::logs::testing::Captured::start();
    wake(&pair, now()).await;
    let lines = logs.lines("the gateway woke a phone");
    assert_eq!(lines.len(), 1, "{}", logs.all());
    let device = format!("device={}", fingerprint(fcm));
    assert!(lines[0].contains(&device), "{}", lines[0]);
    assert!(!logs.all().contains(fcm), "{}", logs.all());
}
