//! transom's settings, read from an optional YAML file and from `TRANSOM_`
//! environment variables, with the environment taking precedence. A double
//! underscore separates nested keys, so `TRANSOM_ISSUERS__CLUSTER__URL` is
//! the `url` of the issuer named `cluster`.

use std::collections::{BTreeMap, HashMap};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

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

mod defaults {
    use std::net::SocketAddr;

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
        assert!(settings.issuers.is_empty());
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
