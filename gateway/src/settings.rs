//! The gateway's settings, read from `TRANSOM_GATEWAY_` environment
//! variables. It has few enough that a file of them would add nothing.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct Settings {
    #[serde(default = "defaults::listen")]
    pub(crate) listen: SocketAddr,

    /// The Google service account key the gateway signs in to FCM with.
    /// Without the file, the gateway still runs, and answers every wake-up
    /// with 503.
    #[serde(default = "defaults::credentials")]
    pub(crate) credentials: PathBuf,

    /// Where FCM's HTTP v1 API is. Only tests point it elsewhere, at a
    /// stand-in. The key's own `token_uri` says where to sign in.
    #[serde(default = "defaults::fcm_url")]
    pub(crate) fcm_url: String,
}

mod defaults {
    use std::net::SocketAddr;
    use std::path::PathBuf;

    pub(super) fn listen() -> SocketAddr {
        SocketAddr::from(([0, 0, 0, 0], 8080))
    }

    pub(super) fn credentials() -> PathBuf {
        PathBuf::from("/var/run/secrets/fcm/service-account.json")
    }

    pub(super) fn fcm_url() -> String {
        "https://fcm.googleapis.com".to_owned()
    }
}

impl Settings {
    /// Reads the settings from the process's environment.
    pub(crate) fn load() -> Result<Self, config::ConfigError> {
        Self::load_from(None)
    }

    /// Reads the settings with `environment` standing in for the process's
    /// environment, so tests do not have to change the process's own.
    fn load_from(
        environment: Option<HashMap<String, String>>,
    ) -> Result<Self, config::ConfigError> {
        config::Config::builder()
            .add_source(
                config::Environment::with_prefix("TRANSOM_GATEWAY")
                    .prefix_separator("_")
                    .source(environment),
            )
            .build()?
            .try_deserialize()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::Path;

    use super::Settings;

    fn environment(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect()
    }

    #[test]
    fn defaults_with_nothing_set() {
        let settings = Settings::load_from(Some(environment(&[]))).unwrap();
        assert_eq!(settings.listen.to_string(), "0.0.0.0:8080");
        assert_eq!(
            settings.credentials,
            Path::new("/var/run/secrets/fcm/service-account.json")
        );
        assert_eq!(settings.fcm_url, "https://fcm.googleapis.com");
    }

    #[test]
    fn reads_each_setting_from_the_environment() {
        let settings = Settings::load_from(Some(environment(&[
            ("TRANSOM_GATEWAY_LISTEN", "127.0.0.1:9000"),
            ("TRANSOM_GATEWAY_CREDENTIALS", "/etc/fcm.json"),
            ("TRANSOM_GATEWAY_FCM_URL", "http://127.0.0.1:9001"),
        ])))
        .unwrap();
        assert_eq!(settings.listen.to_string(), "127.0.0.1:9000");
        assert_eq!(settings.credentials, Path::new("/etc/fcm.json"));
        assert_eq!(settings.fcm_url, "http://127.0.0.1:9001");
    }

    #[test]
    fn rejects_an_address_that_is_not_one() {
        let result = Settings::load_from(Some(environment(&[(
            "TRANSOM_GATEWAY_LISTEN",
            "not-an-address",
        )])));
        assert!(result.is_err());
    }
}
