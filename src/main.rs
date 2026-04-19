mod metrics;
mod push;

use std::{
    thread,
    time::{Duration, Instant},
};

use clap::Parser;
use miette::{IntoDiagnostic, Result};
use sysinfo::{Components, Disks, Networks, System};
use tracing::{error, info};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

const USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

#[derive(Parser, Debug)]
#[command(
    version,
    about = "Agent monitoring - push system metrics to Prometheus remote write"
)]
pub struct Args {
    /// Interval between metric collections in seconds
    #[arg(short, long, default_value_t = 15)]
    interval: u64,

    /// Prometheus remote write endpoint URL
    #[arg(short, long, env = "AGEMON_REMOTE_WRITE_URL", default_value_t = String::from("http://localhost:9090/api/v1/write"))]
    pub remote_write_url: String,

    /// Username for Basic authentication (optional)
    #[arg(short, long, env = "AGEMON_REMOTE_WRITE_USERNAME")]
    pub username: Option<String>,

    /// Password for Basic authentication (optional)
    #[arg(short, long, env = "AGEMON_REMOTE_WRITE_PASSWORD")]
    pub password: Option<String>,

    /// Number of top processes to report by CPU and memory (0 to disable)
    #[arg(short = 't', long, env = "AGEMON_TOP_PROCESSES", default_value_t = 10)]
    top_processes: usize,
}

fn collect_and_push(
    client: &reqwest::blocking::Client,
    args: &Args,
    sys: &mut System,
    disks: &mut Disks,
    networks: &mut Networks,
    components: &mut Components,
) -> Result<()> {
    let timeseries = metrics::collect_metrics(sys, disks, networks, components, args.top_processes);
    info!("collected {} metrics", timeseries.len());
    push::push_metrics(client, args, timeseries)?;
    Ok(())
}

fn execute_at_interval<F>(mut task: F, interval_secs: u64) -> Result<()>
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

fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| "agemon=info".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let args = Args::parse();
    let interval = args.interval;

    let mut sys = System::new_all();
    let mut disks = Disks::new_with_refreshed_list();
    let mut networks = Networks::new_with_refreshed_list();
    let mut components = Components::new_with_refreshed_list();

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .into_diagnostic()?;

    info!("starting agemon with interval: {}s", interval);

    execute_at_interval(
        || {
            collect_and_push(
                &client,
                &args,
                &mut sys,
                &mut disks,
                &mut networks,
                &mut components,
            )
        },
        interval,
    )?;

    Ok(())
}
