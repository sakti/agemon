mod metrics;
mod push;

use std::{
    fs,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

use clap::Parser;
use miette::{IntoDiagnostic, Result, WrapErr};
use sysinfo::System;
use tracing::{error, info, warn};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

use metrics::SystemSources;
use push::PushClient;

const USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

#[derive(Parser, Debug)]
#[command(
    version,
    about = "Agent monitoring - push system metrics to Prometheus remote write"
)]
struct Args {
    /// Interval between metric collections in seconds (minimum 1)
    #[arg(short, long, default_value_t = 15, value_parser = clap::value_parser!(u64).range(1..))]
    interval: u64,

    /// Prometheus remote write endpoint URL
    #[arg(short, long, env = "AGEMON_REMOTE_WRITE_URL", default_value_t = String::from("http://localhost:9090/api/v1/write"))]
    remote_write_url: String,

    /// Username for Basic authentication (optional)
    #[arg(short, long, env = "AGEMON_REMOTE_WRITE_USERNAME")]
    username: Option<String>,

    /// Password for Basic authentication (optional)
    #[arg(
        short,
        long,
        env = "AGEMON_REMOTE_WRITE_PASSWORD",
        conflicts_with = "password_file"
    )]
    password: Option<String>,

    /// File containing the password for Basic authentication (raw value, trailing newline ignored)
    #[arg(long, env = "AGEMON_REMOTE_WRITE_PASSWORD_FILE")]
    password_file: Option<PathBuf>,

    /// Number of top processes to report by CPU and memory (0 to disable)
    #[arg(short = 't', long, env = "AGEMON_TOP_PROCESSES", default_value_t = 10)]
    top_processes: usize,

    /// Value of the `hostname` label (default: the system hostname, read on each collection)
    #[arg(long, env = "AGEMON_HOSTNAME")]
    hostname: Option<String>,
}

fn execute_at_interval<F>(mut task: F, interval_secs: u64) -> !
where
    F: FnMut() -> Result<()>,
{
    let interval = Duration::from_secs(interval_secs);

    loop {
        let start = Instant::now();

        if let Err(err) = task() {
            error!("task failed: {}", err);
        }

        let elapsed = start.elapsed();
        if elapsed < interval {
            thread::sleep(interval - elapsed);
        }
    }
}

fn read_password_file(path: &Path) -> Result<String> {
    let contents = fs::read_to_string(path)
        .into_diagnostic()
        .wrap_err_with(|| format!("failed to read password file {}", path.display()))?;
    Ok(contents.trim_end_matches(['\n', '\r']).to_string())
}

fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| "agemon=info".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let args = Args::parse();

    let password = match &args.password_file {
        Some(path) => Some(read_password_file(path)?),
        None => args.password.clone(),
    };
    match (&args.username, &password) {
        (Some(_), None) => warn!("username set without a password; pushing without authentication"),
        (None, Some(_)) => warn!("password set without a username; pushing without authentication"),
        _ => {}
    }

    let mut sources = SystemSources::new()
        .into_diagnostic()
        .wrap_err("failed to initialize system metric sources")?;
    let client = PushClient::new(
        &args.remote_write_url,
        args.username.as_deref(),
        password.as_deref(),
    )?;

    info!("starting agemon with interval: {}s", args.interval);

    execute_at_interval(
        || {
            let hostname = args
                .hostname
                .clone()
                .or_else(|| System::host_name().ok())
                .unwrap_or_else(|| "unknown".to_string());
            let timeseries = metrics::collect_metrics(&mut sources, &hostname, args.top_processes);
            info!("collected {} metrics", timeseries.len());
            client.push(timeseries)
        },
        args.interval,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interval_zero_rejected() {
        assert!(Args::try_parse_from(["agemon", "--interval", "0"]).is_err());
        assert!(Args::try_parse_from(["agemon", "--interval", "1"]).is_ok());
    }

    #[test]
    fn test_password_and_password_file_conflict() {
        let res = Args::try_parse_from(["agemon", "-p", "x", "--password-file", "/tmp/x"]);
        assert!(res.is_err());
    }

    #[test]
    fn test_read_password_file_strips_trailing_newline() {
        let path = std::env::temp_dir().join(format!("agemon-pw-test-{}", std::process::id()));
        fs::write(&path, "s3cret \r\n").unwrap();
        let pw = read_password_file(&path).unwrap();
        fs::remove_file(&path).unwrap();
        assert_eq!(pw, "s3cret ");
    }

    #[test]
    fn test_read_password_file_missing_errors() {
        assert!(read_password_file(Path::new("/nonexistent/agemon-pw")).is_err());
    }
}
