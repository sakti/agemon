use procfs::{Current, CurrentSI, ProcResult};
use tracing::debug;

use super::MetricSink;

/// Unwraps a /proc read, logging the error at debug level so missing metrics leave a trace.
fn ok_or_log<T>(result: ProcResult<T>, source: &str) -> Option<T> {
    result
        .inspect_err(|err| debug!("failed to read {source}: {err}"))
        .ok()
}

pub fn collect(sink: &mut MetricSink) {
    collect_tcp_connections(sink);
    collect_file_descriptors(sink);
    collect_kernel_stats(sink);
    collect_psi(sink);
    collect_vmstat(sink);
    collect_snmp(sink);
    collect_entropy(sink);
}

fn collect_tcp_connections(sink: &mut MetricSink) {
    for (metric_name, source, entries) in [
        (
            "agemon_tcp_connections",
            "/proc/net/tcp",
            procfs::net::tcp(),
        ),
        (
            "agemon_tcp6_connections",
            "/proc/net/tcp6",
            procfs::net::tcp6(),
        ),
    ] {
        if let Some(entries) = ok_or_log(entries, source) {
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
                sink.record_with(metric_name, count as f64, &[("state", state)]);
            }
        }
    }
}

fn collect_file_descriptors(sink: &mut MetricSink) {
    if let Some(file_state) = ok_or_log(procfs::sys::fs::file_nr(), "/proc/sys/fs/file-nr") {
        sink.record(
            "agemon_file_descriptors_allocated",
            file_state.allocated as f64,
        );
        sink.record("agemon_file_descriptors_max", file_state.max as f64);
    }
}

fn collect_kernel_stats(sink: &mut MetricSink) {
    if let Some(kernel_stats) = ok_or_log(procfs::KernelStats::current(), "/proc/stat") {
        sink.record("agemon_context_switches_total", kernel_stats.ctxt as f64);
        sink.record(
            "agemon_processes_forked_total",
            kernel_stats.processes as f64,
        );
        sink.record(
            "agemon_procs_running",
            kernel_stats.procs_running.unwrap_or(0) as f64,
        );
        sink.record(
            "agemon_procs_blocked",
            kernel_stats.procs_blocked.unwrap_or(0) as f64,
        );
    }
}

fn collect_psi(sink: &mut MetricSink) {
    if let Some(psi) = ok_or_log(procfs::CpuPressure::current(), "/proc/pressure/cpu") {
        sink.record("agemon_psi_cpu_some_avg10", psi.some.avg10.into());
        sink.record("agemon_psi_cpu_some_avg60", psi.some.avg60.into());
        sink.record("agemon_psi_cpu_some_avg300", psi.some.avg300.into());
        sink.record("agemon_psi_cpu_some_total_us", psi.some.total as f64);
    }

    if let Some(psi) = ok_or_log(procfs::MemoryPressure::current(), "/proc/pressure/memory") {
        for (prefix, record) in [("some", &psi.some), ("full", &psi.full)] {
            sink.record(
                &format!("agemon_psi_memory_{prefix}_avg10"),
                record.avg10.into(),
            );
            sink.record(
                &format!("agemon_psi_memory_{prefix}_avg60"),
                record.avg60.into(),
            );
            sink.record(
                &format!("agemon_psi_memory_{prefix}_avg300"),
                record.avg300.into(),
            );
            sink.record(
                &format!("agemon_psi_memory_{prefix}_total_us"),
                record.total as f64,
            );
        }
    }

    if let Some(psi) = ok_or_log(procfs::IoPressure::current(), "/proc/pressure/io") {
        for (prefix, record) in [("some", &psi.some), ("full", &psi.full)] {
            sink.record(
                &format!("agemon_psi_io_{prefix}_avg10"),
                record.avg10.into(),
            );
            sink.record(
                &format!("agemon_psi_io_{prefix}_avg60"),
                record.avg60.into(),
            );
            sink.record(
                &format!("agemon_psi_io_{prefix}_avg300"),
                record.avg300.into(),
            );
            sink.record(
                &format!("agemon_psi_io_{prefix}_total_us"),
                record.total as f64,
            );
        }
    }
}

fn collect_vmstat(sink: &mut MetricSink) {
    if let Some(vmstat) = ok_or_log(procfs::vmstat(), "/proc/vmstat") {
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
                sink.record(metric_name, value as f64);
            }
        }
    }
}

fn collect_snmp(sink: &mut MetricSink) {
    if let Some(snmp) = ok_or_log(procfs::net::snmp(), "/proc/net/snmp") {
        sink.record(
            "agemon_tcp_retrans_segs_total",
            snmp.tcp_retrans_segs as f64,
        );
        sink.record("agemon_tcp_in_segs_total", snmp.tcp_in_segs as f64);
        sink.record("agemon_tcp_out_segs_total", snmp.tcp_out_segs as f64);
        sink.record(
            "agemon_tcp_active_opens_total",
            snmp.tcp_active_opens as f64,
        );
        sink.record(
            "agemon_tcp_passive_opens_total",
            snmp.tcp_passive_opens as f64,
        );
        sink.record("agemon_tcp_curr_estab", snmp.tcp_curr_estab as f64);
        sink.record(
            "agemon_udp_in_datagrams_total",
            snmp.udp_in_datagrams as f64,
        );
        sink.record(
            "agemon_udp_out_datagrams_total",
            snmp.udp_out_datagrams as f64,
        );
        sink.record("agemon_udp_in_errors_total", snmp.udp_in_errors as f64);
    }
}

fn collect_entropy(sink: &mut MetricSink) {
    if let Some(entropy) = ok_or_log(
        procfs::sys::kernel::random::entropy_avail(),
        "/proc/sys/kernel/random/entropy_avail",
    ) {
        sink.record("agemon_entropy_available", entropy as f64);
    }
}
