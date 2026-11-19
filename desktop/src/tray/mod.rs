//! The tray icon and its menu, decided from the inbox: the questions
//! that wait on this person, each answerable from its submenu, and the
//! app's own few commands.

pub mod status_notifier;

use std::future::Future;

use crate::exchange::describe;
use crate::inbox::{Entry, Inbox};

/// The longest a question's line in the menu runs before it is cut.
const LINE: usize = 60;

/// The most questions the menu lists before it says how many more wait.
const QUESTIONS: usize = 8;

/// One question in the menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Question {
    pub exchange: String,
    /// The sender and the summary.
    pub label: String,
    /// Each choice's key and label.
    pub choices: Vec<(String, String)>,
    /// Whether the question takes text, which the window's text box
    /// answers.
    pub takes_text: bool,
}

/// What the tray shows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Menu {
    /// The signed-in handle, or `None` while signed out.
    pub me: Option<String>,
    pub waiting: Vec<Question>,
    /// How many more questions wait than the menu lists.
    pub more: usize,
    /// Answers made here that the server does not have yet, in words.
    pub unsent: Vec<String>,
    pub unread: usize,
    pub notifications: bool,
    pub connected: bool,
}

impl Menu {
    /// The menu for `me`'s `entries`, sorted as `inbox`.
    pub fn of(
        me: &str,
        entries: &[Entry],
        inbox: &Inbox,
        notifications: bool,
        connected: bool,
    ) -> Self {
        let waiting: Vec<Question> = inbox.waiting.iter().take(QUESTIONS).map(question).collect();
        let unsent = entries
            .iter()
            .filter_map(|entry| {
                let sending = entry.sending.as_ref()?;
                Some(cut(&format!(
                    "{}: {}",
                    describe::title(&entry.exchange),
                    describe::unsent(&entry.exchange, sending)
                )))
            })
            .collect();
        Self {
            me: Some(me.to_owned()),
            more: inbox.waiting.len() - waiting.len(),
            waiting,
            unsent,
            unread: inbox.unread.len(),
            notifications,
            connected,
        }
    }

    /// Whether the icon asks for attention, which draws its dot:
    /// something waits on this person, or they have something unread.
    pub fn attention(&self) -> bool {
        !self.waiting.is_empty() || self.unread > 0
    }

    /// How many questions wait on this person, listed or not.
    fn questions(&self) -> usize {
        self.waiting.len() + self.more
    }

    /// The count beside the icon: how many questions wait, or nothing
    /// when none does.
    pub fn count(&self) -> String {
        match self.questions() {
            0 => String::new(),
            questions => questions.to_string(),
        }
    }

    /// What the icon says when pointed at.
    pub fn tooltip(&self) -> String {
        let Some(me) = &self.me else {
            return "transom: signed out".to_owned();
        };
        match self.questions() {
            0 => format!("transom: {me}"),
            1 => format!("transom: 1 question waits on {me}"),
            waiting => format!("transom: {waiting} questions wait on {me}"),
        }
    }
}

fn question(entry: &Entry) -> Question {
    let exchange = &entry.exchange;
    Question {
        exchange: exchange.id.clone(),
        label: cut(&format!(
            "{}: {}",
            exchange.sender(),
            describe::title(exchange)
        )),
        choices: exchange
            .message
            .choices
            .iter()
            .map(|choice| (choice.key().to_owned(), choice.label.clone()))
            .collect(),
        takes_text: exchange.message.prompt.is_some() || exchange.message.choices.is_empty(),
    }
}

fn cut(line: &str) -> String {
    if line.chars().count() <= LINE {
        return line.to_owned();
    }
    let mut cut: String = line.chars().take(LINE).collect();
    cut.push('…');
    cut
}

/// What a person picked in the menu, or did to the icon. `token` lets a
/// window take the focus on their behalf.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Picked {
    Choice {
        exchange: String,
        key: String,
    },
    Reply {
        exchange: String,
        token: Option<String>,
    },
    Open {
        exchange: String,
        token: Option<String>,
    },
    Inbox {
        token: Option<String>,
    },
    Notifications(bool),
    Quit,
}

impl Picked {
    /// This pick with `token`, where it opens a window that the token
    /// lets take the focus.
    pub fn with(self, token: Option<String>) -> Self {
        match self {
            Self::Reply { exchange, .. } => Self::Reply { exchange, token },
            Self::Open { exchange, .. } => Self::Open { exchange, token },
            Self::Inbox { .. } => Self::Inbox { token },
            other => other,
        }
    }
}

