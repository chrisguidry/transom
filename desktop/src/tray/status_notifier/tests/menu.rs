//! The menu as a host reads and clicks it.

use std::collections::HashMap;

use tokio::sync::mpsc::error::TryRecvError;
use zbus::{OwnedVariant, Variant};

use super::*;

#[tokio::test]
async fn a_signed_out_menu_offers_sign_in_and_quit() {
    let session = session().await;
    let (revision, menu) = session.host.menu().await.unwrap();
    assert_eq!(revision, 1);
    assert_eq!(menu.lines(), ["Sign in…", "---", "Quit"]);
}

#[tokio::test]
async fn a_menu_with_questions_lists_each_with_its_answers() {
    let session = showing(asked()).await;
    let (revision, menu) = session.host.menu().await.unwrap();
    assert_eq!(revision, 2);
    assert_eq!(
        menu.lines(),
        [
            "(Waiting on you)",
            "@alice: Summary of a >",
            "  Yes",
            "  No",
            "  ---",
            "  Open",
            "@alice: Summary of b >",
            "  Now",
            "  ---",
            "  Reply…",
            "  Open",
            "(and 1 more)",
            "---",
            "(Not sent yet)",
            "(Summary of c: Your answer, No, is not sent yet)",
            "---",
            "Open transom",
            "[x] Notifications",
            "---",
            "Quit"
        ]
    );
}

#[tokio::test]
async fn a_menu_says_when_notifications_are_off_and_the_server_is_away() {
    let menu = Menu {
        notifications: false,
        connected: false,
        ..signed_in()
    };
    let session = showing(menu).await;
    let (_, menu) = session.host.menu().await.unwrap();
    assert_eq!(
        menu.lines(),
        [
            "(Nothing waits on you)",
            "---",
            "Open transom",
            "[ ] Notifications",
            "(Not connected to the server)",
            "---",
            "Quit"
        ]
    );
}

/// What the app is told when a person clicks the first item labeled
/// `label`, after the host gave the click's token.
async fn click(session: &mut Session, label: &str) -> Picked {
    let (_, menu) = session.host.menu().await.unwrap();
    let clicked = id(&menu, label);
    session
        .host
        .provide_activation_token("token")
        .await
        .unwrap();
    session.host.click(clicked).await.unwrap();
    session.picked.try_recv().unwrap()
}

fn token() -> String {
    "token".to_owned()
}

#[tokio::test]
async fn a_click_on_a_choice_answers_with_it() {
    let mut session = showing(asked()).await;
    assert_eq!(
        click(&mut session, "No").await,
        Picked::Choice {
            exchange: "a".to_owned(),
            key: "no".to_owned()
        }
    );
}

#[tokio::test]
async fn a_click_on_reply_opens_the_reply_with_the_clicks_token() {
    let mut session = showing(asked()).await;
    assert_eq!(
        click(&mut session, "Reply…").await,
        Picked::Reply {
            exchange: "b".to_owned(),
            token: Some(token())
        }
    );
}

#[tokio::test]
async fn a_click_on_open_opens_the_exchange_with_the_clicks_token() {
    let mut session = showing(asked()).await;
    assert_eq!(
        click(&mut session, "Open").await,
        Picked::Open {
            exchange: "a".to_owned(),
            token: Some(token())
        }
    );
}

#[tokio::test]
async fn a_click_on_open_transom_opens_the_inbox_with_the_clicks_token() {
    let mut session = showing(asked()).await;
    assert_eq!(
        click(&mut session, "Open transom").await,
        Picked::Inbox {
            token: Some(token())
        }
    );
}

#[tokio::test]
async fn a_click_on_sign_in_opens_the_window() {
    let mut session = session().await;
    assert_eq!(
        click(&mut session, "Sign in…").await,
        Picked::Inbox {
            token: Some(token())
        }
    );
}

#[tokio::test]
async fn a_click_on_the_checkmark_turns_notifications_off_and_on() {
    let mut session = showing(signed_in()).await;
    assert_eq!(
        click(&mut session, "Notifications").await,
        Picked::Notifications(false)
    );
    let off = Menu {
        notifications: false,
        ..signed_in()
    };
    session.tray.show(off).await;
    assert_eq!(
        click(&mut session, "Notifications").await,
        Picked::Notifications(true)
    );
}

#[tokio::test]
async fn a_click_on_quit_quits() {
    let mut session = showing(signed_in()).await;
    assert_eq!(click(&mut session, "Quit").await, Picked::Quit);
}

#[tokio::test]
async fn a_token_goes_with_one_click_only() {
    let mut session = showing(signed_in()).await;
    click(&mut session, "Quit").await;
    let (_, menu) = session.host.menu().await.unwrap();
    let open = id(&menu, "Open transom");
    session.host.click(open).await.unwrap();
    assert_eq!(session.picked.try_recv(), Ok(Picked::Inbox { token: None }));
}

#[tokio::test]
async fn the_menu_says_how_a_host_draws_it() {
    let session = session().await;
    let host = &session.host;
    assert_eq!(host.menu_property::<u32>("Version").await.unwrap(), 3);
    let status: String = host.menu_property("Status").await.unwrap();
    assert_eq!(status, "normal");
    let direction: String = host.menu_property("TextDirection").await.unwrap();
    assert_eq!(direction, "ltr");
    let icons: Vec<String> = host.menu_property("IconThemePath").await.unwrap();
    assert_eq!(icons, ["/icons"]);
}

