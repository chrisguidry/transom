//! An identity's picture, at `/@{name}/picture`, which clients show beside
//! its name and in its notifications. Whoever manages the identity sets it
//! with a PUT of the image's bytes and removes it with a DELETE. Anyone
//! signed in may see it, as anyone signed in may describe the identity.
//!
//! A picture is a square PNG, JPEG, or WebP, at most 256 KiB and 1024
//! pixels a side. Its type comes from its first bytes, not from what the
//! caller says, and SVG is refused, since it can carry script. Its version
//! is a hash of its bytes. The URLs transom gives for a picture carry the
//! version, so a client that keys a cache by URL sees a new picture as a
//! new URL, and the `ETag` is the version, so a client that kept the bytes
//! asks again and gets `304 Not Modified`.

use std::sync::Arc;

use aws_lc_rs::digest;
use axum::extract::{Path, State};
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE, ETAG, IF_NONE_MATCH, LOCATION};
use axum::http::{HeaderMap, HeaderName, StatusCode};
use axum::response::{IntoResponse, Response};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use bytes::Bytes;
use redis::AsyncCommands;

use super::records::{self, Identity, Lookup};
use super::{caller, dimensions, managed, target};
use crate::app::{App, Failure};
use crate::exchanges::attachments::sniff;
use crate::signin::Verified;

type Outcome = Result<Response, Failure>;

const MOST_BYTES: usize = 256 * 1024;

/// The longest side a picture may have. A phone decodes the whole bitmap
/// to draw even a small icon, and 1024 pixels is more than any icon needs.
const MOST_SIDE: u32 = 1024;

/// Every picture is revalidated before each use, so a client that asks for
/// `/@{name}/picture` with no version still sees a new picture at once.
const CACHE: &str = "private, no-cache";

/// The URL of `identity`'s picture, with its version, when it has one.
/// It is absolute when transom knows its public URL, as an exchange's is.
pub(crate) fn url(app: &App, identity: &Identity) -> Option<String> {
    identity.picture.as_ref().map(|version| {
        let origin = app.public_url.as_deref().unwrap_or("");
        format!("{origin}/@{}/picture?v={version}", identity.name)
    })
}

fn key(id: &str) -> String {
    format!("picture:{id}")
}

/// The type of a picture transom takes, or why it does not take it.
fn check(bytes: &[u8]) -> Result<&'static str, Failure> {
    let refused = |reason: String| Failure::new(StatusCode::UNPROCESSABLE_ENTITY, reason);
    if bytes.len() > MOST_BYTES {
        return Err(Failure::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            format!("a picture is at most {MOST_BYTES} bytes"),
        ));
    }
    let content_type = sniff(bytes)
        .filter(|content_type| *content_type != "image/gif")
        .ok_or_else(|| {
            Failure::new(
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "a picture is a PNG, JPEG, or WebP image",
            )
        })?;
    let (width, height) = dimensions::of(content_type, bytes)
        .ok_or_else(|| refused("the picture's size cannot be read".to_owned()))?;
    if width != height {
        return Err(refused(format!(
            "a picture is square, and this one is {width} by {height} pixels"
        )));
    }
    if width > MOST_SIDE {
        return Err(refused(format!(
            "a picture is at most {MOST_SIDE} pixels a side, and this one is {width}"
        )));
    }
    Ok(content_type)
}

/// The first 16 bytes of the SHA-256 of `bytes`, in URL-safe base64:
/// enough that two pictures never share a version.
fn version(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(&digest::digest(&digest::SHA256, bytes).as_ref()[..16])
}

pub(super) async fn put(
    State(app): State<Arc<App>>,
    Path(path): Path<String>,
    verified: Verified,
    body: Bytes,
) -> Outcome {
    let identity = match managed(&app, &path, &verified).await? {
        Ok(identity) => identity,
        Err(moved) => return Ok(moved),
    };
    let content_type = check(&body)?;
    let version = version(&body);
    let store = &app.store;
    let picture = store.key(&key(&identity.id));
    let mut connection = store.connection().await?;
    let mut pipe = redis::pipe();
    pipe.atomic()
        .hset(&picture, "bytes", body.as_ref())
        .hset(&picture, "type", content_type)
        .hset(&picture, "version", &version)
        .hset(
            store.key(&format!("identity:{}", identity.id)),
            "picture",
            &version,
        );
    let () = pipe.query_async(&mut connection).await?;
    let identity = records::identity(store, &identity.id).await?;
    let webhook = records::webhook(store, &identity.id).await?;
    Ok(super::view(
        &app,
        StatusCode::OK,
        &identity,
        webhook.as_deref(),
    ))
}

pub(super) async fn remove(
    State(app): State<Arc<App>>,
    Path(path): Path<String>,
    verified: Verified,
) -> Outcome {
    let identity = match managed(&app, &path, &verified).await? {
        Ok(identity) => identity,
        Err(moved) => return Ok(moved),
    };
    let store = &app.store;
    let mut connection = store.connection().await?;
    let mut pipe = redis::pipe();
    pipe.atomic()
        .del(store.key(&key(&identity.id)))
        .hdel(store.key(&format!("identity:{}", identity.id)), "picture");
    let () = pipe.query_async(&mut connection).await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// Whether an `If-None-Match` header names `etag`, weakly or not.
fn matches(headers: &HeaderMap, etag: &str) -> bool {
    headers
        .get(IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|tags| {
            tags.split(',')
                .map(|tag| tag.trim().trim_start_matches("W/"))
                .any(|tag| tag == etag || tag == "*")
        })
}

/// Serves the picture to anyone signed in. A former name redirects to the
/// picture at the new one, so an image that follows it still gets an
/// image.
pub(super) async fn serve(
    State(app): State<Arc<App>>,
    Path(path): Path<String>,
    headers: HeaderMap,
    verified: Verified,
) -> Outcome {
    let name = target(&path)?;
    caller(&app, &verified).await?;
    let identity = match records::lookup(&app.store, &name).await? {
        Lookup::Current(identity) => identity,
        Lookup::Former(identity) => {
            let location = format!("/@{}/picture", identity.name);
            return Ok((StatusCode::PERMANENT_REDIRECT, [(LOCATION, location)]).into_response());
        }
        Lookup::Free => {
            return Err(Failure::new(
                StatusCode::NOT_FOUND,
                format!("no one is named {name}"),
            ));
        }
    };
    let mut connection = app.store.connection().await?;
    let (bytes, content_type, version): (Option<Vec<u8>>, Option<String>, Option<String>) =
        connection
            .hmget(
                app.store.key(&key(&identity.id)),
                &["bytes", "type", "version"],
            )
            .await?;
    let (Some(bytes), Some(content_type), Some(version)) = (bytes, content_type, version) else {
        return Err(Failure::new(
            StatusCode::NOT_FOUND,
            format!("@{name} has no picture"),
        ));
    };
    let etag = format!("\"{version}\"");
    if matches(&headers, &etag) {
        let validators = [(ETAG, etag), (CACHE_CONTROL, CACHE.to_owned())];
        return Ok((StatusCode::NOT_MODIFIED, validators).into_response());
    }
    Ok((
        [
            (CONTENT_TYPE, content_type),
            (ETAG, etag),
            (CACHE_CONTROL, CACHE.to_owned()),
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
