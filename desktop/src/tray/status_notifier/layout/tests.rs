use super::*;
use crate::tray::status_notifier::testing::{question, signed_in};

fn layout(menu: &Menu) -> Layout {
    Layout::of(menu, &mut Ids::default())
}

/// The items under `item` as lines: each one's label, or `---` for a
/// separator, in parentheses when it cannot be picked, and indented
/// under its submenu.
fn lines(item: &Item) -> Vec<String> {
    let mut lines = Vec::new();
    for child in &item.children {
        lines.push(match child.kind {
            Kind::Separator => "---".to_owned(),
            Kind::Said => format!("({})", child.label),
            Kind::Checkmark(true) => format!("[x] {}", child.label),
            Kind::Checkmark(false) => format!("[ ] {}", child.label),
            Kind::Standard | Kind::Submenu => child.label.clone(),
        });
        lines.extend(lines_under(child));
    }
    lines
}

fn lines_under(item: &Item) -> Vec<String> {
    lines(item)
        .into_iter()
        .map(|line| format!("  {line}"))
        .collect()
}

fn labeled<'a>(layout: &'a Layout, label: &str) -> &'a Item {
    let mut items = layout.items().into_iter();
    items.find(|item| item.label == label).unwrap()
}

#[test]
fn a_signed_out_menu_offers_sign_in_and_quit() {
    let layout = layout(&Menu::default());
    assert_eq!(lines(&layout.root), ["Sign in…", "---", "Quit"]);
    assert_eq!(
        labeled(&layout, "Sign in…").picked,
        Some(Picked::Inbox { token: None })
    );
    assert_eq!(labeled(&layout, "Quit").picked, Some(Picked::Quit));
}

#[test]
fn a_menu_with_nothing_waiting_says_so() {
    assert_eq!(
        lines(&layout(&signed_in()).root),
        [
            "(Nothing waits on you)",
            "---",
            "Open transom",
            "[x] Notifications",
            "---",
            "Quit"
        ]
    );
}

#[test]
fn each_question_is_a_submenu_of_its_answers() {
    let menu = Menu {
        waiting: vec![
            question("a", &["Yes", "No"], false),
            question("b", &["Yes"], true),
            question("c", &[], true),
        ],
        more: 2,
        ..signed_in()
    };
    assert_eq!(
        lines(&layout(&menu).root),
        [
            "(Waiting on you)",
            "@alice: Summary of a",
            "  Yes",
            "  No",
            "  ---",
            "  Open",
            "@alice: Summary of b",
            "  Yes",
            "  ---",
            "  Reply…",
            "  Open",
            "@alice: Summary of c",
            "  Reply…",
            "  Open",
            "(and 2 more)",
            "---",
            "Open transom",
            "[x] Notifications",
            "---",
            "Quit"
        ]
    );
}

#[test]
fn the_rest_of_the_menu_says_what_the_app_holds() {
    let menu = Menu {
        unsent: vec!["Summary of a: Your answer, No, is not sent yet".to_owned()],
        unread: 3,
        notifications: false,
        connected: false,
        ..signed_in()
    };
    assert_eq!(
        lines(&layout(&menu).root),
        [
            "(Nothing waits on you)",
            "---",
            "(Not sent yet)",
            "(Summary of a: Your answer, No, is not sent yet)",
            "---",
            "Open transom · 3 unread",
            "[ ] Notifications",
            "(Not connected to the server)",
            "---",
            "Quit"
        ]
    );
}

#[test]
fn each_item_picks_what_it_says() {
    let menu = Menu {
        waiting: vec![question("a", &["Yes"], true)],
        ..signed_in()
    };
    let layout = layout(&menu);
    let exchange = "a".to_owned();
    let picks = |label: &str| labeled(&layout, label).picked.clone();
    assert_eq!(picks("Waiting on you"), None);
    assert_eq!(picks("@alice: Summary of a"), None);
    assert_eq!(
        picks("Yes"),
        Some(Picked::Choice {
            exchange: exchange.clone(),
            key: "yes".to_owned()
        })
    );
    assert_eq!(
        picks("Reply…"),
        Some(Picked::Reply {
            exchange: exchange.clone(),
            token: None
        })
    );
    assert_eq!(
        picks("Open"),
        Some(Picked::Open {
            exchange,
            token: None
        })
    );
    assert_eq!(picks("Open transom"), Some(Picked::Inbox { token: None }));
    assert_eq!(picks("Notifications"), Some(Picked::Notifications(false)));
    assert_eq!(picks("Quit"), Some(Picked::Quit));
}