#[tokio::test]
async fn a_layout_goes_only_as_deep_as_the_host_asks() {
    let session = showing(asked()).await;
    let (_, menu) = session.host.layout(0, 1, &[]).await.unwrap();
    assert_eq!(menu.children.len(), 12);
    assert_eq!(menu.find("@alice: Summary of a").unwrap().children, []);
}

#[tokio::test]
async fn a_layout_starts_at_the_item_the_host_names() {
    let session = showing(asked()).await;
    let (_, menu) = session.host.menu().await.unwrap();
    let question = menu.find("@alice: Summary of b").unwrap();
    let (_, under) = session.host.layout(question.id, -1, &[]).await.unwrap();
    assert_eq!(&under, question);
    assert!(session.host.layout(-1, -1, &[]).await.is_err());
}

#[tokio::test]
async fn a_layout_has_only_the_properties_the_host_names() {
    let session = session().await;
    let names = ["type", "children-display"];
    let (_, menu) = session.host.layout(0, -1, &names).await.unwrap();
    assert_eq!(menu.lines(), ["", "---", ""]);
}

/// Some items' properties, as `GetGroupProperties` gives them.
type Group = Vec<(i32, HashMap<String, OwnedVariant>)>;

/// Each item's id and label among `properties`.
fn labels(properties: &Group) -> Vec<(i32, &str)> {
    let labeled = properties.iter().filter_map(|(id, properties)| {
        let label = properties.get("label")?;
        Some((*id, label.downcast_ref().ok()?))
    });
    labeled.collect()
}

#[tokio::test]
async fn a_host_reads_the_properties_of_the_items_it_names() {
    let session = session().await;
    let (_, proxy) = session.host.proxies();
    let (_, menu) = session.host.menu().await.unwrap();
    let (sign_in, quit) = (id(&menu, "Sign in…"), id(&menu, "Quit"));
    let asked = (vec![quit, -1], Vec::<&str>::new());
    let named: Group = proxy.call("GetGroupProperties", &asked).await.unwrap();
    assert_eq!(labels(&named), [(quit, "Quit")]);
    let asked = (Vec::<i32>::new(), vec!["label"]);
    let all: Group = proxy.call("GetGroupProperties", &asked).await.unwrap();
    assert_eq!(
        labels(&all),
        [(0, ""), (sign_in, "Sign in…"), (quit, "Quit")]
    );
}

#[tokio::test]
async fn a_host_reads_one_property_of_one_item() {
    let session = session().await;
    let (_, proxy) = session.host.proxies();
    let (_, menu) = session.host.menu().await.unwrap();
    let quit = id(&menu, "Quit");
    let label: OwnedVariant = proxy.call("GetProperty", &(quit, "label")).await.unwrap();
    assert_eq!(label.downcast_ref::<&str>().unwrap(), "Quit");
    let missing: zbus::Result<OwnedVariant> = proxy.call("GetProperty", &(quit, "icon")).await;
    assert!(missing.is_err());
    let nobody: zbus::Result<OwnedVariant> = proxy.call("GetProperty", &(-1, "label")).await;
    assert!(nobody.is_err());
}

#[tokio::test]
async fn a_menu_that_opens_or_closes_picks_nothing() {
    let mut session = session().await;
    let (_, proxy) = session.host.proxies();
    let (_, menu) = session.host.menu().await.unwrap();
    let quit = id(&menu, "Quit");
    let opened = (0, "opened", Variant::from(0), 0_u32);
    proxy.call::<_, _, ()>("Event", &opened).await.unwrap();
    let hovered = (quit, "hovered", Variant::from(0), 0_u32);
    proxy.call::<_, _, ()>("Event", &hovered).await.unwrap();
    let needs_update: bool = proxy.call("AboutToShow", &(0,)).await.unwrap();
    assert!(!needs_update);
    assert_eq!(session.picked.try_recv(), Err(TryRecvError::Empty));
}

#[tokio::test]
async fn a_click_on_an_item_the_menu_does_not_have_is_refused() {
    let mut session = session().await;
    assert!(session.host.click(-1).await.is_err());
    assert_eq!(session.picked.try_recv(), Err(TryRecvError::Empty));
}

#[tokio::test]
async fn a_group_of_events_picks_each_and_names_the_ids_it_does_not_have() {
    let mut session = session().await;
    let (_, proxy) = session.host.proxies();
    let (_, menu) = session.host.menu().await.unwrap();
    let quit = id(&menu, "Quit");
    let event = |id: i32| (id, "clicked", Variant::from(0), 0_u32);
    let events = (vec![event(quit), event(-1)],);
    let unknown: Vec<i32> = proxy.call("EventGroup", &events).await.unwrap();
    assert_eq!(unknown, [-1]);
    assert_eq!(session.picked.try_recv(), Ok(Picked::Quit));
    let shown: (Vec<i32>, Vec<i32>) = proxy
        .call("AboutToShowGroup", &(vec![0, quit, -1],))
        .await
        .unwrap();
    assert_eq!(shown, (vec![], vec![-1]));
}
