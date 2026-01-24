use std::{
    str::FromStr,
    thread,
    time::{Duration, Instant},
};

use base64::{Engine, engine::general_purpose::STANDARD};
use clap::Parser;
use miette::{IntoDiagnostic, Result, miette};
use prometheus_remote_write::{LABEL_NAME, Label, Sample, TimeSeries, WriteRequest};
use reqwest::blocking::Client;
use sysinfo::{Disks, Networks, System};
use tracing::{debug, error, info};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

const USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

#[derive(Parser, Debug)]
#[command(
    version,
    about = "Agent monitoring - push system metrics to Prometheus remote write"
)]
struct Args {
    /// Interval between metric collections in seconds
    #[arg(short, long, default_value_t = 15)]
    interval: u64,

    /// Prometheus remote write endpoint URL
    #[arg(short, long, env = "AGEMON_REMOTE_WRITE_URL", default_value_t = String::from("http://localhost:9090/api/v1/write"))]
    remote_write_url: String,

    /// Username for Basic authentication (optional)
    #[arg(short, long, env = "AGEMON_REMOTE_WRITE_USERNAME")]
    username: Option<String>,

    /// Password for Basic authentication (optional)
    #[arg(short, long, env = "AGEMON_REMOTE_WRITE_PASSWORD")]
    password: Option<String>,
}

fn create_timeseries(metric_name: &str, value: f64, timestamp: i64, hostname: &str) -> TimeSeries {
    TimeSeries {
        labels: vec![
            Label {
                name: "hostname".to_string(),
                value: hostname.to_string(),
            },
            Label {
                name: LABEL_NAME.to_string(),
                value: metric_name.to_string(),
            },
        ],
        samples: vec![Sample { value, timestamp }],
    }
}

fn create_timeseries_with_labels(
    metric_name: &str,
    value: f64,
    timestamp: i64,
    hostname: &str,
    extra_labels: Vec<(&str, &str)>,
) -> TimeSeries {
    let mut labels = vec![
        Label {
            name: "hostname".to_string(),
            value: hostname.to_string(),
        },
        Label {
            name: LABEL_NAME.to_string(),
            value: metric_name.to_string(),
        },
    ];
    for (k, v) in extra_labels {
        labels.push(Label {
            name: k.to_string(),
            value: v.to_string(),
        });
    }
    TimeSeries {
        labels,
        samples: vec![Sample { value, timestamp }],
    }
}

fn collect_metrics(
    sys: &mut System,
    disks: &mut Disks,
    networks: &mut Networks,
) -> Vec<TimeSeries> {
    let hostname = hostname::get()
        .map(|h| h.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "unknown".to_string());

    let time: i64 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;

    let mut timeseries = vec![];

    sys.refresh_all();
    disks.refresh(true);
    networks.refresh(true);

    timeseries.push(create_timeseries(
        "agemon_cpu_usage_percent",
        sys.global_cpu_usage() as f64,
        time,
        &hostname,
    ));

    timeseries.push(create_timeseries(
        "agemon_memory_total_bytes",
        sys.total_memory() as f64,
        time,
        &hostname,
    ));
    timeseries.push(create_timeseries(
        "agemon_memory_used_bytes",
        sys.used_memory() as f64,
        time,
        &hostname,
    ));
    timeseries.push(create_timeseries(
        "agemon_swap_total_bytes",
        sys.total_swap() as f64,
        time,
        &hostname,
    ));
    timeseries.push(create_timeseries(
        "agemon_swap_used_bytes",
        sys.used_swap() as f64,
        time,
        &hostname,
    ));

    for disk in disks.list() {
        let mount_point = disk.mount_point().to_string_lossy().into_owned();
        let labels = vec![("mount_point", mount_point.as_str())];

        timeseries.push(create_timeseries_with_labels(
            "agemon_disk_total_bytes",
            disk.total_space() as f64,
            time,
            &hostname,
            labels.clone(),
        ));
        timeseries.push(create_timeseries_with_labels(
            "agemon_disk_available_bytes",
            disk.available_space() as f64,
            time,
            &hostname,
            labels,
        ));
    }

    for (interface_name, data) in networks.list() {
        let labels = vec![("interface", interface_name.as_str())];

        timeseries.push(create_timeseries_with_labels(
            "agemon_network_received_bytes",
            data.total_received() as f64,
            time,
            &hostname,
            labels.clone(),
        ));
        timeseries.push(create_timeseries_with_labels(
            "agemon_network_transmitted_bytes",
            data.total_transmitted() as f64,
            time,
            &hostname,
            labels,
        ));
    }

    timeseries
}

fn push_metrics(args: &Args, timeseries: Vec<TimeSeries>) -> Result<()> {
    let write_request = WriteRequest { timeseries };

    let mut req = write_request
        .build_http_request(
            &args
                .remote_write_url
                .parse::<url::Url>()
                .into_diagnostic()?,
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

    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .into_diagnostic()?;

    let (parts, body) = req.into_parts();
    let method = reqwest::Method::from_str(parts.method.as_str()).into_diagnostic()?;
    let mut req_builder = client.request(method, parts.uri.to_string());
    for (name, value) in parts.headers.iter() {
        req_builder = req_builder.header(name.to_string(), value.as_bytes());
    }
    req_builder = req_builder.body(body);

    let response = req_builder.send().into_diagnostic()?;
    debug!("push response status: {}", response.status());

    Ok(())
}

fn collect_and_push(
    args: &Args,
    sys: &mut System,
    disks: &mut Disks,
    networks: &mut Networks,
) -> Result<()> {
    let timeseries = collect_metrics(sys, disks, networks);
    info!("collected {} metrics", timeseries.len());
    push_metrics(args, timeseries)?;
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

    info!("starting agemon with interval: {}s", interval);

    execute_at_interval(
        || collect_and_push(&args, &mut sys, &mut disks, &mut networks),
        interval,
    )?;

    Ok(())
}
