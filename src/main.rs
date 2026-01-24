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

fn collect_cpu_metrics(
    sys: &System,
    timestamp: i64,
    hostname: &str,
    timeseries: &mut Vec<TimeSeries>,
) {
    // agemon_cpu_usage_percent: Global CPU usage percentage (0-100)
    timeseries.push(create_timeseries(
        "agemon_cpu_usage_percent",
        sys.global_cpu_usage() as f64,
        timestamp,
        hostname,
    ));

    // agemon_cpu_count: Number of logical CPU cores
    timeseries.push(create_timeseries(
        "agemon_cpu_count",
        sys.cpus().len() as f64,
        timestamp,
        hostname,
    ));

    // agemon_cpu_core_usage_percent: Per-core CPU usage percentage
    for cpu in sys.cpus() {
        let labels = vec![("cpu", cpu.name())];
        timeseries.push(create_timeseries_with_labels(
            "agemon_cpu_core_usage_percent",
            cpu.cpu_usage() as f64,
            timestamp,
            hostname,
            labels,
        ));
    }
}

fn collect_memory_metrics(
    sys: &System,
    timestamp: i64,
    hostname: &str,
    timeseries: &mut Vec<TimeSeries>,
) {
    // agemon_memory_total_bytes: Total physical memory in bytes
    timeseries.push(create_timeseries(
        "agemon_memory_total_bytes",
        sys.total_memory() as f64,
        timestamp,
        hostname,
    ));

    // agemon_memory_used_bytes: Used physical memory in bytes
    timeseries.push(create_timeseries(
        "agemon_memory_used_bytes",
        sys.used_memory() as f64,
        timestamp,
        hostname,
    ));

    // agemon_memory_free_bytes: Free physical memory in bytes
    timeseries.push(create_timeseries(
        "agemon_memory_free_bytes",
        sys.free_memory() as f64,
        timestamp,
        hostname,
    ));

    // agemon_memory_available_bytes: Available physical memory in bytes (includes cached/buffered)
    timeseries.push(create_timeseries(
        "agemon_memory_available_bytes",
        sys.available_memory() as f64,
        timestamp,
        hostname,
    ));

    // agemon_memory_usage_ratio: Memory usage ratio (0.0-1.0)
    let usage_ratio = if sys.total_memory() > 0 {
        sys.used_memory() as f64 / sys.total_memory() as f64
    } else {
        0.0
    };
    timeseries.push(create_timeseries(
        "agemon_memory_usage_ratio",
        usage_ratio,
        timestamp,
        hostname,
    ));

    // agemon_swap_total_bytes: Total swap space in bytes
    timeseries.push(create_timeseries(
        "agemon_swap_total_bytes",
        sys.total_swap() as f64,
        timestamp,
        hostname,
    ));

    // agemon_swap_used_bytes: Used swap space in bytes
    timeseries.push(create_timeseries(
        "agemon_swap_used_bytes",
        sys.used_swap() as f64,
        timestamp,
        hostname,
    ));

    // agemon_swap_free_bytes: Free swap space in bytes
    timeseries.push(create_timeseries(
        "agemon_swap_free_bytes",
        sys.free_swap() as f64,
        timestamp,
        hostname,
    ));

    // agemon_swap_usage_ratio: Swap usage ratio (0.0-1.0)
    let swap_ratio = if sys.total_swap() > 0 {
        sys.used_swap() as f64 / sys.total_swap() as f64
    } else {
        0.0
    };
    timeseries.push(create_timeseries(
        "agemon_swap_usage_ratio",
        swap_ratio,
        timestamp,
        hostname,
    ));
}

fn collect_disk_metrics(disks: &Disks, timestamp: i64, hostname: &str, timeseries: &mut Vec<TimeSeries>) {
    for disk in disks.list() {
        let mount_point = disk.mount_point().to_string_lossy().into_owned();
        let device = disk.name().to_string_lossy().into_owned();
        let fs_type = disk.file_system().to_string_lossy().into_owned();

        let labels = vec![
            ("mount_point", mount_point.as_str()),
            ("device", device.as_str()),
            ("fs_type", fs_type.as_str()),
        ];

        // agemon_disk_total_bytes: Total disk space in bytes
        timeseries.push(create_timeseries_with_labels(
            "agemon_disk_total_bytes",
            disk.total_space() as f64,
            timestamp,
            hostname,
            labels.clone(),
        ));

        // agemon_disk_available_bytes: Available disk space in bytes
        timeseries.push(create_timeseries_with_labels(
            "agemon_disk_available_bytes",
            disk.available_space() as f64,
            timestamp,
            hostname,
            labels.clone(),
        ));

        // agemon_disk_used_bytes: Used disk space in bytes
        let used = disk.total_space().saturating_sub(disk.available_space());
        timeseries.push(create_timeseries_with_labels(
            "agemon_disk_used_bytes",
            used as f64,
            timestamp,
            hostname,
            labels.clone(),
        ));

        // agemon_disk_usage_ratio: Disk usage ratio (0.0-1.0)
        let usage_ratio = if disk.total_space() > 0 {
            used as f64 / disk.total_space() as f64
        } else {
            0.0
        };
        timeseries.push(create_timeseries_with_labels(
            "agemon_disk_usage_ratio",
            usage_ratio,
            timestamp,
            hostname,
            labels.clone(),
        ));

        // agemon_disk_is_removable: Whether the disk is removable (1=yes, 0=no)
        timeseries.push(create_timeseries_with_labels(
            "agemon_disk_is_removable",
            if disk.is_removable() { 1.0 } else { 0.0 },
            timestamp,
            hostname,
            labels,
        ));
    }
}

