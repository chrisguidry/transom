//! Consent, and the other exchanges `@transom` sends. A sender reaches a
//! recipient only once the recipient allows it. Until then its messages
//! are held as `pending`, and `@transom` asks the recipient, in an exchange
//! of its own, to allow or decline the sender.

use redis::AsyncCommands;

use super::changes;
use super::message::{Choice, Message, Urgency};
use super::records::{self, Exchange, Found, Purpose, State};
use crate::app::{App, Failure};
use crate::identities::records::{self as identities, Identity, Kind, TRANSOM};

/// Whether `sender` may send to `recipient` now: it is the recipient, or
/// `@transom`, or a member of the group it sends to, or the recipient
/// allowed it.
pub(crate) async fn admits(app: &App, recipient: &Identity, sender: &str) -> Result<bool, Failure> {
    if sender == recipient.id || sender == TRANSOM {
        return Ok(true);
    }
    if recipient.kind == Kind::Group
        && identities::is_member(&app.store, &recipient.id, sender).await?
    {
        return Ok(true);
    }
    let mut connection = app.store.connection().await?;
    Ok(connection
        .sismember(app.store.key(&format!("accepts:{}", recipient.id)), sender)
        .await?)
}

fn choice(label: &str, id: &str) -> Choice {
    Choice {
        label: label.to_owned(),
        description: None,
        id: Some(id.to_owned()),
    }
}

/// Sends `message` from `@transom` to `recipient`, for `purpose`.
pub(crate) async fn from_transom(
    app: &App,
    recipient: &str,
    message: Message,
    purpose: Purpose,
) -> Result<Exchange, Failure> {
    let exchange = Exchange {
        id: records::new_id(),
        sender: TRANSOM.to_owned(),
        recipient: recipient.to_owned(),
        state: State::Open,
        message,
        reply: None,
        created: records::now(),
        expires: None,
        purpose: Some(purpose),
    };
    records::create(&app.store, &exchange).await?;
    super::announce(app, &exchange).await?;
    Ok(exchange)
}

/// Holds `held` for the recipient's consent, and asks for it, unless an
/// earlier message from the same sender is already asking.
pub(crate) async fn ask(app: &App, recipient: &Identity, held: &Exchange) -> Result<(), Failure> {
    let mut connection = app.store.connection().await?;
    let held_key = app
        .store
        .key(&format!("held:{}:{}", recipient.id, held.sender));
    let () = connection.rpush(&held_key, &held.id).await?;
    let asking = app.store.key(&format!("consent:{}", recipient.id));
    let waiting: Option<String> = connection.hget(&asking, &held.sender).await?;
    if waiting.is_some() {
        return Ok(());
    }
    let sender = identities::identity(&app.store, &held.sender).await?.name;
    let first = held
        .message
        .summary
        .as_deref()
        .or(held.message.body.as_deref())
        .unwrap_or_default();
    let message = Message {
        summary: Some(format!("@{sender} wants to send you messages")),
        body: Some(format!(
            "@{sender} has not sent to @{} before. Its first message is held until you decide:\n\n\
             > {first}\n\n[Read the whole message](/exchanges/{})",
            recipient.name, held.id
        )),
        choices: vec![choice("Allow", "allow"), choice("Decline", "decline")],
        urgency: Urgency::Normal,
        ..Message::default()
    };
    let purpose = Purpose::Consent {
        sender: held.sender.clone(),
    };
    let request = from_transom(app, &recipient.id, message, purpose).await?;
    let () = connection.hset(asking, &held.sender, &request.id).await?;
    Ok(())
}

/// Invites `invitee` to join `group`, on behalf of `by`, unless an
/// invitation is already waiting. Returns the invitation's exchange id.
pub(crate) async fn invite(
    app: &App,
    group: &Identity,
    invitee: &Identity,
    by: &str,
) -> Result<String, Failure> {
    let mut connection = app.store.connection().await?;
    let invitations = app.store.key(&format!("invitations:{}", group.id));
    let waiting: Option<String> = connection.hget(&invitations, &invitee.id).await?;
    if let Some(id) = waiting {
        return Ok(id);
    }
    let message = Message {
        summary: Some(format!("@{by} invites you to join @{}", group.name)),
        body: Some(format!(
            "Members of @{} receive everything sent to it, and any of them can reply.",
            group.name
        )),
        choices: vec![choice("Join", "join"), choice("Decline", "decline")],
        ..Message::default()
    };
    let purpose = Purpose::Invitation {
        group: group.id.clone(),
    };
    let invitation = from_transom(app, &invitee.id, message, purpose).await?;
    let () = connection
        .hset(invitations, &invitee.id, &invitation.id)
        .await?;
    Ok(invitation.id)
}

/// Does what a reply to one of `@transom`'s exchanges decided: allows or
/// declines a sender, or joins a group.
pub(crate) async fn settle(app: &App, exchange: &Exchange, choice: &str) -> Result<(), Failure> {
    match &exchange.purpose {
        Some(Purpose::Consent { sender }) => {
            decide(app, &exchange.recipient, sender, choice == "allow").await
        }
        Some(Purpose::Invitation { group }) => {
            let mut connection = app.store.connection().await?;
            let () = connection
                .hdel(
                    app.store.key(&format!("invitations:{group}")),
                    &exchange.recipient,
                )
                .await?;
            if choice == "join" {
                identities::add_member(&app.store, group, &exchange.recipient).await?;
                super::announce_groups(app, &exchange.recipient).await?;
            }
            Ok(())
        }
        None => Ok(()),
    }
}

/// Allows or declines `sender` for `recipient`, releasing or declining
/// every message held for it.
async fn decide(app: &App, recipient: &str, sender: &str, allow: bool) -> Result<(), Failure> {
    let mut connection = app.store.connection().await?;
    if allow {
        let () = connection
            .sadd(app.store.key(&format!("accepts:{recipient}")), sender)
            .await?;
    }
    let () = connection
        .hdel(app.store.key(&format!("consent:{recipient}")), sender)
        .await?;
    let held_key = app.store.key(&format!("held:{recipient}:{sender}"));
    let held: Vec<String> = connection.lrange(&held_key, 0, -1).await?;
    let () = connection.del(&held_key).await?;
    let to = if allow { State::Open } else { State::Declined };
    for id in held {
        let (moved, _) = changes::transition(&app.store, &id, &[State::Pending], to).await?;
        if let (true, Found::Exchange(exchange)) = (moved, records::load(&app.store, &id).await?) {
            super::announce(app, &exchange).await?;
        }
    }
    Ok(())
}
