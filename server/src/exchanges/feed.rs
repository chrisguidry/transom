//! The live feed: an identity's exchanges, at `/@{name}/exchanges`, as a
//! stream of Server-Sent Events that sends each exchange in the list again
//! when it changes.

use std::collections::BTreeSet;
use std::sync::Arc;

use axum::response::sse::Event;
use axum::response::{IntoResponse, Response};
use serde_json::json;

use super::changes::{Notice, identity_channel};
use super::records::{self, Exchange, Found};
use super::waiting::{self, Step};
use super::{is_recipient, view};
use crate::app::{App, Failure};
use crate::identities::records::{self as identities, Identity, Kind};

/// One open feed: the identity whose list it is, the individual reading
/// it, and the groups whose channels it subscribed to.
struct Feed {
    identity: Identity,
    viewer: String,
    groups: BTreeSet<String>,
}

impl Feed {
    /// Whether the feed may go on as it subscribed: an individual's feed
    /// while its groups are the ones it subscribed to, and a group's feed
    /// while its reader is a member.
    async fn current(&self, app: &App) -> Result<bool, Failure> {
        if self.identity.kind == Kind::Group {
            return Ok(identities::is_member(&app.store, &self.identity.id, &self.viewer).await?);
        }
        let groups = identities::groups_of(&app.store, &self.identity.id).await?;
        Ok(groups.into_iter().collect::<BTreeSet<_>>() == self.groups)
    }

    /// Whether `exchange` is in the identity's list. A group's feed also
    /// hears its reader's own channel, which carries the reader's other
    /// exchanges.
    fn lists(&self, exchange: &Exchange) -> bool {
        exchange.sender == self.identity.id
            || exchange.recipient == self.identity.id
            || self.groups.contains(&exchange.recipient)
    }

    /// What the feed sends for `notice`, or for `None` when it opens.
    async fn step(&self, app: &App, notice: Option<Notice>) -> Result<Step, Failure> {
        let id = match notice {
            // When the feed opens, this also catches a group joined or
            // left between reading the groups and subscribing. Ending the
            // feed makes its client reconnect, and subscribe again.
            None | Some(Notice::Groups) => {
                return Ok(Step {
                    events: vec![],
                    done: !self.current(app).await?,
                });
            }
            Some(Notice::Exchange(id)) => id,
        };
        let event = match records::load(&app.store, &id).await? {
            Found::Exchange(exchange) if self.lists(&exchange) => {
                let mut view = view(app, &exchange, &self.viewer).await?;
                // Read state is private to each recipient, as in the list.
                if is_recipient(app, &exchange, &self.viewer).await? {
                    let read = records::read_ids(&app.store, &self.viewer).await?;
                    view["read"] = json!(read.contains(&id));
                }
                Event::default().event("exchange").json_data(view)
            }
            Found::Exchange(_) => {
                return Ok(Step {
                    events: vec![],
                    done: false,
                });
            }
            // An exchange of a deleted group is gone, and the feed says so,
            // for a client to drop it.
            Found::Gone | Found::Missing => Event::default()
                .event("gone")
                .json_data(json!({ "id": id })),
        };
        Ok(Step {
            events: vec![event.expect("an exchange serializes")],
            done: false,
        })
    }
}

/// Follows `identity`'s list as `viewer`, who may read it.
pub(super) async fn follow(
    app: Arc<App>,
    identity: Identity,
    viewer: String,
) -> Result<Response, Failure> {
    let groups: BTreeSet<String> = if identity.kind == Kind::Individual {
        identities::groups_of(&app.store, &identity.id)
            .await?
            .into_iter()
            .collect()
    } else {
        BTreeSet::new()
    };
    // The reader's own channel says when the reader joins or leaves a
    // group, which a group's feed needs to hear too.
    let channels: BTreeSet<String> = [&identity.id, &viewer]
        .into_iter()
        .chain(&groups)
        .map(|id| identity_channel(id))
        .collect();
    let channels: Vec<String> = channels.into_iter().collect();
    let feed = Arc::new(Feed {
        identity,
        viewer,
        groups,
    });
    let stream_app = app.clone();
    let sse = waiting::stream(&app.store, &channels, move |payload| {
        let (app, feed) = (stream_app.clone(), feed.clone());
        async move { feed.step(&app, payload.map(Notice::parse)).await }
    })
    .await?;
    Ok(sse.into_response())
}
