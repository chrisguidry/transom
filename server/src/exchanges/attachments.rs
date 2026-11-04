//! Images sent with a message. The body places one inline by naming it,
//! as `![the graph](graph.png)`, and transom serves each one at
//! `/exchanges/{id}/attachments/{name}`, to whoever may see the exchange.
//!
//! - `attachments:{id}`, a hash of an exchange's attachments, by name, to
//!   their bytes. The message lists each one's name, type, and size.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::header::{CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_TYPE};
use axum::http::{HeaderName, StatusCode};
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};

use crate::app::{App, Failure};
use crate::identities::caller;
use crate::signin::Verified;

/// The most one image may be.
pub(crate) const MOST_EACH: usize = 5 * 1024 * 1024;

/// The most images one message may carry, and their most in all.
const MOST_IMAGES: usize = 10;
const MOST_IN_ALL: usize = 20 * 1024 * 1024;

/// An image as it arrives: the name the sender gave it, and its bytes.
#[derive(Debug, PartialEq)]
pub(crate) struct Attachment {
    pub(crate) name: String,
    pub(crate) bytes: Bytes,
}

/// What the message records of an attachment.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub(crate) struct Attached {
    pub(crate) name: String,
    #[serde(rename = "type")]
    pub(crate) content_type: String,
    pub(crate) size: usize,
}

pub(crate) fn key(id: &str) -> String {
    format!("attachments:{id}")
}

/// The type of an image, from its first bytes rather than what the
/// sender said: PNG, JPEG, GIF, or WebP. SVG is not an image here, since
/// it can carry script.
fn sniff(bytes: &[u8]) -> Option<&'static str> {
    match bytes {
        [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, ..] => Some("image/png"),
        [0xFF, 0xD8, 0xFF, ..] => Some("image/jpeg"),
        [b'G', b'I', b'F', b'8', b'7' | b'9', b'a', ..] => Some("image/gif"),
        [
            b'R',
            b'I',
            b'F',
            b'F',
            _,
            _,
            _,
            _,
            b'W',
            b'E',
            b'B',
            b'P',
            ..,
        ] => Some("image/webp"),
        _ => None,
    }
}

/// A name that is safe in a URL and a header: letters, digits, `.`, `-`,
/// and `_`, with anything else made a `-`.
fn plain(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or_default();
    let plain: String = base
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '-'
            }
        })
        .take(100)
        .collect();
    let plain = plain.trim_start_matches('.');
    if plain.is_empty() {
        "image".to_owned()
    } else {
        plain.to_owned()
    }
}

/// Checks a message's attachments: images, within the limits, each with
/// a name of its own. Returns them with their names made plain, and what
/// the message records of each.
pub(crate) fn check(
    attachments: Vec<Attachment>,
) -> Result<(Vec<Attachment>, Vec<Attached>), Failure> {
    let too_large = |reason: String| Failure::new(StatusCode::PAYLOAD_TOO_LARGE, reason);
    let refused = |reason: String| Failure::new(StatusCode::BAD_REQUEST, reason);
    if attachments.len() > MOST_IMAGES {
        return Err(refused(format!(
            "a message carries at most {MOST_IMAGES} images"
        )));
    }
    if attachments.iter().map(|a| a.bytes.len()).sum::<usize>() > MOST_IN_ALL {
        return Err(too_large(format!(
            "a message's images come to at most {MOST_IN_ALL} bytes"
        )));
    }
    let mut checked = Vec::new();
    let mut attached: Vec<Attached> = Vec::new();
    for attachment in attachments {
        let name = plain(&attachment.name);
        if attachment.bytes.len() > MOST_EACH {
            return Err(too_large(format!(
                "{name} is larger than {MOST_EACH} bytes"
            )));
        }
        let content_type = sniff(&attachment.bytes)
            .ok_or_else(|| refused(format!("{name} is not a PNG, JPEG, GIF, or WebP image")))?;
        if attached.iter().any(|other| other.name == name) {
            return Err(refused(format!("two attachments are named {name}")));
        }
        attached.push(Attached {
            name: name.clone(),
            content_type: content_type.to_owned(),
            size: attachment.bytes.len(),
        });
        checked.push(Attachment {
            name,
            bytes: attachment.bytes,
        });
    }
    Ok((checked, attached))
}

/// Keeps an exchange's attachments.
pub(crate) async fn store(
    app: &App,
    id: &str,
    attachments: &[Attachment],
) -> redis::RedisResult<()> {
    if attachments.is_empty() {
        return Ok(());
    }
    let mut connection = app.store.connection().await?;
    let items: Vec<(&str, &[u8])> = attachments
        .iter()
        .map(|attachment| (attachment.name.as_str(), attachment.bytes.as_ref()))
        .collect();
    connection
        .hset_multiple(app.store.key(&key(id)), &items)
        .await
}

/// Serves one attachment, to whoever may see its exchange. The image is
/// sandboxed, so opening it alone can run nothing.
pub(crate) async fn serve(
    State(app): State<Arc<App>>,
    Path((id, name)): Path<(String, String)>,
    verified: Verified,
) -> Result<Response, Failure> {
    let viewer = caller(&app, &verified).await?;
    let exchange = super::visible(&app, &id, &viewer.id).await?;
    let missing = || {
        Failure::new(
            StatusCode::NOT_FOUND,
            format!("exchange {id} has no {name}"),
        )
    };
    let attached = exchange
        .message
        .attachments
        .iter()
        .find(|attached| attached.name == name)
        .ok_or_else(missing)?;
    let mut connection = app.store.connection().await?;
    let bytes: Option<Vec<u8>> = connection.hget(app.store.key(&key(&id)), &name).await?;
    let bytes = bytes.ok_or_else(missing)?;
    Ok((
        [
            (CONTENT_TYPE, attached.content_type.clone()),
            (
                CACHE_CONTROL,
                "private, max-age=31536000, immutable".to_owned(),
            ),
            (CONTENT_DISPOSITION, format!("inline; filename=\"{name}\"")),
            (
                HeaderName::from_static("content-security-policy"),
                "default-src 'none'; sandbox".to_owned(),
            ),
            (
                HeaderName::from_static("x-content-type-options"),
                "nosniff".to_owned(),
            ),
        ],
        bytes,
    )
        .into_response())
}
