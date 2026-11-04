use axum::http::StatusCode;
use serde_json::json;

use super::{JSON, pair, send};
use crate::app::testing::request;

#[tokio::test]
async fn plain_text_is_the_body() {
    let pair = pair().await;
    let (status, exchange) = send(
        &pair.app,
        &pair.sender,
        "/@chris",
        &[("content-type", "text/plain")],
        "Backup finished in 4m",
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(exchange["message"]["body"], "Backup finished in 4m");
    assert_eq!(exchange["state"], "open");
    assert_eq!(exchange["question"], false);
    assert_eq!(
        (exchange["from"].as_str(), exchange["to"].as_str()),
        (Some("@review-bot"), Some("@chris"))
    );
}

#[tokio::test]
async fn curls_default_form_type_with_plain_text_is_the_body() {
    let pair = pair().await;
    let form = ("content-type", "application/x-www-form-urlencoded");
    let (_, exchange) = send(
        &pair.app,
        &pair.sender,
        "/@chris",
        &[form],
        "Backup finished in 4m",
    )
    .await;
    assert_eq!(exchange["message"]["body"], "Backup finished in 4m");
}

#[tokio::test]
async fn a_form_of_fields_is_a_message() {
    let pair = pair().await;
    let form = ("content-type", "application/x-www-form-urlencoded");
    let body = "summary=Restart%3F&choices=Restart&choices=Leave+it&ttl=1h&urgency=high&link=https%3A%2F%2Fgrafana%2Fd&metadata.session=abc";
    let (_, exchange) = send(&pair.app, &pair.sender, "/@chris", &[form], body).await;
    let message = &exchange["message"];
    assert_eq!(
        message["choices"],
        json!([{"label": "Restart"}, {"label": "Leave it"}])
    );
    assert_eq!(
        (message["ttl"].as_u64(), message["urgency"].as_str()),
        (Some(3600), Some("high"))
    );
    assert_eq!(message["link"], json!([{"href": "https://grafana/d"}]));
    assert_eq!(message["metadata"], json!({"session": "abc"}));
    assert!(exchange["expires"].is_string());
}

#[tokio::test]
async fn json_carries_every_field() {
    let pair = pair().await;
    let body = json!({
        "summary": "Restart postgres?", "body": "The disk is at 94%.",
        "choices": ["Leave it", {"label": "Restart", "id": "restart", "description": "About 30 seconds down."}],
        "prompt": "", "ttl": 600, "urgency": "low",
        "link": ["https://grafana/d", {"href": "https://runbooks/disk", "rel": "help"}],
        "metadata": {"session": "abc"},
    });
    let (status, exchange) = send(
        &pair.app,
        &pair.sender,
        "/@chris",
        &[JSON],
        &body.to_string(),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(exchange["question"], true);
    assert_eq!(exchange["message"]["prompt"], "");
    assert_eq!(
        exchange["message"]["link"][1],
        json!({"href": "https://runbooks/disk", "rel": "help"})
    );
}

#[tokio::test]
async fn yaml_is_a_message_too() {
    let pair = pair().await;
    let body = "summary: Restart postgres?\nchoices: [Restart, Leave it]\nttl: 90m\n";
    let (_, exchange) = send(
        &pair.app,
        &pair.sender,
        "/@chris",
        &[("content-type", "application/yaml")],
        body,
    )
    .await;
    assert_eq!(exchange["message"]["ttl"], 5400);
}

#[tokio::test]
async fn headers_and_the_query_carry_fields() {
    let pair = pair().await;
    let headers = [
        ("content-type", "text/markdown"),
        ("summary", "Restart postgres?"),
        ("choices", r#""Restart now", Hold"#),
        ("prompt", "Why not?"),
        ("ttl", "60"),
        ("urgency", "very-low"),
        ("link", r#"<https://grafana/d>; rel="related""#),
        ("metadata", r#"session="abc", run=7"#),
    ];
    let (_, exchange) = send(
        &pair.app,
        &pair.sender,
        "/@chris?summary=Restart+postgres%3F",
        &headers,
        "**Context**",
    )
    .await;
    let message = &exchange["message"];
    assert_eq!(
        message["choices"],
        json!([{"label": "Restart now"}, {"label": "Hold"}])
    );
    assert_eq!(message["metadata"], json!({"run": "7", "session": "abc"}));
    assert_eq!(
        message["link"],
        json!([{"href": "https://grafana/d", "rel": "related"}])
    );
    assert_eq!(message["body"], "**Context**");
}

#[tokio::test]
async fn a_field_given_two_ways_is_a_bad_request() {
    let pair = pair().await;
    let (status, _) = send(
        &pair.app,
        &pair.sender,
        "/@chris?summary=One",
        &[("summary", "Two")],
        "body",
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn bad_messages_are_bad_requests() {
    let pair = pair().await;
    let bad: [(&[(&str, &str)], &str); 11] = [
        (&[JSON], "{\"summary\": \"x\", \"unknown\": 1}"),
        (&[JSON], "{}"),
        (
            &[("content-type", "application/yaml")],
            "summary: [unclosed",
        ),
        (&[("choices", "(inner list)")], "body"),
        (&[("metadata", "a=(1 2)")], "body"),
        (&[("metadata", "not a dictionary!")], "body"),
        (&[("metadata", "flag=?1")], "body"),
        (&[("ttl", "soon")], "body"),
        (&[("urgency", "urgent")], "body"),
        (&[("link", "no brackets")], "body"),
        (
            &[("content-type", "application/x-www-form-urlencoded")],
            "summary=x&ttl=0",
        ),
    ];
    for (headers, body) in bad {
        let (status, _) = send(&pair.app, &pair.sender, "/@chris", headers, body).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{headers:?} {body}");
    }
}

#[tokio::test]
async fn metadata_can_be_a_display_string() {
    let pair = pair().await;
    let headers = [("metadata", r#"place=%"caf%c3%a9""#)];
    let (_, exchange) = send(&pair.app, &pair.sender, "/@chris", &headers, "body").await;
    assert_eq!(exchange["message"]["metadata"]["place"], "café");
}

#[tokio::test]
async fn a_multipart_field_that_is_not_a_field_is_a_bad_request() {
    let pair = pair().await;
    let body = "--b\r\nContent-Disposition: form-data; name=\"colour\"\r\n\r\nred\r\n--b--\r\n";
    let (status, _) = send(
        &pair.app,
        &pair.sender,
        "/@chris",
        &[("content-type", "multipart/form-data; boundary=b")],
        body,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn unknown_query_fields_are_bad_requests() {
    let pair = pair().await;
    let (status, _) = send(&pair.app, &pair.sender, "/@chris?colour=red", &[], "body").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a_multipart_body_carries_fields_and_attachments() {
    let pair = pair().await;
    let boundary = "transom-boundary";
    let body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"summary\"\r\n\r\nThe disk graph\r\n\
         --{boundary}\r\nContent-Disposition: form-data; name=\"attachments\"; filename=\"disk.png\"\r\nContent-Type: image/png\r\n\r\nPNG\r\n\
         --{boundary}--\r\n"
    );
    let content_type = format!("multipart/form-data; boundary={boundary}");
    let (status, exchange) = send(
        &pair.app,
        &pair.sender,
        "/@chris",
        &[("content-type", &content_type)],
        &body,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(exchange["message"]["summary"], "The disk graph");
}

#[tokio::test]
async fn sending_to_a_name_no_one_holds_is_not_found() {
    let pair = pair().await;
    let (status, _) = send(&pair.app, &pair.sender, "/@nobody", &[], "hello").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn sending_to_a_former_name_redirects_and_keeps_the_post() {
    let pair = pair().await;
    let rename = json!({"name": "guid.foo"}).to_string();
    request(
        &pair.app,
        "PATCH",
        "/@chris",
        &pair.recipient,
        &[JSON],
        rename,
    )
    .await;
    let (status, headers, _) =
        request(&pair.app, "POST", "/@chris", &pair.sender, &[], "hello").await;
    assert_eq!(status, StatusCode::PERMANENT_REDIRECT);
    assert_eq!(headers["location"], "/@guid.foo");
}

#[tokio::test]
async fn a_send_answers_with_the_exchanges_location() {
    let pair = pair().await;
    let (status, headers, _) =
        request(&pair.app, "POST", "/@chris", &pair.sender, &[], "hello").await;
    assert_eq!(status, StatusCode::CREATED);
    assert!(
        headers["location"]
            .to_str()
            .unwrap()
            .starts_with("/exchanges/")
    );
}

#[tokio::test]
async fn a_form_field_that_does_not_parse_is_a_bad_request() {
    let pair = pair().await;
    let form = ("content-type", "application/x-www-form-urlencoded");
    let (status, _) = send(
        &pair.app,
        &pair.sender,
        "/@chris",
        &[form],
        "summary=x&ttl=soon",
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a_body_that_is_not_utf8_is_a_bad_request() {
    let pair = pair().await;
    let not_utf8 = vec![0xff, 0xfe, 0xfd];
    for content_type in ["text/plain", "application/yaml"] {
        let headers = [("content-type", content_type)];
        let (status, _, _) = request(
            &pair.app,
            "POST",
            "/@chris",
            &pair.sender,
            &headers,
            not_utf8.clone(),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{content_type}");
    }
}

#[tokio::test]
async fn a_malformed_multipart_body_is_a_bad_request() {
    let pair = pair().await;
    let headers = [("content-type", "multipart/form-data; boundary=b")];
    let unterminated = "--b\r\nContent-Disposition: form-data; name=\"summary\"\r\n\r\nno end";
    let (status, _) = send(&pair.app, &pair.sender, "/@chris", &headers, unterminated).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}
