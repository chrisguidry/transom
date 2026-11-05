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

/// The first line of a message's body, to show under its summary in a
/// list, or nothing when the body's first line is already its title.
fn excerpt(message: &Value) -> String {
    if text(&message["summary"]).is_empty() {
        return String::new();
    }
    let body = text(&message["body"]);
    let line = body
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or_default();
    let mut excerpt: String = line.chars().take(120).collect();
    if line.chars().count() > 120 {
        excerpt.push('…');
    }
    excerpt
}

fn page(template: &impl Template) -> Result<Response, Failure> {
    let html = template
        .render()
        .map_err(|error| Failure::new(StatusCode::INTERNAL_SERVER_ERROR, error))?;
    Ok(Html(html).into_response())
}

/// Up to two letters for a circle that stands in for a missing picture:
/// the first letter of each of the first two words of a name, where a
/// handle's words are split at its dots and hyphens.
pub(super) fn initials(name: &str) -> String {
    let initials: String = name
        .split(|c: char| c.is_whitespace() || c == '-' || c == '.')
        .filter_map(|word| word.chars().next())
        .take(2)
        .flat_map(char::to_uppercase)
        .collect();
    if initials.is_empty() {
        "?".to_owned()
    } else {
        initials
    }
}

/// A picture's URL as a page loads it, from the page's own origin: the
/// page's Content Security Policy allows images from nowhere else, and
/// the browser may have reached transom by another name than its public
/// URL.
fn same_origin(url: &str) -> String {
    url::Url::parse(url).map_or_else(
        |_| url.to_owned(),
        |url| url[url::Position::BeforePath..].to_owned(),
    )
}

/// A party to an exchange, as a page shows it: by its display name, or
/// its handle when it has none, and by its picture, or its initials.
pub(crate) struct Party {
    handle: String,
    name: String,
    /// Whether the name is a display name, which the exchange page shows
    /// the handle beside.
    named: bool,
    picture: Option<String>,
    initials: String,
}

impl Party {
    /// The party the exchange view names under `field`.
    fn of(exchange: &Value, field: &str) -> Self {
        let handle = text(&exchange[field]);
        let display_name = exchange[format!("{field}_name")].as_str();
        let name = display_name.unwrap_or(&handle).to_owned();
        Self {
            initials: initials(name.trim_start_matches('@')),
            named: display_name.is_some(),
            picture: exchange[format!("{field}_picture")]
                .as_str()
                .map(same_origin),
            handle,
            name,
        }
    }
}

pub(crate) struct Card {
    id: String,
    summary: String,
    excerpt: String,
    from: Party,
    to: Party,
    /// Whose picture the row shows: whoever sent it, or for one the
    /// viewer sent, whoever it went to.
    pictured: Party,
    when: String,
    state: String,
    /// Read by the viewer, or sent by the viewer, which has no read state.
    read: bool,
}

impl Card {
    fn of(exchange: &Value, me: &str) -> Self {
        let other = if exchange["from"] == me { "to" } else { "from" };
        Self {
            id: text(&exchange["id"]),
            summary: title(&exchange["message"]),
            excerpt: excerpt(&exchange["message"]),
            from: Party::of(exchange, "from"),
            to: Party::of(exchange, "to"),
            pictured: Party::of(exchange, other),
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
    /// The viewer, whose read state the page shows and marks.
    reader: String,
    name: String,
    waiting: Vec<Card>,
    unread: Vec<Card>,
    others: Vec<Card>,
}

/// An identity's list: open questions sent to the viewer first, then what
/// the viewer has not read, then the rest, newest first in each.
pub(crate) fn inbox(me: &str, name: &str, exchanges: &[Value]) -> Result<Response, Failure> {
    let me_name = format!("@{me}");
    let (waiting, rest): (Vec<&Value>, Vec<&Value>) = exchanges.iter().partition(|exchange| {
        exchange["question"] == true
            && exchange["state"] == "open"
            && exchange["from"] != me_name.as_str()
    });
    let (unread, others) = rest
        .into_iter()
        .map(|exchange| Card::of(exchange, &me_name))
        .partition(|card| !card.read);
    page(&Inbox {
        me: Some(me.to_owned()),
        reader: me.to_owned(),
        name: name.to_owned(),
        waiting: waiting
            .into_iter()
            .map(|exchange| Card::of(exchange, &me_name))
            .collect(),
        unread,
        others,
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
    from: Party,
    to: Party,
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
        by: reply.get("by_name").and_then(Value::as_str).map_or_else(
            || reply.get("by").map(text).unwrap_or_default(),
            str::to_owned,
        ),
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
            let label = match (link["title"].as_str(), link["rel"].as_str()) {
                (Some(title), _) => title.to_owned(),
                (None, Some(rel)) => format!("{rel}: {href}"),
                (None, None) => href.clone(),
            };
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
        from: Party::of(exchange, "from"),
        to: Party::of(exchange, "to"),
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
                Wake::WebPush(_) => "Web Push",
                Wake::Connection(_) => "Direct connection",
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
