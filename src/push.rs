use std::str::FromStr;

use base64::{Engine, engine::general_purpose::STANDARD};
use miette::{IntoDiagnostic, Result, miette};
use prometheus_remote_write::{TimeSeries, WriteRequest};
use reqwest::{Url, blocking::Client};
use tracing::debug;

use crate::{Args, USER_AGENT};

pub fn push_metrics(client: &Client, args: &Args, timeseries: Vec<TimeSeries>) -> Result<()> {
    let write_request = WriteRequest { timeseries };

    let mut req = write_request
        .build_http_request(
            &args.remote_write_url.parse::<Url>().into_diagnostic()?,
            USER_AGENT,
        )
        .map_err(|err| miette!("failed to build request: {}", err))?;

    if let (Some(username), Some(password)) = (&args.username, &args.password) {
        let credentials = STANDARD.encode(format!("{}:{}", username, password));
        req.headers_mut().insert(
            "Authorization",
            format!("Basic {}", credentials).parse().unwrap(),
        );
    }

    let (parts, body) = req.into_parts();
    let method = reqwest::Method::from_str(parts.method.as_str()).into_diagnostic()?;
    let mut req_builder = client.request(method, parts.uri.to_string());
    for (name, value) in parts.headers.iter() {
        req_builder = req_builder.header(name.to_string(), value.as_bytes());
    }
    req_builder = req_builder.body(body);

    let response = req_builder.send().into_diagnostic()?;
    debug!("push response status: {}", response.status());

    if response.status() != 204 {
        return Err(miette!("push failed with status: {}", response.status()));
    }

    Ok(())
}
