//! How an exchange names its parties: each one's handle, display name,
//! and picture.

use serde_json::{Value, json};

use super::{JSON, decide, individual, send};
use crate::app::testing::{TestApp, request};
use crate::identities::dimensions::testing::png;

/// An exchange `@grafana`, named and pictured, sent to `@chris`, named
/// but not pictured, after `@chris` replied to it.
async fn replied() -> Value {
    let app = TestApp::new().await;
    let grafana = individual(&app, "grafana").await;
    let chris = individual(&app, "chris").await;
    for (token, path, name) in [
        (&grafana, "/@grafana", "Grafana"),
        (&chris, "/@chris", "Chris"),
    ] {
        let named = json!({ "display_name": name }).to_string();
        request(&app, "PATCH", path, token, &[JSON], named).await;
    }
    request(&app, "PUT", "/@grafana/picture", &grafana, &[], png(64, 64)).await;
    send(&app, &grafana, "/@chris", &[], "hello").await;
    decide(&app, &chris, "chris", "allow").await;
    let (_, sent) = send(
        &app,
        &grafana,
        "/@chris",
        &[JSON],
        r#"{"summary": "Disk is filling", "choices": ["Ack"]}"#,
    )
    .await;
    let id = sent["id"].as_str().unwrap();
    let reply = r#"{"choice": "Ack"}"#;
    let path = format!("/exchanges/{id}/reply");
    request(&app, "PUT", &path, &chris, &[JSON], reply).await;
    let (_, _, text) = request(&app, "GET", &format!("/exchanges/{id}"), &chris, &[], "").await;
    serde_json::from_str(&text).unwrap()
}

#[tokio::test]
async fn each_party_has_its_display_name_and_picture_beside_its_handle() {
    let exchange = replied().await;
    let picture = exchange["from_picture"].as_str().unwrap();
    assert!(picture.starts_with("/@grafana/picture?v="), "{picture}");
    assert_eq!(
        [
            &exchange["from"],
            &exchange["from_name"],
            &exchange["to"],
            &exchange["to_name"],
            &exchange["to_picture"],
            &exchange["reply"]["by"],
            &exchange["reply"]["by_name"],
            &exchange["reply"]["by_picture"],
        ],
        [
            &json!("@grafana"),
            &json!("Grafana"),
            &json!("@chris"),
            &json!("Chris"),
            &Value::Null,
            &json!("@chris"),
            &json!("Chris"),
            &Value::Null,
        ]
    );
}
