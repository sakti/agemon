use std::time::Duration;

use base64::{Engine, engine::general_purpose::STANDARD};
use miette::{IntoDiagnostic, Result, miette};
use prometheus_remote_write::{
    CONTENT_TYPE, HEADER_NAME_REMOTE_WRITE_VERSION, REMOTE_WRITE_VERSION_01, TimeSeries,
    WriteRequest,
};
use reqwest::{
    Url,
    blocking::Client,
    header::{self, HeaderValue},
};
use tracing::debug;

use crate::USER_AGENT;

pub struct PushClient {
    client: Client,
    url: Url,
    auth_header: Option<HeaderValue>,
}

impl PushClient {
    pub fn new(url: &str, username: Option<&str>, password: Option<&str>) -> Result<Self> {
        let url = url.parse::<Url>().into_diagnostic()?;

        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .into_diagnostic()?;

        let auth_header = match (username, password) {
            (Some(username), Some(password)) => {
                let credentials = STANDARD.encode(format!("{username}:{password}"));
                let mut value: HeaderValue =
                    format!("Basic {credentials}").parse().into_diagnostic()?;
                value.set_sensitive(true);
                Some(value)
            }
            _ => None,
        };

        Ok(Self {
            client,
            url,
            auth_header,
        })
    }

    pub fn push(&self, timeseries: Vec<TimeSeries>) -> Result<()> {
        let body = WriteRequest { timeseries }
            .encode_compressed()
            .map_err(|err| miette!("failed to encode write request: {}", err))?;

        let mut req = self
            .client
            .post(self.url.clone())
            .header(header::CONTENT_TYPE, CONTENT_TYPE)
            .header(header::CONTENT_ENCODING, "snappy")
            .header(HEADER_NAME_REMOTE_WRITE_VERSION, REMOTE_WRITE_VERSION_01)
            .header(header::USER_AGENT, USER_AGENT)
            .body(body);

        if let Some(auth) = &self.auth_header {
            req = req.header(header::AUTHORIZATION, auth.clone());
        }

        let response = req.send().into_diagnostic()?;
        debug!("push response status: {}", response.status());

        if !response.status().is_success() {
            return Err(miette!("push failed with status: {}", response.status()));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_rejects_invalid_url() {
        assert!(PushClient::new("not a url", None, None).is_err());
    }

    #[test]
    fn test_new_builds_basic_auth_header() {
        let client = PushClient::new(
            "http://localhost:9090/api/v1/write",
            Some("user"),
            Some("pass"),
        )
        .unwrap();
        let auth = client.auth_header.expect("auth header should be set");
        assert_eq!(auth.to_str().unwrap(), "Basic dXNlcjpwYXNz");
        assert!(auth.is_sensitive());
    }

    #[test]
    fn test_new_skips_auth_without_password() {
        let client =
            PushClient::new("http://localhost:9090/api/v1/write", Some("user"), None).unwrap();
        assert!(client.auth_header.is_none());
    }
}
