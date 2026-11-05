use serde_json::{Value, json};

use super::{Browser, JSON, PushService, individual};
use crate::app::testing::{TestApp, decide, request};
use crate::exchanges::records::now;
use crate::push::deliver_due;

/// `@chris` and `@spouse` in `@household`, which allowed `@review-bot`,
/// each with a browser subscribed at a push service of its own.
struct Household {
    app: TestApp,
    bot: String,
    chris: (String, PushService, Browser),
    spouse: (String, PushService, Browser),
}

async fn subscribed(app: &TestApp, name: &str, token: String) -> (String, PushService, Browser) {
    let service = PushService::start().await;
    let browser = Browser::at(&service.url);
    let path = format!("/@{name}/devices/laptop");
    request(app, "PUT", &path, &token, &[JSON], browser.device()).await;
    (token, service, browser)
}

async fn household() -> Household {
    let app = TestApp::new().await;
    let bot = individual(&app, "review-bot").await;
    let chris = individual(&app, "chris").await;
    let spouse = individual(&app, "spouse").await;
    request(&app, "PUT", "/@household", &chris, &[], "").await;
    request(&app, "PUT", "/@household/members/@spouse", &chris, &[], "").await;
    decide(&app, &spouse, "spouse", "join").await;
    request(&app, "POST", "/@household", &bot, &[], "hello").await;
    decide(&app, &chris, "household", "allow").await;
    let chris = subscribed(&app, "chris", chris).await;
    let spouse = subscribed(&app, "spouse", spouse).await;
    Household {
        app,
        bot,
        chris,
        spouse,
    }
}

/// Sends `@household` a question from `@review-bot`, and returns its id.
async fn ask(household: &Household) -> String {
    let body = json!({ "summary": "Deploy?", "choices": ["Ship it", "Wait"] }).to_string();
    let (_, _, text) = request(
        &household.app,
        "POST",
        "/@household",
        &household.bot,
        &[JSON],
        body,
    )
    .await;
    serde_json::from_str::<Value>(&text).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

/// The event of each push `service` received, as `browser` reads it.
fn events(service: &PushService, browser: &Browser) -> Vec<String> {
    service
        .received()
        .iter()
        .map(|(_, body)| browser.read(body)["event"].as_str().unwrap().to_owned())
        .collect()
}

/// Marks exchange `id` read with `PUT` or unread with `DELETE`, as
/// `@chris`.
async fn mark(household: &Household, id: &str, method: &str) {
    let path = format!("/exchanges/{id}/@chris");
    request(&household.app, method, &path, &household.chris.0, &[], "").await;
}

#[tokio::test]
async fn reading_clears_the_notification_on_the_readers_devices_only() {
    let household = household().await;
    let id = ask(&household).await;
    mark(&household, &id, "PUT").await;
    deliver_due(&household.app.app, now()).await.unwrap();
    let (_, chris, chris_browser) = &household.chris;
    let (_, spouse, spouse_browser) = &household.spouse;
    assert_eq!(events(chris, chris_browser), ["received", "read"]);
    assert_eq!(events(spouse, spouse_browser), ["received"]);
    let (headers, body) = chris.received().remove(1);
    assert_eq!(
        (
            headers["topic"].to_str().unwrap(),
            chris_browser.read(&body)
        ),
        (id.as_str(), json!({ "event": "read", "id": id }))
    );
}

/// How many pushes `@chris`'s browser receives after an arrival, when he
/// marks it with each of `methods` in turn.
async fn after_marking(methods: &[&str]) -> usize {
    let household = household().await;
    let id = ask(&household).await;
    for method in methods {
        mark(&household, &id, method).await;
    }
    deliver_due(&household.app.app, now()).await.unwrap();
    household.chris.1.received().len() - 1
}

#[tokio::test]
async fn only_a_change_to_read_pushes() {
    let pushed = [
        after_marking(&["PUT"]).await,
        after_marking(&["PUT", "PUT"]).await,
        after_marking(&["DELETE"]).await,
        after_marking(&["PUT", "DELETE"]).await,
        after_marking(&["PUT", "DELETE", "PUT"]).await,
    ];
    assert_eq!(pushed, [1, 1, 0, 1, 2]);
}

#[tokio::test]
async fn a_reply_clears_the_notification_once() {
    let household = household().await;
    let id = ask(&household).await;
    let reply = format!("/exchanges/{id}/reply");
    let (token, chris, browser) = &household.chris;
    let choice = r#"{"choice": "Wait"}"#;
    request(&household.app, "PUT", &reply, token, &[JSON], choice).await;
    mark(&household, &id, "PUT").await;
    deliver_due(&household.app.app, now()).await.unwrap();
    assert_eq!(events(chris, browser), ["received", "ended"]);
}

#[tokio::test]
async fn each_arrival_names_the_reader_it_was_pushed_to() {
    let household = household().await;
    ask(&household).await;
    deliver_due(&household.app.app, now()).await.unwrap();
    let reader = |service: &PushService, browser: &Browser| {
        let (_, body) = service.received().remove(0);
        browser.read(&body)["reader"].clone()
    };
    let (_, chris, chris_browser) = &household.chris;
    let (_, spouse, spouse_browser) = &household.spouse;
    assert_eq!(
        [reader(chris, chris_browser), reader(spouse, spouse_browser)],
        ["chris", "spouse"]
    );
}