/// The desktop's tray. The app has one implementation, a
/// `StatusNotifierItem`, and this is where a second would fit.
pub trait Tray: Send + Sync + 'static {
    /// Makes the icon and its menu show `menu`.
    fn show(&self, menu: Menu) -> impl Future<Output = ()> + Send;
}

/// A desktop without a tray shows no icon.
impl<T: Tray> Tray for Option<T> {
    async fn show(&self, menu: Menu) {
        if let Some(tray) = self {
            tray.show(menu).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exchange::testing::{exchange, question as asked};
    use crate::inbox::Sending;

    fn entry(exchange: crate::exchange::Exchange) -> Entry {
        Entry {
            read: exchange.read,
            exchange,
            sending: None,
            lost: None,
        }
    }

    fn menu(entries: &[Entry]) -> Menu {
        let now = "2026-11-19T13:00:00Z".parse().unwrap();
        Menu::of(
            "@bob",
            entries,
            &Inbox::of("@bob", entries, now),
            true,
            true,
        )
    }

    #[test]
    fn a_waiting_question_is_listed_with_its_choices() {
        let menu = menu(&[entry(asked("a", &["Yes", "No"])), entry(exchange("b"))]);
        assert_eq!(menu.waiting.len(), 1);
        assert_eq!(menu.waiting[0].label, "@alice: Summary of a");
        assert_eq!(
            menu.waiting[0].choices,
            [
                ("Yes".to_owned(), "Yes".to_owned()),
                ("No".to_owned(), "No".to_owned())
            ]
        );
        assert!(!menu.waiting[0].takes_text);
        assert_eq!(menu.unread, 1);
        assert!(menu.attention());
        assert_eq!(menu.tooltip(), "transom: 1 question waits on @bob");
        assert_eq!(menu.count(), "1");
    }

    #[test]
    fn more_questions_than_fit_are_counted() {
        let entries: Vec<Entry> = (0..10)
            .map(|number| entry(asked(&format!("q{number}"), &["Yes"])))
            .collect();
        let menu = menu(&entries);
        assert_eq!(menu.waiting.len(), 8);
        assert_eq!(menu.more, 2);
        assert_eq!(menu.tooltip(), "transom: 10 questions wait on @bob");
        assert_eq!(menu.count(), "10");
    }

    #[test]
    fn an_unsent_answer_is_said_and_no_longer_waits() {
        let mut answered = entry(asked("a", &["Yes", "No"]));
        answered.sending = Some(Sending {
            choice: Some("No".to_owned()),
            text: None,
        });
        // A reply reads its exchange, here as on the server.
        answered.read = Some(true);
        let menu = menu(&[answered]);
        assert_eq!(menu.waiting, []);
        assert_eq!(
            menu.unsent,
            ["Summary of a: Your answer, No, is not sent yet"]
        );
        assert!(!menu.attention());
        assert_eq!(menu.tooltip(), "transom: @bob");
        assert_eq!(menu.count(), "");
    }

    #[test]
    fn something_unread_asks_for_attention_and_nothing_does_not() {
        let mut read = exchange("b");
        read.read = Some(true);
        assert!(menu(&[entry(exchange("a"))]).attention());
        assert!(!menu(&[entry(read)]).attention());
        assert!(!Menu::default().attention());
    }

    #[test]
    fn a_signed_out_menu_says_so() {
        assert_eq!(Menu::default().tooltip(), "transom: signed out");
    }

    fn token() -> String {
        "token".to_owned()
    }

    #[test]
    fn a_pick_that_opens_a_window_takes_the_token() {
        let exchange = || "a".to_owned();
        assert_eq!(
            Picked::Reply {
                exchange: exchange(),
                token: None
            }
            .with(Some(token())),
            Picked::Reply {
                exchange: exchange(),
                token: Some(token())
            }
        );
        assert_eq!(
            Picked::Open {
                exchange: exchange(),
                token: None
            }
            .with(Some(token())),
            Picked::Open {
                exchange: exchange(),
                token: Some(token())
            }
        );
        assert_eq!(
            Picked::Inbox { token: None }.with(Some(token())),
            Picked::Inbox {
                token: Some(token())
            }
        );
    }

    #[test]
    fn a_pick_that_opens_no_window_stays_as_it_is() {
        assert_eq!(Picked::Quit.with(Some(token())), Picked::Quit);
    }

    #[test]
    fn a_long_line_is_cut() {
        let mut long = asked("a", &["Yes"]);
        long.message.summary = Some("x".repeat(100));
        assert_eq!(menu(&[entry(long)]).waiting[0].label.chars().count(), 61);
    }
}
