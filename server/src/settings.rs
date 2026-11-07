//! transom's settings, read from an optional YAML file and from `TRANSOM_`
//! environment variables, with the environment taking precedence. A double
//! underscore separates nested keys, so `TRANSOM_ISSUERS__CLUSTER__URL` is
//! the `url` of the issuer named `cluster`.

use std::collections::{BTreeMap, HashMap};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct Settings {
    #[serde(default = "defaults::listen")]
    pub(crate) listen: SocketAddr,

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
}

pub(crate) mod defaults {
    use std::net::SocketAddr;

    /// The App Link the app from transom's own releases claims.
    pub(crate) fn app_callbacks() -> Vec<url::Url> {
        vec![url::Url::parse("https://transom.sh/app/callback").expect("the default is a URL")]
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
mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;

    use super::Settings;

    fn environment(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect()
    }

    #[test]
    fn defaults_with_nothing_set() {
        let settings = Settings::load_from(None, Some(environment(&[]))).unwrap();
        assert_eq!(settings.listen.to_string(), "0.0.0.0:8080");
        assert_eq!(settings.redis_url, "redis://127.0.0.1:6379");
        assert_eq!(settings.redis_prefix, "transom");
        assert_eq!(settings.wake_gateway, "https://wake.transom.sh/");
        assert!(settings.issuers.is_empty());
    }

    #[test]
    fn reads_the_wake_gateway_from_the_environment() {
        let settings = Settings::load_from(
            None,
            Some(environment(&[(
                "TRANSOM_WAKE_GATEWAY",
                "https://gateway.example.com/",
            )])),
        )
        .unwrap();
        assert_eq!(settings.wake_gateway, "https://gateway.example.com/");
    }

    #[test]
    fn reads_issuers_from_the_environment() {
        let settings = Settings::load_from(
            None,
            Some(environment(&[
                (
                    "TRANSOM_ISSUERS__AUTHENTIK__URL",
                    "https://auth.example.com/application/o/transom/",
                ),
                ("TRANSOM_ISSUERS__AUTHENTIK__AUDIENCE", "transom"),
                (
                    "TRANSOM_ISSUERS__AUTHENTIK__NAME_CLAIM",
                    "preferred_username",
                ),
            ])),
        )
        .unwrap();
        let authentik = &settings.issuers["authentik"];
        assert_eq!(authentik.name, "authentik");
        assert_eq!(
            authentik.url,
            "https://auth.example.com/application/o/transom/"
        );
        assert_eq!(authentik.audience, "transom");
        assert_eq!(authentik.name_claim.as_deref(), Some("preferred_username"));
    }

    fn yaml_file(contents: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("transom-{}.yaml", ulid::Ulid::generate()));
        std::fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn the_environment_overrides_the_file() {
        let file = yaml_file(
            "redis_prefix: from-file\n\
             issuers:\n  \
               cluster:\n    \
                 url: https://kubernetes.default.svc.cluster.local\n    \
                 audience: transom\n    \
                 ca_file: /var/run/secrets/kubernetes.io/serviceaccount/ca.crt\n",
        );
        let settings = Settings::load_from(
            Some(&file),
            Some(environment(&[("TRANSOM_REDIS_PREFIX", "from-environment")])),
        )
        .unwrap();
        std::fs::remove_file(file).unwrap();
        assert_eq!(settings.redis_prefix, "from-environment");
        assert_eq!(
            settings.issuers["cluster"].ca_file.as_deref(),
            Some(std::path::Path::new(
                "/var/run/secrets/kubernetes.io/serviceaccount/ca.crt"
            ))
        );
    }

    /// The app callbacks the settings give, from `yaml` and then the
    /// environment's `TRANSOM_APP_CALLBACKS`.
    fn app_callbacks(yaml: &str, environment_value: Option<&str>) -> Option<Vec<String>> {
        let file = yaml_file(yaml);
        let pairs: Vec<(&str, &str)> = environment_value
            .map(|value| ("TRANSOM_APP_CALLBACKS", value))
            .into_iter()
            .collect();
        let settings = Settings::load_from(Some(&file), Some(environment(&pairs)));
        std::fs::remove_file(file).unwrap();
        settings.ok().map(|settings| {
            settings
                .app_callbacks
                .iter()
                .map(ToString::to_string)
                .collect()
        })
    }

    #[test]
    fn app_callbacks_default_to_the_released_app() {
        assert_eq!(
            app_callbacks("{}", None),
            Some(vec!["https://transom.sh/app/callback".to_owned()])
        );
    }

    #[test]
    fn app_callbacks_are_a_yaml_list() {
        assert_eq!(
            app_callbacks(
                "app_callbacks:\n  - https://a.example.com/cb\n  - https://b.example.com/cb\n",
                None
            ),
            Some(vec![
                "https://a.example.com/cb".to_owned(),
                "https://b.example.com/cb".to_owned()
            ])
        );
    }

    #[test]
    fn app_callbacks_are_separated_by_spaces_in_the_environment() {
        assert_eq!(
            app_callbacks(
                "{}",
                Some("https://a.example.com/cb?x=1,2 https://b.example.com/cb")
            ),
            Some(vec![
                "https://a.example.com/cb?x=1,2".to_owned(),
                "https://b.example.com/cb".to_owned()
            ])
        );
    }

    #[test]
    fn rejects_an_app_callback_that_is_not_a_url() {
        assert_eq!(app_callbacks("{}", Some("transom.sh/app/callback")), None);
    }

    /// The retention the settings give, in seconds, from `yaml` and then the
    /// environment's `TRANSOM_RETENTION`.
    fn retention(yaml: &str, environment_value: Option<&str>) -> Option<u64> {
        let file = yaml_file(yaml);
        let pairs: Vec<(&str, &str)> = environment_value
            .map(|value| ("TRANSOM_RETENTION", value))
            .into_iter()
            .collect();
        let settings = Settings::load_from(Some(&file), Some(environment(&pairs)));
        std::fs::remove_file(file).unwrap();
        settings.ok().map(|settings| settings.retention.as_secs())
    }

    #[test]
    fn retention_defaults_to_two_days() {
        assert_eq!(retention("{}", None), Some(2 * 24 * 60 * 60));
    }

    #[test]
    fn retention_is_a_duration_or_seconds() {
        assert_eq!(
            [
                retention("{}", Some("2d")),
                retention("{}", Some("36h")),
                retention("{}", Some("600")),
                retention("retention: 1w\n", None),
                retention("retention: 90\n", None),
            ],
            [
                Some(2 * 24 * 60 * 60),
                Some(36 * 60 * 60),
                Some(600),
                Some(7 * 24 * 60 * 60),
                Some(90),
            ]
        );
    }

    #[test]
    fn rejects_a_retention_that_is_not_a_duration() {
        assert_eq!(
            [retention("{}", Some("soon")), retention("{}", Some("1mo"))],
            [None, None]
        );
    }

    /// The watchdog's URL and interval, in seconds, from the environment.
    fn watchdog(pairs: &[(&str, &str)]) -> Option<(Option<String>, u64)> {
        Settings::load_from(None, Some(environment(pairs)))
            .ok()
            .map(|settings| {
                (
                    settings.watchdog_url.map(String::from),
                    settings.watchdog_interval.as_secs(),
                )
            })
    }

    #[test]
    fn the_watchdog_is_off_until_given_a_url() {
        assert_eq!(watchdog(&[]), Some((None, 600)));
    }

    #[test]
    fn the_watchdog_takes_a_url_and_an_interval() {
        assert_eq!(
            watchdog(&[
                ("TRANSOM_WATCHDOG_URL", "https://hc-ping.com/abc"),
                ("TRANSOM_WATCHDOG_INTERVAL", "5m"),
            ]),
            Some((Some("https://hc-ping.com/abc".to_owned()), 300))
        );
    }

    #[test]
    fn rejects_a_watchdog_url_that_is_not_a_url() {
        assert_eq!(
            watchdog(&[("TRANSOM_WATCHDOG_URL", "hc-ping.com/abc")]),
            None
        );
    }

    #[test]
    fn rejects_an_issuer_without_an_audience() {
        let result = Settings::load_from(
            None,
            Some(environment(&[(
                "TRANSOM_ISSUERS__CLUSTER__URL",
                "https://example.com",
            )])),
        );
        assert!(result.is_err());
    }
}
