//! transom's settings, read from an optional YAML file and from `TRANSOM_`
//! environment variables, with the environment taking precedence. A double
//! underscore separates nested keys, so `TRANSOM_ISSUERS__CLUSTER__URL` is
//! the `url` of the issuer named `cluster`.

use std::collections::{BTreeMap, HashMap};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;

pub(crate) use aliases::Aliases;

mod aliases;

#[derive(Debug, Deserialize)]
pub(crate) struct Settings {
    #[serde(default = "defaults::listen")]
    pub(crate) listen: SocketAddr,

    /// Where transom serves `/metrics` for Prometheus. Without it, nothing
    /// serves them.
    pub(crate) metrics_listen: Option<SocketAddr>,

    #[serde(default = "defaults::redis_url")]
    pub(crate) redis_url: String,

    /// Every key transom writes starts with this, so transom can share a
    /// Redis with other programs, and each test can have keys of its own.
    #[serde(default = "defaults::redis_prefix")]
    pub(crate) redis_prefix: String,

    #[serde(default)]
    pub(crate) issuers: BTreeMap<String, IssuerSettings>,

    /// The URL people reach transom at, which browser sign-in sends the
    /// issuer back to. Without it, the request's own host is used.
    pub(crate) public_url: Option<String>,

    /// Where the app may ask for its sign-in code to be sent: the App Links
    /// of each build of the app this server signs in. Someone who builds
    /// the app with their own key claims a domain of their own and names
    /// it here. A YAML list, or in the environment, URLs separated by
    /// spaces.
    #[serde(default = "defaults::app_callbacks", deserialize_with = "urls")]
    pub(crate) app_callbacks: Vec<url::Url>,

    /// The gateway that wakes phones running the official app through
    /// FCM. Someone who builds the app with their own Firebase project runs
    /// the gateway with that project's credentials and names it here.
    #[serde(default = "defaults::wake_gateway")]
    pub(crate) wake_gateway: String,

    /// How long transom keeps an exchange after it ends, or a notification
    /// after it arrives, before deleting it: seconds, or a duration such as
    /// `2d`, as a message's `ttl` takes. An open question is kept until it
    /// ends.
    #[serde(default = "defaults::retention", deserialize_with = "duration")]
    pub(crate) retention: Duration,

    /// A URL transom sends a GET to every `watchdog_interval` while it can
    /// reach Redis, for a dead man's switch such as a healthchecks.io
    /// check. Without it, transom pings nothing.
    #[serde(default, deserialize_with = "optional_url")]
    pub(crate) watchdog_url: Option<url::Url>,

    /// How often transom pings `watchdog_url`: seconds, or a duration such
    /// as `10m`.
    #[serde(default = "defaults::watchdog_interval", deserialize_with = "duration")]
    pub(crate) watchdog_interval: Duration,
}

fn optional_url<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<url::Url>, D::Error> {
    Option::<String>::deserialize(deserializer)?
        .map(|url| url::Url::parse(&url).map_err(serde::de::Error::custom))
        .transpose()
}

/// Seconds, or a duration such as `2d`, read as a message's `ttl` is.
fn duration<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Duration, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Length {
        Seconds(u64),
        Text(String),
    }
    match Length::deserialize(deserializer)? {
        Length::Seconds(seconds) => Ok(seconds),
        Length::Text(text) => {
            crate::exchanges::fields::ttl(&text).map_err(serde::de::Error::custom)
        }
    }
    .map(Duration::from_secs)
}

/// A list of URLs, or one string of them separated by spaces, since an
/// environment variable holds only a string. A URL has no spaces, but may
/// have commas.
fn urls<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Vec<url::Url>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Urls {
        List(Vec<String>),
        Spaced(String),
    }
    let urls = match Urls::deserialize(deserializer)? {
        Urls::List(urls) => urls,
        Urls::Spaced(urls) => urls.split_whitespace().map(str::to_owned).collect(),
    };
    urls.iter()
        .map(|url| url::Url::parse(url).map_err(serde::de::Error::custom))
        .collect()
}

