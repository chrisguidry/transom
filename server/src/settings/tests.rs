//! Reading the settings from a file and the environment.

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
    assert_eq!(settings.metrics_listen, None);
}

#[test]
fn reads_the_metrics_address_from_the_environment() {
    let settings = Settings::load_from(
        None,
        Some(environment(&[("TRANSOM_METRICS_LISTEN", "0.0.0.0:9090")])),
    )
    .unwrap();
    assert_eq!(settings.metrics_listen.unwrap().to_string(), "0.0.0.0:9090");
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
fn app_callbacks_default_to_the_released_apps() {
    assert_eq!(
        app_callbacks("{}", None),
        Some(vec![
            "https://transom.sh/app/callback".to_owned(),
            "sh.transom:/callback".to_owned()
        ])
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