fn collect_network_metrics(
    networks: &Networks,
    timestamp: i64,
    hostname: &str,
    timeseries: &mut Vec<TimeSeries>,
) {
    for (interface_name, data) in networks.list() {
        let labels = vec![("interface", interface_name.as_str())];

        // agemon_network_received_bytes_total: Total bytes received on interface (counter)
        timeseries.push(create_timeseries_with_labels(
            "agemon_network_received_bytes_total",
            data.total_received() as f64,
            timestamp,
            hostname,
            labels.clone(),
        ));

        // agemon_network_transmitted_bytes_total: Total bytes transmitted on interface (counter)
        timeseries.push(create_timeseries_with_labels(
            "agemon_network_transmitted_bytes_total",
            data.total_transmitted() as f64,
            timestamp,
            hostname,
            labels.clone(),
        ));

        // agemon_network_received_packets_total: Total packets received on interface (counter)
        timeseries.push(create_timeseries_with_labels(
            "agemon_network_received_packets_total",
            data.total_packets_received() as f64,
            timestamp,
            hostname,
            labels.clone(),
        ));

        // agemon_network_transmitted_packets_total: Total packets transmitted on interface (counter)
        timeseries.push(create_timeseries_with_labels(
            "agemon_network_transmitted_packets_total",
            data.total_packets_transmitted() as f64,
            timestamp,
            hostname,
            labels.clone(),
        ));

        // agemon_network_received_errors_total: Total receive errors on interface (counter)
        timeseries.push(create_timeseries_with_labels(
            "agemon_network_received_errors_total",
            data.total_errors_on_received() as f64,
            timestamp,
            hostname,
            labels.clone(),
        ));

        // agemon_network_transmitted_errors_total: Total transmit errors on interface (counter)
        timeseries.push(create_timeseries_with_labels(
            "agemon_network_transmitted_errors_total",
            data.total_errors_on_transmitted() as f64,
            timestamp,
            hostname,
            labels,
        ));
    }
}

fn collect_system_metrics(timestamp: i64, hostname: &str, timeseries: &mut Vec<TimeSeries>) {
    // agemon_system_uptime_seconds: System uptime in seconds
    timeseries.push(create_timeseries(
        "agemon_system_uptime_seconds",
        System::uptime() as f64,
        timestamp,
        hostname,
    ));

    // agemon_system_boot_time_seconds: System boot time as Unix timestamp
    timeseries.push(create_timeseries(
        "agemon_system_boot_time_seconds",
        System::boot_time() as f64,
        timestamp,
        hostname,
    ));

    // agemon_load_average_1m: 1-minute load average
    let load_avg = System::load_average();
    timeseries.push(create_timeseries(
        "agemon_load_average_1m",
        load_avg.one,
        timestamp,
        hostname,
    ));

    // agemon_load_average_5m: 5-minute load average
    timeseries.push(create_timeseries(
        "agemon_load_average_5m",
        load_avg.five,
        timestamp,
        hostname,
    ));

    // agemon_load_average_15m: 15-minute load average
    timeseries.push(create_timeseries(
        "agemon_load_average_15m",
        load_avg.fifteen,
        timestamp,
        hostname,
    ));

    // agemon_info: System information (value is always 1, labels contain metadata)
    let os_name = System::name().unwrap_or_else(|| "unknown".to_string());
    let os_version = System::os_version().unwrap_or_else(|| "unknown".to_string());
    let kernel_version = System::kernel_version().unwrap_or_else(|| "unknown".to_string());
    let arch = System::cpu_arch();

    let info_labels = vec![
        ("os_name", os_name.as_str()),
        ("os_version", os_version.as_str()),
        ("kernel_version", kernel_version.as_str()),
        ("arch", arch.as_str()),
    ];
    timeseries.push(create_timeseries_with_labels(
        "agemon_info",
        1.0,
        timestamp,
        hostname,
        info_labels,
    ));
}

fn collect_metrics(
    sys: &mut System,
    disks: &mut Disks,
    networks: &mut Networks,
) -> Vec<TimeSeries> {
    let hostname = hostname::get()
        .map(|h| h.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "unknown".to_string());

    let timestamp: i64 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;

    let mut timeseries = vec![];

    sys.refresh_all();
    disks.refresh(true);
    networks.refresh(true);

    collect_cpu_metrics(sys, timestamp, &hostname, &mut timeseries);
    collect_memory_metrics(sys, timestamp, &hostname, &mut timeseries);
    collect_disk_metrics(disks, timestamp, &hostname, &mut timeseries);
    collect_network_metrics(networks, timestamp, &hostname, &mut timeseries);
    collect_system_metrics(timestamp, &hostname, &mut timeseries);

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
