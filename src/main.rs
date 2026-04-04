use std::{
    collections::HashMap,
    str::FromStr,
    thread,
    time::{Duration, Instant},
};

use base64::{Engine, engine::general_purpose::STANDARD};
use clap::Parser;
use miette::{IntoDiagnostic, Result, miette};
use prometheus_remote_write::{LABEL_NAME, Label, Sample, TimeSeries, WriteRequest};
use reqwest::{Url, blocking::Client};
use sysinfo::{Components, Disks, MemoryRefreshKind, Networks, ProcessRefreshKind, System};
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

    /// Number of top processes to report by CPU and memory (0 to disable)
    #[arg(short = 't', long, env = "AGEMON_TOP_PROCESSES", default_value_t = 10)]
    top_processes: usize,
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

fn collect_disk_metrics(
    disks: &Disks,
    timestamp: i64,
    hostname: &str,
    timeseries: &mut Vec<TimeSeries>,
) {
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

fn collect_disk_io_metrics(
    sys: &System,
    timestamp: i64,
    hostname: &str,
    timeseries: &mut Vec<TimeSeries>,
) {
    let mut total_read_bytes: u64 = 0;
    let mut total_written_bytes: u64 = 0;
    let mut read_bytes_per_sec: u64 = 0;
    let mut written_bytes_per_sec: u64 = 0;

    for process in sys.processes().values() {
        let disk_usage = process.disk_usage();
        total_read_bytes += disk_usage.total_read_bytes;
        total_written_bytes += disk_usage.total_written_bytes;
        read_bytes_per_sec += disk_usage.read_bytes;
        written_bytes_per_sec += disk_usage.written_bytes;
    }

    // agemon_disk_io_read_bytes_total: Total bytes read from disk (counter)
    timeseries.push(create_timeseries(
        "agemon_disk_io_read_bytes_total",
        total_read_bytes as f64,
        timestamp,
        hostname,
    ));

    // agemon_disk_io_written_bytes_total: Total bytes written to disk (counter)
    timeseries.push(create_timeseries(
        "agemon_disk_io_written_bytes_total",
        total_written_bytes as f64,
        timestamp,
        hostname,
    ));

    // agemon_disk_io_read_bytes_per_sec: Bytes read per second since last refresh
    timeseries.push(create_timeseries(
        "agemon_disk_io_read_bytes_per_sec",
        read_bytes_per_sec as f64,
        timestamp,
        hostname,
    ));

    // agemon_disk_io_written_bytes_per_sec: Bytes written per second since last refresh
    timeseries.push(create_timeseries(
        "agemon_disk_io_written_bytes_per_sec",
        written_bytes_per_sec as f64,
        timestamp,
        hostname,
    ));
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

#[cfg(target_os = "linux")]
fn collect_procfs_metrics(timestamp: i64, hostname: &str, timeseries: &mut Vec<TimeSeries>) {
    use procfs::{Current, CurrentSI};

    // TCP connection counts by state
    if let Ok(tcp_entries) = procfs::net::tcp() {
        let mut established: u64 = 0;
        let mut listen: u64 = 0;
        let mut time_wait: u64 = 0;
        let mut close_wait: u64 = 0;
        let mut other: u64 = 0;

        for entry in &tcp_entries {
            match entry.state {
                procfs::net::TcpState::Established => established += 1,
                procfs::net::TcpState::Listen => listen += 1,
                procfs::net::TcpState::TimeWait => time_wait += 1,
                procfs::net::TcpState::CloseWait => close_wait += 1,
                _ => other += 1,
            }
        }

        for (state, count) in [
            ("established", established),
            ("listen", listen),
            ("time_wait", time_wait),
            ("close_wait", close_wait),
            ("other", other),
        ] {
            timeseries.push(create_timeseries_with_labels(
                "agemon_tcp_connections",
                count as f64,
                timestamp,
                hostname,
                vec![("state", state)],
            ));
        }
    }

    // TCP6 connection counts by state
    if let Ok(tcp6_entries) = procfs::net::tcp6() {
        let mut established: u64 = 0;
        let mut listen: u64 = 0;
        let mut time_wait: u64 = 0;
        let mut close_wait: u64 = 0;
        let mut other: u64 = 0;

        for entry in &tcp6_entries {
            match entry.state {
                procfs::net::TcpState::Established => established += 1,
                procfs::net::TcpState::Listen => listen += 1,
                procfs::net::TcpState::TimeWait => time_wait += 1,
                procfs::net::TcpState::CloseWait => close_wait += 1,
                _ => other += 1,
            }
        }

        for (state, count) in [
            ("established", established),
            ("listen", listen),
            ("time_wait", time_wait),
            ("close_wait", close_wait),
            ("other", other),
        ] {
            timeseries.push(create_timeseries_with_labels(
                "agemon_tcp6_connections",
                count as f64,
                timestamp,
                hostname,
                vec![("state", state)],
            ));
        }
    }

    // System-wide file descriptor usage
    if let Ok(file_state) = procfs::sys::fs::file_nr() {
        timeseries.push(create_timeseries(
            "agemon_file_descriptors_allocated",
            file_state.allocated as f64,
            timestamp,
            hostname,
        ));
        timeseries.push(create_timeseries(
            "agemon_file_descriptors_max",
            file_state.max as f64,
            timestamp,
            hostname,
        ));
    }

    // Context switches and process forks from /proc/stat
    if let Ok(kernel_stats) = procfs::KernelStats::current() {
        timeseries.push(create_timeseries(
            "agemon_context_switches_total",
            kernel_stats.ctxt as f64,
            timestamp,
            hostname,
        ));
        timeseries.push(create_timeseries(
            "agemon_processes_forked_total",
            kernel_stats.processes as f64,
            timestamp,
            hostname,
        ));
        timeseries.push(create_timeseries(
            "agemon_procs_running",
            kernel_stats.procs_running.unwrap_or(0) as f64,
            timestamp,
            hostname,
        ));
        timeseries.push(create_timeseries(
            "agemon_procs_blocked",
            kernel_stats.procs_blocked.unwrap_or(0) as f64,
            timestamp,
            hostname,
        ));
    }

    // PSI (Pressure Stall Information) - cpu, memory, io
    if let Ok(psi) = procfs::CpuPressure::current() {
        timeseries.push(create_timeseries(
            "agemon_psi_cpu_some_avg10",
            psi.some.avg10.into(),
            timestamp,
            hostname,
        ));
        timeseries.push(create_timeseries(
            "agemon_psi_cpu_some_avg60",
            psi.some.avg60.into(),
            timestamp,
            hostname,
        ));
        timeseries.push(create_timeseries(
            "agemon_psi_cpu_some_avg300",
            psi.some.avg300.into(),
            timestamp,
            hostname,
        ));
        timeseries.push(create_timeseries(
            "agemon_psi_cpu_some_total_us",
            psi.some.total as f64,
            timestamp,
            hostname,
        ));
    }

    if let Ok(psi) = procfs::MemoryPressure::current() {
        for (prefix, record) in [("some", &psi.some), ("full", &psi.full)] {
            timeseries.push(create_timeseries(
                &format!("agemon_psi_memory_{prefix}_avg10"),
                record.avg10.into(),
                timestamp,
                hostname,
            ));
            timeseries.push(create_timeseries(
                &format!("agemon_psi_memory_{prefix}_avg60"),
                record.avg60.into(),
                timestamp,
                hostname,
            ));
            timeseries.push(create_timeseries(
                &format!("agemon_psi_memory_{prefix}_avg300"),
                record.avg300.into(),
                timestamp,
                hostname,
            ));
            timeseries.push(create_timeseries(
                &format!("agemon_psi_memory_{prefix}_total_us"),
                record.total as f64,
                timestamp,
                hostname,
            ));
        }
    }

    if let Ok(psi) = procfs::IoPressure::current() {
        for (prefix, record) in [("some", &psi.some), ("full", &psi.full)] {
            timeseries.push(create_timeseries(
                &format!("agemon_psi_io_{prefix}_avg10"),
                record.avg10.into(),
                timestamp,
                hostname,
            ));
            timeseries.push(create_timeseries(
                &format!("agemon_psi_io_{prefix}_avg60"),
                record.avg60.into(),
                timestamp,
                hostname,
            ));
            timeseries.push(create_timeseries(
                &format!("agemon_psi_io_{prefix}_avg300"),
                record.avg300.into(),
                timestamp,
                hostname,
            ));
            timeseries.push(create_timeseries(
                &format!("agemon_psi_io_{prefix}_total_us"),
                record.total as f64,
                timestamp,
                hostname,
            ));
        }
    }

    // Vmstat - page faults, swap activity, OOM kills
    if let Ok(vmstat) = procfs::vmstat() {
        for (key, metric_name) in [
            ("pgfault", "agemon_vmstat_pgfault_total"),
            ("pgmajfault", "agemon_vmstat_pgmajfault_total"),
            ("pgpgin", "agemon_vmstat_pgpgin_total"),
            ("pgpgout", "agemon_vmstat_pgpgout_total"),
            ("pswpin", "agemon_vmstat_pswpin_total"),
            ("pswpout", "agemon_vmstat_pswpout_total"),
            ("oom_kill", "agemon_vmstat_oom_kill_total"),
        ] {
            if let Some(&value) = vmstat.get(key) {
                timeseries.push(create_timeseries(
                    metric_name,
                    value as f64,
                    timestamp,
                    hostname,
                ));
            }
        }
    }

    // SNMP TCP/UDP stats - retransmits, segments in/out
    if let Ok(snmp) = procfs::net::snmp() {
        timeseries.push(create_timeseries(
            "agemon_tcp_retrans_segs_total",
            snmp.tcp_retrans_segs as f64,
            timestamp,
            hostname,
        ));
        timeseries.push(create_timeseries(
            "agemon_tcp_in_segs_total",
            snmp.tcp_in_segs as f64,
            timestamp,
            hostname,
        ));
        timeseries.push(create_timeseries(
            "agemon_tcp_out_segs_total",
            snmp.tcp_out_segs as f64,
            timestamp,
            hostname,
        ));
        timeseries.push(create_timeseries(
            "agemon_tcp_active_opens_total",
            snmp.tcp_active_opens as f64,
            timestamp,
            hostname,
        ));
        timeseries.push(create_timeseries(
            "agemon_tcp_passive_opens_total",
            snmp.tcp_passive_opens as f64,
            timestamp,
            hostname,
        ));
        timeseries.push(create_timeseries(
            "agemon_tcp_curr_estab",
            snmp.tcp_curr_estab as f64,
            timestamp,
            hostname,
        ));
        timeseries.push(create_timeseries(
            "agemon_udp_in_datagrams_total",
            snmp.udp_in_datagrams as f64,
            timestamp,
            hostname,
        ));
        timeseries.push(create_timeseries(
            "agemon_udp_out_datagrams_total",
            snmp.udp_out_datagrams as f64,
            timestamp,
            hostname,
        ));
        timeseries.push(create_timeseries(
            "agemon_udp_in_errors_total",
            snmp.udp_in_errors as f64,
            timestamp,
            hostname,
        ));
    }

    // Entropy available
    if let Ok(entropy) = procfs::sys::kernel::random::entropy_avail() {
        timeseries.push(create_timeseries(
            "agemon_entropy_available",
            entropy as f64,
            timestamp,
            hostname,
        ));
    }
}

fn collect_temperature_metrics(
    components: &Components,
    timestamp: i64,
    hostname: &str,
    timeseries: &mut Vec<TimeSeries>,
) {
    for component in components.list() {
        let sensor = component.label();
        let labels = vec![("sensor", sensor)];

        // agemon_temperature_celsius: Current temperature of the sensor
        if let Some(temp) = component.temperature() {
            timeseries.push(create_timeseries_with_labels(
                "agemon_temperature_celsius",
                temp as f64,
                timestamp,
                hostname,
                labels.clone(),
            ));
        }

        // agemon_temperature_max_celsius: Maximum observed temperature of the sensor
        if let Some(max) = component.max() {
            timeseries.push(create_timeseries_with_labels(
                "agemon_temperature_max_celsius",
                max as f64,
                timestamp,
                hostname,
                labels.clone(),
            ));
        }

        // agemon_temperature_critical_celsius: Critical threshold temperature (only if available)
        if let Some(critical) = component.critical() {
            timeseries.push(create_timeseries_with_labels(
                "agemon_temperature_critical_celsius",
                critical as f64,
                timestamp,
                hostname,
                labels,
            ));
        }
    }
}

fn collect_process_metrics(
    sys: &System,
    timestamp: i64,
    hostname: &str,
    top_n: usize,
    timeseries: &mut Vec<TimeSeries>,
) {
    // Aggregate CPU and memory by process name
    let mut cpu_by_name: HashMap<String, f64> = HashMap::new();
    let mut mem_by_name: HashMap<String, u64> = HashMap::new();

    for process in sys.processes().values() {
        let name = process.name().to_string_lossy().into_owned();
        *cpu_by_name.entry(name.clone()).or_default() += process.cpu_usage() as f64;
        // Only count memory for non-thread processes to avoid double-counting RSS
        // (threads share address space, so each thread reports the same RSS as its parent)
        if process.thread_kind().is_none() {
            *mem_by_name.entry(name).or_default() += process.memory();
        }
    }

    // agemon_process_count: Total number of running processes
    timeseries.push(create_timeseries(
        "agemon_process_count",
        sys.processes().len() as f64,
        timestamp,
        hostname,
    ));

    // Top N by CPU
    let mut cpu_sorted: Vec<_> = cpu_by_name.into_iter().collect();
    cpu_sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let mut other_cpu = 0.0;
    for (i, (name, cpu)) in cpu_sorted.iter().enumerate() {
        if i < top_n {
            timeseries.push(create_timeseries_with_labels(
                "agemon_process_cpu_usage_percent",
                *cpu,
                timestamp,
                hostname,
                vec![("process", name.as_str())],
            ));
        } else {
            other_cpu += cpu;
        }
    }
    if cpu_sorted.len() > top_n {
        timeseries.push(create_timeseries_with_labels(
            "agemon_process_cpu_usage_percent",
            other_cpu,
            timestamp,
            hostname,
            vec![("process", "other")],
        ));
    }

    // Top N by memory
    let mut mem_sorted: Vec<_> = mem_by_name.into_iter().collect();
    mem_sorted.sort_by(|a, b| b.1.cmp(&a.1));

    let mut other_mem: u64 = 0;
    for (i, (name, mem)) in mem_sorted.iter().enumerate() {
        if i < top_n {
            timeseries.push(create_timeseries_with_labels(
                "agemon_process_memory_bytes",
                *mem as f64,
                timestamp,
                hostname,
                vec![("process", name.as_str())],
            ));
        } else {
            other_mem += mem;
        }
    }
    if mem_sorted.len() > top_n {
        timeseries.push(create_timeseries_with_labels(
            "agemon_process_memory_bytes",
            other_mem as f64,
            timestamp,
            hostname,
            vec![("process", "other")],
        ));
    }
}

fn collect_metrics(
    sys: &mut System,
    disks: &mut Disks,
    networks: &mut Networks,
    components: &mut Components,
    top_processes: usize,
) -> Vec<TimeSeries> {
    let hostname = System::host_name().unwrap_or_else(|| "unknown".to_string());

    let timestamp: i64 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;

    let mut timeseries = vec![];

    sys.refresh_memory_specifics(MemoryRefreshKind::everything());
    sys.refresh_cpu_usage();
    sys.refresh_processes_specifics(
        sysinfo::ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing()
            .with_disk_usage()
            .with_cpu()
            .with_memory(),
    );
    disks.refresh(true);
    networks.refresh(true);
    components.refresh(true);

    collect_cpu_metrics(sys, timestamp, &hostname, &mut timeseries);
    collect_memory_metrics(sys, timestamp, &hostname, &mut timeseries);
    collect_disk_metrics(disks, timestamp, &hostname, &mut timeseries);
    collect_disk_io_metrics(sys, timestamp, &hostname, &mut timeseries);
    collect_network_metrics(networks, timestamp, &hostname, &mut timeseries);
    collect_temperature_metrics(components, timestamp, &hostname, &mut timeseries);
    collect_system_metrics(timestamp, &hostname, &mut timeseries);
    #[cfg(target_os = "linux")]
    collect_procfs_metrics(timestamp, &hostname, &mut timeseries);

    if top_processes > 0 {
        collect_process_metrics(sys, timestamp, &hostname, top_processes, &mut timeseries);
    }

    timeseries
}

fn push_metrics(client: &Client, args: &Args, timeseries: Vec<TimeSeries>) -> Result<()> {
    let write_request = WriteRequest { timeseries };

    let mut req = write_request
        .build_http_request(
            &args
                .remote_write_url
                .parse::<Url>()
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

fn collect_and_push(
    client: &Client,
    args: &Args,
    sys: &mut System,
    disks: &mut Disks,
    networks: &mut Networks,
    components: &mut Components,
) -> Result<()> {
    let timeseries = collect_metrics(sys, disks, networks, components, args.top_processes);
    info!("collected {} metrics", timeseries.len());
    push_metrics(client, args, timeseries)?;
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

    let client = Client::builder()
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
