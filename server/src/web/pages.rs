//! The pages: each builds what its template shows from the same JSON view
//! of an exchange that the API returns, so a page never shows what the
//! API would not.

use askama::Template;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use serde_json::Value;

use super::{markdown, qr};
use crate::app::Failure;
use crate::push::devices::{Device, Wake};
use crate::signin::app::AppSignIn;

fn text(value: &Value) -> String {
    value.as_str().unwrap_or_default().to_owned()
}

/// "2026-11-03 21:14 UTC" from an RFC 3339 timestamp.
fn when(value: &Value) -> String {
    let text = text(value);
    text.parse::<jiff::Timestamp>()
        .map_or(text, |at| at.strftime("%Y-%m-%d %H:%M UTC").to_string())
}

/// What a message is called in a list: its summary, or the start of its
/// body.
fn title(message: &Value) -> String {
    let summary = text(&message["summary"]);
    if !summary.is_empty() {
        return summary;
    }
    let body = text(&message["body"]);
    let line = body.lines().next().unwrap_or_default();
    let mut title: String = line.chars().take(80).collect();
    if line.chars().count() > 80 {
        title.push('…');
    }
    title
}

fn page(template: &impl Template) -> Result<Response, Failure> {
    let html = template
        .render()
        .map_err(|error| Failure::new(StatusCode::INTERNAL_SERVER_ERROR, error))?;
    Ok(Html(html).into_response())
}

pub(crate) struct Card {
    id: String,
    summary: String,
    from: String,
    to: String,
    when: String,
    state: String,
    read: bool,
}

impl Card {
    fn of(exchange: &Value) -> Self {
        Self {
            id: text(&exchange["id"]),
            summary: title(&exchange["message"]),
            from: text(&exchange["from"]),
            to: text(&exchange["to"]),
            when: when(&exchange["created"]),
            state: text(&exchange["state"]),
            read: exchange["read"].as_bool().unwrap_or(true),
        }
    }
}

#[derive(Template)]
#[template(path = "inbox.html")]
struct Inbox {
    me: Option<String>,
    name: String,
    waiting: Vec<Card>,
    others: Vec<Card>,
}

/// An identity's list: open questions sent to the viewer first, then the
/// rest, newest first in each.
pub(crate) fn inbox(me: &str, name: &str, exchanges: &[Value]) -> Result<Response, Failure> {
    let me_name = format!("@{me}");
    let (waiting, others): (Vec<&Value>, Vec<&Value>) = exchanges.iter().partition(|exchange| {
        exchange["question"] == true
            && exchange["state"] == "open"
            && exchange["from"] != me_name.as_str()
    });
    page(&Inbox {
        me: Some(me.to_owned()),
        name: name.to_owned(),
        waiting: waiting.into_iter().map(Card::of).collect(),
        others: others.into_iter().map(Card::of).collect(),
    })
}

pub(crate) struct ChoicePage {
    key: String,
    label: String,
    description: String,
}

pub(crate) struct ReplyPage {
    by: String,
    when: String,
    choice: Option<String>,
    text: Option<String>,
}

#[derive(Template)]
#[template(path = "exchange.html")]
struct ExchangePage {
    me: Option<String>,
    id: String,
    summary: String,
    body: String,
    /// The images the body did not place: each one's name and URL.
    images: Vec<(String, String)>,
    links: Vec<(String, String)>,
    from: String,
    to: String,
    when: String,
    state: String,
    reply: Option<ReplyPage>,
    choices: Vec<ChoicePage>,
    prompt: String,
    reader: Option<String>,
    can_reply: bool,
    can_withdraw: bool,
    live: bool,
}

/// What the viewer may do with an exchange.
pub(crate) struct Role {
    pub(crate) recipient: bool,
    pub(crate) sender: bool,
}