/// One trusted token issuer.
#[derive(Clone, Debug, Deserialize)]
pub(crate) struct IssuerSettings {
    /// The issuer's name in the settings, which the sign-in page's links
    /// use. It is the issuer's key, so it is filled in after reading.
    #[serde(skip)]
    pub(crate) name: String,

    /// The issuer's URL, which must equal the `iss` claim of its tokens.
    pub(crate) url: String,

    /// The audience its tokens must be issued for.
    pub(crate) audience: String,

    /// The claim that names an individual the first time it signs in
    /// without choosing a name.
    pub(crate) name_claim: Option<String>,

    /// A CA certificate to trust when fetching the issuer's keys, for an
    /// issuer like a Kubernetes cluster that signs its own certificates.
    pub(crate) ca_file: Option<PathBuf>,

    /// A token to send when fetching the issuer's keys, for an issuer that
    /// serves them only to callers it knows. It is read on every fetch,
    /// because a Kubernetes projected token is rotated in place.
    pub(crate) token_file: Option<PathBuf>,

    /// The client transom signs people in to a browser as. An issuer with
    /// one is offered on the sign-in page.
    pub(crate) client_id: Option<String>,

    /// The client's secret, for an issuer that requires one.
    pub(crate) client_secret: Option<String>,

    /// The issuer's name on the sign-in page.
    pub(crate) label: Option<String>,

    /// Subjects that sign in as another subject's individual, for an
    /// issuer whose subjects name workloads and not people, such as a
    /// Kubernetes cluster where one agent runs under several accounts.
    #[serde(default)]
    pub(crate) aliases: Aliases,
}

pub(crate) mod defaults {
    use std::net::SocketAddr;

    /// The callbacks of the apps from transom's own releases: the App
    /// Link the Android app claims, and the `sh.transom` scheme the
    /// desktop app's desktop entry claims.
    pub(crate) fn app_callbacks() -> Vec<url::Url> {
        ["https://transom.sh/app/callback", "sh.transom:/callback"]
            .map(|callback| url::Url::parse(callback).expect("the default is a URL"))
            .to_vec()
    }

    /// The gateway the project runs for the app from transom's own
    /// releases.
    pub(crate) fn wake_gateway() -> String {
        "https://wake.transom.sh/".to_owned()
    }

    pub(crate) fn retention() -> std::time::Duration {
        std::time::Duration::from_hours(48)
    }

    pub(crate) fn watchdog_interval() -> std::time::Duration {
        std::time::Duration::from_mins(10)
    }

    pub(super) fn listen() -> SocketAddr {
        SocketAddr::from(([0, 0, 0, 0], 8080))
    }

    pub(super) fn redis_url() -> String {
        "redis://127.0.0.1:6379".to_owned()
    }

    pub(super) fn redis_prefix() -> String {
        "transom".to_owned()
    }
}

impl Settings {
    /// Reads the settings from `file`, if given, and the process's
    /// environment.
    pub(crate) fn load(file: Option<&Path>) -> Result<Self, config::ConfigError> {
        Self::load_from(file, None)
    }

    /// Reads the settings with `environment` standing in for the process's
    /// environment, so tests do not have to change the process's own.
    fn load_from(
        file: Option<&Path>,
        environment: Option<HashMap<String, String>>,
    ) -> Result<Self, config::ConfigError> {
        let mut builder = config::Config::builder();
        if let Some(file) = file {
            builder = builder.add_source(config::File::from(file));
        }
        builder
            .add_source(
                config::Environment::with_prefix("TRANSOM")
                    .prefix_separator("_")
                    .separator("__")
                    .source(environment),
            )
            .build()?
            .try_deserialize()
            .map(|mut settings: Self| {
                for (name, issuer) in &mut settings.issuers {
                    issuer.name.clone_from(name);
                }
                settings
            })
    }
}

#[cfg(test)]
mod tests;