#[test]
fn the_checkmark_picks_the_other_setting() {
    let menu = Menu {
        notifications: false,
        ..signed_in()
    };
    assert_eq!(
        labeled(&layout(&menu), "Notifications").picked,
        Some(Picked::Notifications(true))
    );
}

#[test]
fn every_item_has_an_id_of_its_own_and_the_root_has_zero() {
    let menu = Menu {
        waiting: vec![
            question("a", &["Yes", "Yes"], true),
            question("b", &["Yes"], false),
        ],
        unsent: vec!["One".to_owned(), "Two".to_owned()],
        ..signed_in()
    };
    let layout = layout(&menu);
    let mut ids: Vec<i32> = layout.items().iter().map(|item| item.id).collect();
    let items = ids.len();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), items);
    assert_eq!(layout.root.id, 0);
    assert_eq!(layout.find(0), Some(&layout.root));
    assert_eq!(layout.find(-1), None);
}

#[test]
fn an_item_keeps_its_id_when_the_menu_around_it_changes() {
    let mut ids = Ids::default();
    let one = Menu {
        waiting: vec![question("a", &["Yes"], false)],
        ..signed_in()
    };
    let two = Menu {
        waiting: vec![
            question("b", &["Yes"], false),
            question("a", &["Yes"], false),
        ],
        unread: 1,
        ..signed_in()
    };
    let before = Layout::of(&one, &mut ids);
    let after = Layout::of(&two, &mut ids);
    let question = labeled(&before, "@alice: Summary of a");
    assert_eq!(after.find(question.id), Some(question));
    assert_eq!(
        labeled(&after, "Open transom · 1 unread").id,
        labeled(&before, "Open transom").id
    );
    assert_eq!(labeled(&after, "Quit").id, labeled(&before, "Quit").id);
}

#[test]
fn an_id_is_not_given_to_another_item_once_its_own_is_gone() {
    let mut ids = Ids::default();
    let asked = Menu {
        waiting: vec![question("a", &["Yes"], false)],
        ..signed_in()
    };
    let before = Layout::of(&asked, &mut ids);
    Layout::of(&signed_in(), &mut ids);
    let after = Layout::of(&asked, &mut ids);
    let earlier = labeled(&before, "@alice: Summary of a").id;
    assert_eq!(after.find(earlier), None);
}

#[test]
fn an_item_has_the_properties_its_host_draws_it_by() {
    let text = |text: &str| Property::Text(text.to_owned());
    let menu = Menu {
        waiting: vec![question("a", &["Yes"], false)],
        ..signed_in()
    };
    let layout = layout(&menu);
    let properties = |label: &str| labeled(&layout, label).properties();
    assert_eq!(properties("Yes"), [("label", text("Yes"))]);
    assert_eq!(
        properties("Waiting on you"),
        [
            ("label", text("Waiting on you")),
            ("enabled", Property::Flag(false))
        ]
    );
    assert_eq!(
        layout.root.children[2].properties(),
        [("type", text("separator"))]
    );
    assert_eq!(
        properties("Notifications"),
        [
            ("label", text("Notifications")),
            ("toggle-type", text("checkmark")),
            ("toggle-state", Property::Number(1))
        ]
    );
    assert_eq!(
        properties("@alice: Summary of a"),
        [
            ("label", text("@alice: Summary of a")),
            ("children-display", text("submenu"))
        ]
    );
}

#[test]
fn only_the_properties_that_differ_have_changed() {
    let mut ids = Ids::default();
    let before = Layout::of(&signed_in(), &mut ids);
    let now = Menu {
        waiting: vec![question("a", &["Yes"], false)],
        notifications: false,
        ..signed_in()
    };
    let after = Layout::of(&now, &mut ids);
    assert_eq!(
        after.changed(&before),
        [
            (
                labeled(&after, "Waiting on you").id,
                vec![("label", Property::Text("Waiting on you".to_owned()))]
            ),
            (
                labeled(&after, "Notifications").id,
                vec![("toggle-state", Property::Number(0))]
            ),
        ]
    );
    assert_eq!(after.changed(&after), []);
}
