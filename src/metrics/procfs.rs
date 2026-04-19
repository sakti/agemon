use procfs::{Current, CurrentSI};
use prometheus_remote_write::TimeSeries;

use super::{create_timeseries, create_timeseries_with_labels};

pub fn collect(timestamp: i64, hostname: &str, timeseries: &mut Vec<TimeSeries>) {
    collect_tcp_connections(timestamp, hostname, timeseries);
    collect_file_descriptors(timestamp, hostname, timeseries);
    collect_kernel_stats(timestamp, hostname, timeseries);
    collect_psi(timestamp, hostname, timeseries);
    collect_vmstat(timestamp, hostname, timeseries);
    collect_snmp(timestamp, hostname, timeseries);
    collect_entropy(timestamp, hostname, timeseries);
}

fn collect_tcp_connections(timestamp: i64, hostname: &str, timeseries: &mut Vec<TimeSeries>) {
    for (metric_name, entries) in [
        ("agemon_tcp_connections", procfs::net::tcp()),
        ("agemon_tcp6_connections", procfs::net::tcp6()),
    ] {
        if let Ok(entries) = entries {
            let mut established: u64 = 0;
            let mut listen: u64 = 0;
            let mut time_wait: u64 = 0;
            let mut close_wait: u64 = 0;
            let mut other: u64 = 0;

            for entry in &entries {
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
                    metric_name,
                    count as f64,
                    timestamp,
                    hostname,
                    vec![("state", state)],
                ));
            }
        }
    }
}

fn collect_file_descriptors(timestamp: i64, hostname: &str, timeseries: &mut Vec<TimeSeries>) {
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
}

fn collect_kernel_stats(timestamp: i64, hostname: &str, timeseries: &mut Vec<TimeSeries>) {
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
}

fn collect_psi(timestamp: i64, hostname: &str, timeseries: &mut Vec<TimeSeries>) {
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
}

fn collect_vmstat(timestamp: i64, hostname: &str, timeseries: &mut Vec<TimeSeries>) {
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
}

fn collect_snmp(timestamp: i64, hostname: &str, timeseries: &mut Vec<TimeSeries>) {
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
}

fn collect_entropy(timestamp: i64, hostname: &str, timeseries: &mut Vec<TimeSeries>) {
    if let Ok(entropy) = procfs::sys::kernel::random::entropy_avail() {
        timeseries.push(create_timeseries(
            "agemon_entropy_available",
            entropy as f64,
            timestamp,
            hostname,
        ));
    }
}
