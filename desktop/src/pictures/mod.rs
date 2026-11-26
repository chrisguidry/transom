//! Identities' pictures and messages' images, fetched from the session's
//! own server and kept as files under the cache. A picture's URL carries
//! its version, so a new picture has a new URL and a kept file is never
//! out of date.

pub mod widgets;

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use aws_lc_rs::digest::{SHA256, digest};

use crate::exchange::Disposition;
use crate::sync::client::Client;

/// The cache of one session's pictures.
#[derive(Clone, Debug)]
pub struct Pictures {
    directory: PathBuf,
    client: Client,
}

impl Pictures {
    pub fn new(cache: &Path, client: Client) -> Self {
        Self {
            directory: cache.join("pictures"),
            client,
        }
    }

    fn kept(&self, what: &str) -> PathBuf {
        let mut name = String::new();
        for byte in digest(&SHA256, what.as_bytes()).as_ref().iter().take(16) {
            let _ = write!(name, "{byte:02x}");
        }
        self.directory.join(name)
    }

    async fn keep(&self, path: PathBuf, bytes: Vec<u8>) -> Option<PathBuf> {
        tokio::fs::create_dir_all(&self.directory).await.ok()?;
        tokio::fs::write(&path, bytes).await.ok()?;
        Some(path)
    }

    /// A file holding the picture at `url`, fetched if it is not kept
    /// yet, or `None` when it cannot be had. A fetch the server refuses
    /// because the session ended is still reported, by the client.
    pub async fn picture(&self, url: &str) -> Option<PathBuf> {
        let path = self.kept(url);
        if tokio::fs::try_exists(&path).await.unwrap_or(false) {
            return Some(path);
        }
        let bytes = self.client.picture(url).await.ok()?;
        self.keep(path, bytes).await
    }

    /// A file holding `name`, an image attached to `exchange`.
    pub async fn attachment(&self, exchange: &str, name: &str) -> Option<PathBuf> {
        let path = self.kept(&format!("attachment:{exchange}:{name}"));
        if tokio::fs::try_exists(&path).await.unwrap_or(false) {
            return Some(path);
        }
        let bytes = self.client.attachment(exchange, name).await.ok()?;
        self.keep(path, bytes).await
    }

    /// Deletes every kept picture, as signing out does.
    pub fn clear(cache: &Path) {
        let _ = std::fs::remove_dir_all(cache.join("pictures"));
    }
}

/// The initials a party without a picture is drawn with, as the web
/// draws them: the first letters of its first two words, or `?`.
pub fn initials(name: &str) -> String {
    let initials: String = name
        .trim_start_matches('@')
        .split(|character: char| character.is_whitespace() || character == '-' || character == '.')
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

/// The badge's class and glyph, or `None` for a message that is only
/// informational, which carries no badge.
pub fn badge(disposition: Disposition) -> Option<(&'static str, &'static str)> {
    match disposition {
        Disposition::Informational => None,
        Disposition::Problem => Some(("problem", "!")),
        Disposition::Resolved => Some(("resolved", "✓")),
        Disposition::Question => Some(("question", "?")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Places;
    use crate::sync::Heard;
    use crate::testserver::{PICTURE, Pair};

    /// A cache of `@bob`'s pictures on `pair`'s server, in a directory
    /// of its own.
    fn pictures(pair: &Pair) -> Pictures {
        Pictures::new(&Places::fresh().cache, pair.client())
    }

    #[tokio::test]
    async fn a_picture_is_fetched_into_a_file() {
        let pair = Pair::start().await;
        let url = pair.pictured("Backup finished").await;
        let kept = pictures(&pair).picture(&url).await.unwrap();
        assert_eq!(std::fs::read(kept).unwrap(), PICTURE);
    }

    #[tokio::test]
    async fn a_kept_picture_is_not_fetched_again() {
        let mut pair = Pair::start().await;
        let url = pair.pictured("Backup finished").await;
        let pictures = pictures(&pair);
        let kept = pictures.picture(&url).await;
        pair.server.stop();
        assert_eq!(pictures.picture(&url).await, kept);
    }

    #[tokio::test]
    async fn a_picture_the_server_does_not_have_is_none() {
        let pair = Pair::start().await;
        assert_eq!(pictures(&pair).picture("/@bob/picture").await, None);
    }

    #[tokio::test]
    async fn an_attached_image_is_fetched_into_a_file_and_kept() {
        let mut pair = Pair::start().await;
        let id = pair.attach("graph.png").await;
        let pictures = pictures(&pair);
        let kept = pictures.attachment(&id, "graph.png").await.unwrap();
        assert_eq!(std::fs::read(&kept).unwrap(), PICTURE);
        pair.server.stop();
        assert_eq!(pictures.attachment(&id, "graph.png").await, Some(kept));
    }

    #[tokio::test]
    async fn an_attachment_the_message_does_not_have_is_none() {
        let pair = Pair::start().await;
        let id = pair.tell("Backup finished").await;
        assert_eq!(pictures(&pair).attachment(&id, "graph.png").await, None);
    }

    #[tokio::test]
    async fn a_fetch_the_ended_session_refuses_reports_it() {
        let pair = Pair::start().await;
        let (url, id) = (
            pair.pictured("Backup finished").await,
            pair.attach("graph.png").await,
        );
        let (heard, mut hearing) = tokio::sync::mpsc::unbounded_channel();
        let client = Client::new(&pair.server.url, "not-a-token").reporting(3, heard);
        let pictures = Pictures::new(&Places::fresh().cache, client);
        assert_eq!(pictures.picture(&url).await, None);
        assert_eq!(pictures.attachment(&id, "graph.png").await, None);
        let reported = [hearing.try_recv().unwrap(), hearing.try_recv().unwrap()];
        let ended = |heard: &Heard| matches!(heard, Heard::SignedOut { generation: 3, .. });
        assert!(reported.iter().all(ended));
    }

    #[tokio::test]
    async fn clearing_deletes_every_kept_picture() {
        let pair = Pair::start().await;
        let url = pair.pictured("Backup finished").await;
        let cache = Places::fresh().cache;
        let pictures = Pictures::new(&cache, pair.client());
        let kept = pictures.picture(&url).await.unwrap();
        Pictures::clear(&cache);
        assert!(!kept.exists());
    }

    #[test]
    fn initials_are_two_letters() {
        let cases = [
            ("@alertmanager", "A"),
            ("Chris Guidry", "CG"),
            ("review-bot", "RB"),
            ("guid.foo", "GF"),
            ("", "?"),
        ];
        for (name, letters) in cases {
            assert_eq!(initials(name), letters);
        }
    }

    #[test]
    fn every_disposition_but_informational_has_a_badge() {
        let cases = [
            (Disposition::Informational, None),
            (Disposition::Problem, Some(("problem", "!"))),
            (Disposition::Resolved, Some(("resolved", "✓"))),
            (Disposition::Question, Some(("question", "?"))),
        ];
        for (disposition, drawn) in cases {
            assert_eq!(badge(disposition), drawn);
        }
    }
}
