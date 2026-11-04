//! The test issuer, with the settings that make transom trust it. The
//! issuer is its own crate, which cannot name transom's settings, so this
//! wrapper adds them and passes everything else through.

use std::ops::Deref;

pub(crate) use test_issuer::{AUDIENCE, BROWSER_CLIENT, Refreshing};

use crate::settings::IssuerSettings;

pub(crate) struct TestIssuer(test_issuer::TestIssuer);

impl Deref for TestIssuer {
    type Target = test_issuer::TestIssuer;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl TestIssuer {
    pub(crate) async fn start() -> Self {
        Self(test_issuer::TestIssuer::start().await)
    }

    pub(crate) fn settings(&self) -> IssuerSettings {
        IssuerSettings {
            name: "test".to_owned(),
            url: self.url.clone(),
            audience: AUDIENCE.to_owned(),
            name_claim: Some("preferred_username".to_owned()),
            ca_file: None,
            token_file: None,
            client_id: None,
            client_secret: None,
            label: None,
        }
    }

    /// Settings for signing people in to a browser as `transom-browser`.
    pub(crate) fn browser_settings(&self, secret: Option<&str>) -> IssuerSettings {
        IssuerSettings {
            client_id: Some(BROWSER_CLIENT.to_owned()),
            client_secret: secret.map(str::to_owned),
            label: Some("Test".to_owned()),
            ..self.settings()
        }
    }
}
