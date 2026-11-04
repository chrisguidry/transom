//! Reading a message from a send request: its body in any of the forms
//! transom accepts, its query string, and its headers.

use axum::http::HeaderMap;
use axum::http::header::CONTENT_TYPE;
use bytes::Bytes;

use super::fields::{Document, Fields, is_field};
use super::message::Message;

/// An image sent with a message, in a multipart body.
#[derive(Debug, PartialEq)]
pub(crate) struct Attachment {
    pub(crate) name: String,
    pub(crate) content_type: String,
    pub(crate) bytes: Bytes,
}

#[derive(Debug, PartialEq)]
pub(crate) struct Incoming {
    pub(crate) message: Message,
    pub(crate) attachments: Vec<Attachment>,
}

/// Whether every name in a form is a field of a message. `curl -d text`
/// sends a form content type with any text, so a form whose names are not
/// all fields is read as the message's body instead.
fn is_form(pairs: &[(String, String)]) -> bool {
    !pairs.is_empty() && pairs.iter().all(|(name, _)| is_field(name))
}

fn utf8(body: &[u8]) -> Result<&str, String> {
    std::str::from_utf8(body).map_err(|_| "the body is not UTF-8".to_owned())
}

/// What is wrong with a multipart body that multer could not read.
#[allow(clippy::needless_pass_by_value)]
fn malformed(error: multer::Error) -> String {
    format!("the multipart body is malformed: {error}")
}

fn text_body(body: &Bytes) -> Result<Fields, String> {
    let text = utf8(body)?;
    Ok(Fields {
        body: Some(text.to_owned()),
        ..Fields::default()
    })
}

async fn multipart(content_type: &str, body: Bytes) -> Result<(Fields, Vec<Attachment>), String> {
    let boundary = multer::parse_boundary(content_type).map_err(malformed)?;
    let stream = futures_util::stream::once(async move { Ok::<_, std::convert::Infallible>(body) });
    let mut parts = multer::Multipart::new(stream, boundary);
    let mut pairs = Vec::new();
    let mut attachments = Vec::new();
    while let Some(part) = parts.next_field().await.map_err(malformed)? {
        let name = part.name().unwrap_or_default().to_owned();
        if let Some(file_name) = part.file_name().map(str::to_owned) {
            let content_type = part
                .content_type()
                .map_or("application/octet-stream".to_owned(), ToString::to_string);
            let bytes = part.bytes().await.map_err(malformed)?;
            attachments.push(Attachment {
                name: file_name,
                content_type,
                bytes,
            });
        } else {
            pairs.push((name, part.text().await.map_err(malformed)?));
        }
    }
    let fields = Fields::from_pairs(&pairs)?;
    Ok((fields, attachments))
}

/// Reads the message a send request carries.
pub(crate) async fn read(
    headers: &HeaderMap,
    query: Option<&str>,
    body: Bytes,
) -> Result<Incoming, String> {
    let content_type = headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_owned();
    let essence = content_type
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    let mut attachments = Vec::new();
    let from_body = match essence.as_str() {
        "application/json" => serde_json::from_slice::<Document>(&body)
            .map_err(|error| format!("the JSON body is not a message: {error}"))?
            .try_into()?,
        "application/yaml" | "application/x-yaml" | "text/yaml" => {
            let text = utf8(&body)?;
            serde_saphyr::from_str::<Document>(text)
                .map_err(|error| format!("the YAML body is not a message: {error}"))?
                .try_into()?
        }
        "multipart/form-data" => {
            let (fields, parts) = multipart(&content_type, body).await?;
            attachments = parts;
            fields
        }
        "application/x-www-form-urlencoded" => {
            let pairs: Vec<(String, String)> =
                url::form_urlencoded::parse(&body).into_owned().collect();
            if is_form(&pairs) {
                Fields::from_pairs(&pairs)?
            } else {
                text_body(&body)?
            }
        }
        _ if body.is_empty() => Fields::default(),
        _ => text_body(&body)?,
    };
    let query_pairs: Vec<(String, String)> = query
        .map(|query| {
            url::form_urlencoded::parse(query.as_bytes())
                .into_owned()
                .collect()
        })
        .unwrap_or_default();
    let from_query = Fields::from_pairs(&query_pairs)?;
    let message = from_body
        .merge(from_query)?
        .merge(Fields::from_headers(headers)?)?
        .into_message()?;
    Ok(Incoming {
        message,
        attachments,
    })
}

/// A reply as a JSON or YAML body carries it.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ReplyDocument {
    choice: Option<String>,
    text: Option<String>,
}

/// Reads a reply's `choice` and `text` from its request body: JSON, YAML,
/// a form, or plain text or Markdown, which is the text.
pub(crate) fn read_reply(
    headers: &HeaderMap,
    body: &Bytes,
) -> Result<(Option<String>, Option<String>), String> {
    let content_type = headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    let essence = content_type
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    let document = match essence.as_str() {
        "application/json" => serde_json::from_slice::<ReplyDocument>(body)
            .map_err(|error| format!("the JSON body is not a reply: {error}"))?,
        "application/yaml" | "application/x-yaml" | "text/yaml" => {
            let text = utf8(body)?;
            serde_saphyr::from_str::<ReplyDocument>(text)
                .map_err(|error| format!("the YAML body is not a reply: {error}"))?
        }
        "application/x-www-form-urlencoded"
            if url::form_urlencoded::parse(body)
                .all(|(name, _)| name == "choice" || name == "text") =>
        {
            let mut document = ReplyDocument {
                choice: None,
                text: None,
            };
            for (name, value) in url::form_urlencoded::parse(body) {
                if name == "choice" {
                    document.choice = Some(value.into_owned());
                } else {
                    document.text = Some(value.into_owned());
                }
            }
            document
        }
        _ => ReplyDocument {
            choice: None,
            text: Some(utf8(body)?.to_owned()).filter(|text| !text.is_empty()),
        },
    };
    Ok((document.choice, document.text))
}