pub(crate) fn exchange(me: &str, exchange: &Value, role: &Role) -> Result<Response, Failure> {
    let message = &exchange["message"];
    let id = text(&exchange["id"]);
    let attached: Vec<(String, String)> = message["attachments"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .map(|attachment| {
            let name = text(&attachment["name"]);
            let url = format!("/exchanges/{id}/attachments/{name}");
            (name, url)
        })
        .collect();
    let (body, shown) =
        markdown::render_with(message["body"].as_str().unwrap_or_default(), &attached);
    // The images the body did not place are shown after it.
    let images = attached
        .into_iter()
        .filter(|(name, _)| !shown.contains(name))
        .collect();
    let state = text(&exchange["state"]);
    let choices: Vec<ChoicePage> = message["choices"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .map(|choice| ChoicePage {
            key: choice["id"]
                .as_str()
                .unwrap_or(choice["label"].as_str().unwrap_or_default())
                .to_owned(),
            label: text(&choice["label"]),
            description: markdown::render(choice["description"].as_str().unwrap_or_default()),
        })
        .collect();
    let label_of = |key: &str| {
        choices
            .iter()
            .find(|choice| choice.key == key)
            .map_or(key.to_owned(), |choice| choice.label.clone())
    };
    let reply = exchange["reply"].as_object().map(|reply| ReplyPage {
        by: reply.get("by").map(text).unwrap_or_default(),
        when: reply.get("at").map(when).unwrap_or_default(),
        choice: reply.get("choice").and_then(Value::as_str).map(label_of),
        text: reply.get("text").and_then(Value::as_str).map(str::to_owned),
    });
    let links = message["link"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .map(|link| {
            let href = text(&link["href"]);
            let label = link["rel"]
                .as_str()
                .map_or(href.clone(), |rel| format!("{rel}: {href}"));
            (href, label)
        })
        .collect();
    let open = state == "open";
    page(&ExchangePage {
        me: Some(me.to_owned()),
        id,
        summary: title(message),
        body,
        images,
        links,
        from: text(&exchange["from"]),
        to: text(&exchange["to"]),
        when: when(&exchange["created"]),
        prompt: message["prompt"]
            .as_str()
            .filter(|prompt| !prompt.is_empty())
            .unwrap_or("Add a note, if you like")
            .to_owned(),
        reader: role.recipient.then(|| me.to_owned()),
        can_reply: role.recipient && open,
        can_withdraw: role.sender && (open || state == "pending"),
        live: open || state == "pending",
        state,
        reply,
        choices,
    })
}

#[derive(Template)]
#[template(path = "members.html")]
struct MembersPage {
    me: Option<String>,
    group: String,
    members: Vec<String>,
}

pub(crate) fn members(me: &str, group: &str, members: Vec<String>) -> Result<Response, Failure> {
    page(&MembersPage {
        me: Some(me.to_owned()),
        group: group.to_owned(),
        members,
    })
}

pub(crate) struct DeviceRow {
    id: String,
    name: String,
    wakes: &'static str,
}

#[derive(Template)]
#[template(path = "devices.html")]
struct DevicesPage {
    me: Option<String>,
    name: String,
    devices: Vec<DeviceRow>,
    server: String,
    qr: Option<String>,
}

/// The identity's devices, each with how it wakes, and the code a phone
/// scans to learn the server's address. The address is all the code
/// holds: the app signs in through the server as a browser does.
pub(crate) fn devices(
    me: &str,
    devices: Vec<(String, Device)>,
    server: String,
) -> Result<Response, Failure> {
    let mut devices: Vec<DeviceRow> = devices
        .into_iter()
        .map(|(id, device)| DeviceRow {
            id,
            name: device.name,
            wakes: match device.wake {
                Wake::WebPush(_) => "Wakes by Web Push through its browser",
                Wake::Connection(_) => "Holds a connection open to transom",
                Wake::Fcm(_) => "Google Play services",
            },
        })
        .collect();
    devices.sort_by(|one, other| (&one.name, &one.id).cmp(&(&other.name, &other.id)));
    page(&DevicesPage {
        me: Some(me.to_owned()),
        name: me.to_owned(),
        devices,
        qr: qr::svg(&server),
        server,
    })
}

pub(crate) struct IssuerLink {
    name: String,
    label: String,
}

#[derive(Template)]
#[template(path = "login.html")]
struct Login {
    me: Option<String>,
    issuers: Vec<IssuerLink>,
    next: String,
    app: Option<AppSignIn>,
}

pub(crate) fn login(
    issuers: Vec<(String, String)>,
    next: &str,
    app: Option<AppSignIn>,
) -> Result<Response, Failure> {
    page(&Login {
        me: None,
        issuers: issuers
            .into_iter()
            .map(|(name, label)| IssuerLink { name, label })
            .collect(),
        next: next.to_owned(),
        app,
    })
}
