//! The pages: each builds what its template shows from the same JSON view
//! of an exchange that the API returns, so a page never shows what the
//! API would not.

use askama::Template;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use serde_json::Value;

use super::inbox::Inbox;
use super::{markdown, qr};
use crate::app::Failure;
use crate::push::devices::{Device, Wake};
use crate::signin::app::AppSignIn;

pub(super) fn text(value: &Value) -> String {
    value.as_str().unwrap_or_default().to_owned()
}

/// A time as a page says it in full, `Thu, Nov 5 at 17:29 UTC`, with the
/// RFC 3339 time it came from, which the page's script says again in the
/// browser's own time zone and clock.
pub(crate) struct LongWhen {
    pub(crate) iso: String,
    pub(crate) text: String,
}

pub(super) fn when(value: &Value) -> LongWhen {
    let iso = text(value);
    let text = iso.parse::<jiff::Timestamp>().map_or_else(
        |_| iso.clone(),
        |at| at.strftime("%a, %b %-d at %H:%M UTC").to_string(),
    );
    LongWhen { iso, text }
}

/// Where an exchange in `state` stands, in words, as its page says it, or
/// nothing while it is open. The reply, when there is one, says the rest.
pub(super) fn standing(state: &str, question: bool) -> Option<&'static str> {
    match state {
        "pending" => Some("Waiting to be accepted"),
        "replied" => Some("Answered"),
        "expired" if question => Some("Expired unanswered"),
        "expired" => Some("Expired"),
        "withdrawn" => Some("Withdrawn"),
        "declined" => Some("Declined"),
        "replaced" => Some("Replaced by a newer message"),
        _ => None,
    }
}

/// What a message is called in a list: its summary, or the first line of
/// its body without its Markdown, cut at 80 characters.
pub(super) fn title(message: &Value) -> String {
    let summary = text(&message["summary"]);
    if !summary.is_empty() {
        return summary;
    }
    let line = markdown::first_line(message["body"].as_str().unwrap_or_default());
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
    pub(super) fn of(exchange: &Value, field: &str) -> Self {
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

#[derive(Template)]
#[template(path = "inbox.html")]
struct InboxPage {
    me: Option<String>,
    /// The viewer, whose read state the page shows and marks.
    reader: String,
    name: String,
    inbox: Inbox,
    /// The ids Mark all read marks, as the form sends them.
    unread_ids: Vec<String>,
}

/// An identity's list, as `inbox::inbox` sorts it: what waits on the
/// viewer, then what the viewer has not read, then everything earlier.
pub(crate) fn inbox(me: &str, name: &str, exchanges: &[Value]) -> Result<Response, Failure> {
    let inbox = super::inbox::inbox(exchanges, &format!("@{me}"), jiff::Timestamp::now());
    page(&InboxPage {
        me: Some(me.to_owned()),
        reader: me.to_owned(),
        name: name.to_owned(),
        unread_ids: inbox.unread_ids().into_iter().map(str::to_owned).collect(),
        inbox,
    })
}

pub(crate) struct ChoicePage {
    key: String,
    label: String,
    description: String,
}

pub(crate) struct ReplyPage {
    by: String,
    when: LongWhen,
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
    when: LongWhen,
    /// Where the exchange stands, in words, unless it is still open.
    standing: Option<&'static str>,
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
        when: when(reply.get("at").unwrap_or(&Value::Null)),
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
        standing: standing(&state, exchange["question"] == true),
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
