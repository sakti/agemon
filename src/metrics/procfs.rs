use std::{collections::HashMap, fs, io, path::Path};

use procfs::{Current, CurrentSI};

use super::MetricSink;

const MICROS_PER_SEC: f64 = 1e6;
const MILLIS_PER_SEC: f64 = 1e3;
/// `/sys/class/net/<interface>/speed` is in Mbit/s.
const BYTES_PER_SEC_PER_MBIT: f64 = 125_000.0;

pub fn collect(sink: &mut MetricSink) {
    collect_tcp_connections(sink);
    collect_file_descriptors(sink);
    collect_kernel_stats(sink);
    collect_psi(sink);
    collect_vmstat(sink);
    collect_snmp(sink);
    collect_netstat(sink);
    collect_conntrack(sink);
    collect_entropy(sink);
    collect_net_dev(sink);
    collect_timex(sink);
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
        if let Some(entries) = sink.record_source(source, entries) {
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
    if let Some(file_state) = sink.record_source("/proc/sys/fs/file-nr", procfs::sys::fs::file_nr())
    {
        sink.record(
            "agemon_file_descriptors_allocated",
            file_state.allocated as f64,
        );
        sink.record("agemon_file_descriptors_max", file_state.max as f64);
    }
}

fn collect_kernel_stats(sink: &mut MetricSink) {
    if let Some(kernel_stats) = sink.record_source("/proc/stat", procfs::KernelStats::current()) {
        let cpu = &kernel_stats.total;
        for (mode, millis) in [
            ("user", Some(cpu.user_ms())),
            ("nice", Some(cpu.nice_ms())),
            ("system", Some(cpu.system_ms())),
            ("idle", Some(cpu.idle_ms())),
            ("iowait", cpu.iowait_ms()),
            ("irq", cpu.irq_ms()),
            ("softirq", cpu.softirq_ms()),
            ("steal", cpu.steal_ms()),
        ] {
            if let Some(millis) = millis {
                sink.record_with(
                    "agemon_cpu_seconds_total",
                    millis as f64 / MILLIS_PER_SEC,
                    &[("mode", mode)],
                );
            }
        }

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
    if let Some(psi) = sink.record_source("/proc/pressure/cpu", procfs::CpuPressure::current()) {
        sink.record("agemon_psi_cpu_some_avg10", psi.some.avg10.into());
        sink.record("agemon_psi_cpu_some_avg60", psi.some.avg60.into());
        sink.record("agemon_psi_cpu_some_avg300", psi.some.avg300.into());
        sink.record(
            "agemon_psi_cpu_some_seconds_total",
            psi.some.total as f64 / MICROS_PER_SEC,
        );
    }

    if let Some(psi) =
        sink.record_source("/proc/pressure/memory", procfs::MemoryPressure::current())
    {
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
                &format!("agemon_psi_memory_{prefix}_seconds_total"),
                record.total as f64 / MICROS_PER_SEC,
            );
        }
    }

    if let Some(psi) = sink.record_source("/proc/pressure/io", procfs::IoPressure::current()) {
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
                &format!("agemon_psi_io_{prefix}_seconds_total"),
                record.total as f64 / MICROS_PER_SEC,
            );
        }
    }
}

fn collect_vmstat(sink: &mut MetricSink) {
    if let Some(vmstat) = sink.record_source("/proc/vmstat", procfs::vmstat()) {
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

/// Parses `/proc/net/snmp` or `/proc/net/netstat` into a map with keys such as
/// `Tcp:RetransSegs` or `TcpExt:ListenOverflows`.
///
/// The file is a list of line pairs: a header line with the field names and a line
/// with the values, both with the protocol as prefix. The parse is done here and not
/// with `procfs::net::snmp()`, because that function needs every field that it knows
/// and fails on kernels that removed one (for example UDP-Lite in Linux 7).
fn parse_snmp(contents: &str) -> HashMap<String, f64> {
    let mut values = HashMap::new();
    let mut lines = contents.lines();
    while let (Some(header), Some(data)) = (lines.next(), lines.next()) {
        let mut names = header.split_whitespace();
        let mut numbers = data.split_whitespace();
        let (Some(prefix), Some(data_prefix)) = (names.next(), numbers.next()) else {
            continue;
        };
        if prefix != data_prefix {
            continue;
        }
        for (name, number) in names.zip(numbers) {
            if let Ok(number) = number.parse() {
                values.insert(format!("{prefix}{name}"), number);
            }
        }
    }
    values
}

fn collect_snmp(sink: &mut MetricSink) {
    const SOURCE: &str = "/proc/net/snmp";

    let Some(contents) = sink.record_source(SOURCE, fs::read_to_string(SOURCE)) else {
        return;
    };
    let snmp = parse_snmp(&contents);

    for (key, metric_name) in [
        ("Tcp:RetransSegs", "agemon_tcp_retrans_segs_total"),
        ("Tcp:InSegs", "agemon_tcp_in_segs_total"),
        ("Tcp:OutSegs", "agemon_tcp_out_segs_total"),
        ("Tcp:ActiveOpens", "agemon_tcp_active_opens_total"),
        ("Tcp:PassiveOpens", "agemon_tcp_passive_opens_total"),
        ("Tcp:CurrEstab", "agemon_tcp_curr_estab"),
        ("Udp:InDatagrams", "agemon_udp_in_datagrams_total"),
        ("Udp:OutDatagrams", "agemon_udp_out_datagrams_total"),
        ("Udp:InErrors", "agemon_udp_in_errors_total"),
    ] {
        if let Some(&value) = snmp.get(key) {
            sink.record(metric_name, value);
        }
    }
}

/// Listen queue counters from `/proc/net/netstat`. They count the connections that were
/// dropped because the accept queue of a listening socket was full.
fn collect_netstat(sink: &mut MetricSink) {
    const SOURCE: &str = "/proc/net/netstat";

    let Some(contents) = sink.record_source(SOURCE, fs::read_to_string(SOURCE)) else {
        return;
    };
    let netstat = parse_snmp(&contents);

    for (key, metric_name) in [
        (
            "TcpExt:ListenOverflows",
            "agemon_tcp_listen_overflows_total",
        ),
        ("TcpExt:ListenDrops", "agemon_tcp_listen_drops_total"),
    ] {
        if let Some(&value) = netstat.get(key) {
            sink.record(metric_name, value);
        }
    }
}

/// Reads a number from a file. `Ok(None)` means that the file does not exist.
fn read_optional_number(path: &str) -> io::Result<Option<f64>> {
    match fs::read_to_string(path) {
        Ok(contents) => contents
            .trim()
            .parse()
            .map(Some)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err)),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err),
    }
}

/// Connection tracking table use. The files exist only when the `nf_conntrack` module is
/// loaded; without them the metrics and `agemon_source_success` are not reported.
fn collect_conntrack(sink: &mut MetricSink) {
    for (source, metric_name) in [
        (
            "/proc/sys/net/netfilter/nf_conntrack_count",
            "agemon_conntrack_entries",
        ),
        (
            "/proc/sys/net/netfilter/nf_conntrack_max",
            "agemon_conntrack_entries_limit",
        ),
    ] {
        match read_optional_number(source) {
            Ok(None) => {}
            result => {
                if let Some(Some(value)) = sink.record_source(source, result) {
                    sink.record(metric_name, value);
                }
            }
        }
    }
}

fn collect_entropy(sink: &mut MetricSink) {
    if let Some(entropy) = sink.record_source(
        "/proc/sys/kernel/random/entropy_avail",
        procfs::sys::kernel::random::entropy_avail(),
    ) {
        sink.record("agemon_entropy_available", entropy as f64);
    }
}

/// Drop counters from `/proc/net/dev` and link state from `/sys/class/net`.
fn collect_net_dev(sink: &mut MetricSink) {
    let Some(devices) = sink.record_source("/proc/net/dev", procfs::net::dev_status()) else {
        return;
    };

    let mut devices: Vec<_> = devices.into_values().collect();
    devices.sort_by(|a, b| a.name.cmp(&b.name));

    for device in devices {
        let labels = [("interface", device.name.as_str())];
        sink.record_with(
            "agemon_network_received_dropped_total",
            device.recv_drop as f64,
            &labels,
        );
        sink.record_with(
            "agemon_network_transmitted_dropped_total",
            device.sent_drop as f64,
            &labels,
        );

        let sysfs = Path::new("/sys/class/net").join(&device.name);
        if let Ok(operstate) = fs::read_to_string(sysfs.join("operstate")) {
            sink.record_with(
                "agemon_network_up",
                if operstate.trim() == "up" { 1.0 } else { 0.0 },
                &labels,
            );
        }
        // `carrier` and `speed` cannot be read when the interface is administratively
        // down, and virtual interfaces report no speed or -1.
        if let Some(carrier) = read_sysfs_number(&sysfs.join("carrier")) {
            sink.record_with("agemon_network_carrier", carrier, &labels);
        }
        if let Some(speed) = read_sysfs_number(&sysfs.join("speed")).filter(|s| *s > 0.0) {
            sink.record_with(
                "agemon_network_speed_bytes",
                speed * BYTES_PER_SEC_PER_MBIT,
                &labels,
            );
        }
    }
}

/// Kernel clock state from `adjtimex(2)`, read without a change to the clock.
fn read_timex() -> io::Result<(libc::c_int, libc::timex)> {
    // SAFETY: `timex` has only integer fields, so the all-zero value is valid. With
    // `modes == 0` the call only reads the clock state into the struct.
    let mut timex: libc::timex = unsafe { std::mem::zeroed() };
    let state = unsafe { libc::adjtimex(&mut timex) };
    if state == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok((state, timex))
}

/// Clock synchronisation: whether the kernel thinks the clock is synchronised (by NTP,
/// chrony or systemd-timesyncd), and the offset and maximum error that it reports.
fn collect_timex(sink: &mut MetricSink) {
    const NANOS_PER_SEC: f64 = 1e9;

    let Some((state, timex)) = sink.record_source("adjtimex", read_timex()) else {
        return;
    };

    let synchronised = state != libc::TIME_ERROR && timex.status & libc::STA_UNSYNC == 0;
    sink.record(
        "agemon_timex_sync_status",
        if synchronised { 1.0 } else { 0.0 },
    );

    // `offset` is in nanoseconds when STA_NANO is set and in microseconds otherwise.
    let offset_divisor = if timex.status & libc::STA_NANO != 0 {
        NANOS_PER_SEC
    } else {
        MICROS_PER_SEC
    };
    sink.record(
        "agemon_timex_offset_seconds",
        timex.offset as f64 / offset_divisor,
    );
    sink.record(
        "agemon_timex_maxerror_seconds",
        timex.maxerror as f64 / MICROS_PER_SEC,
    );
}

fn read_sysfs_number(path: &Path) -> Option<f64> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::{find_metric, get_label_value};

    fn collected() -> Vec<prometheus_remote_write::TimeSeries> {
        let mut sink = MetricSink::new(1000, "testhost");
        collect(&mut sink);
        sink.into_series()
    }

    #[test]
    fn test_cpu_seconds_have_core_modes() {
        let timeseries = collected();
        let cpu = find_metric(&timeseries, "agemon_cpu_seconds_total");
        let modes: Vec<_> = cpu
            .iter()
            .filter_map(|ts| get_label_value(ts, "mode"))
            .collect();
        for mode in ["user", "nice", "system", "idle"] {
            assert!(modes.contains(&mode), "missing mode {mode}");
        }
        // Each mode appears once: the series are totals over all CPUs.
        assert_eq!(modes.len(), cpu.len());
        let mut unique = modes.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), modes.len());
    }

    #[test]
    fn test_every_source_reports_success() {
        let timeseries = collected();
        let success = find_metric(&timeseries, "agemon_source_success");
        for source in ["/proc/stat", "/proc/vmstat", "/proc/net/dev"] {
            assert!(
                success
                    .iter()
                    .any(|ts| get_label_value(ts, "source") == Some(source)),
                "missing agemon_source_success for {source}"
            );
        }
        for ts in success {
            let value = ts.samples[0].value;
            assert!(value == 0.0 || value == 1.0);
        }
    }

    #[test]
    fn test_parse_snmp_without_udplite_section() {
        // Linux 7 has no UdpLite section; the fields that remain must still parse.
        let snmp = parse_snmp(
            "Tcp: RtoAlgorithm MaxConn CurrEstab InSegs OutSegs RetransSegs\n\
             Tcp: 1 -1 30 2024129 1609527 1903\n\
             Udp: InDatagrams NoPorts InErrors OutDatagrams\n\
             Udp: 1842 1 0 1720\n",
        );
        assert_eq!(snmp.get("Tcp:RetransSegs"), Some(&1903.0));
        assert_eq!(snmp.get("Tcp:MaxConn"), Some(&-1.0));
        assert_eq!(snmp.get("Udp:OutDatagrams"), Some(&1720.0));
        assert_eq!(snmp.get("UdpLite:InDatagrams"), None);
    }

    #[test]
    fn test_parse_snmp_ignores_mismatched_and_incomplete_lines() {
        let snmp = parse_snmp("Tcp: InSegs OutSegs\nUdp: 1 2\nIp: Forwarding\n");
        assert!(snmp.is_empty());
    }

    #[test]
    fn test_snmp_metrics_are_collected() {
        let timeseries = collected();
        for name in [
            "agemon_tcp_retrans_segs_total",
            "agemon_tcp_out_segs_total",
            "agemon_udp_in_datagrams_total",
        ] {
            assert_eq!(find_metric(&timeseries, name).len(), 1, "{name}");
        }
    }

    #[test]
    fn test_parse_netstat_listen_counters() {
        let netstat = parse_snmp(
            "TcpExt: SyncookiesSent ListenOverflows ListenDrops\n\
             TcpExt: 0 12 15\n\
             IpExt: InNoRoutes InOctets\n\
             IpExt: 0 123456\n",
        );
        assert_eq!(netstat.get("TcpExt:ListenOverflows"), Some(&12.0));
        assert_eq!(netstat.get("TcpExt:ListenDrops"), Some(&15.0));
        assert_eq!(netstat.get("IpExt:InOctets"), Some(&123456.0));
    }

    #[test]
    fn test_netstat_metrics_are_collected() {
        let timeseries = collected();
        for name in [
            "agemon_tcp_listen_overflows_total",
            "agemon_tcp_listen_drops_total",
        ] {
            assert_eq!(find_metric(&timeseries, name).len(), 1, "{name}");
        }
    }

    #[test]
    fn test_read_optional_number() {
        let dir = std::env::temp_dir().join(format!("agemon-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("number");
        let path_str = path.to_str().unwrap();

        fs::write(&path, "262144\n").unwrap();
        assert_eq!(read_optional_number(path_str).unwrap(), Some(262144.0));

        fs::write(&path, "not a number\n").unwrap();
        assert!(read_optional_number(path_str).is_err());

        fs::remove_dir_all(&dir).unwrap();
        assert_eq!(read_optional_number(path_str).unwrap(), None);
    }

    #[test]
    fn test_conntrack_metrics_are_paired() {
        let timeseries = collected();
        let entries = find_metric(&timeseries, "agemon_conntrack_entries");
        let limit = find_metric(&timeseries, "agemon_conntrack_entries_limit");
        assert_eq!(entries.len(), limit.len());
        assert!(entries.len() <= 1);
        if Path::new("/proc/sys/net/netfilter/nf_conntrack_count").exists() {
            assert_eq!(entries.len(), 1);
            assert!(limit[0].samples[0].value > 0.0);
        }
    }

    #[test]
    fn test_timex_metrics_are_collected() {
        let timeseries = collected();
        let status = find_metric(&timeseries, "agemon_timex_sync_status");
        assert_eq!(status.len(), 1);
        let value = status[0].samples[0].value;
        assert!(value == 0.0 || value == 1.0);

        let max_error = find_metric(&timeseries, "agemon_timex_maxerror_seconds");
        assert_eq!(max_error.len(), 1);
        assert!(max_error[0].samples[0].value >= 0.0);
        assert_eq!(
            find_metric(&timeseries, "agemon_timex_offset_seconds").len(),
            1
        );
    }

    #[test]
    fn test_net_dev_drop_metrics_are_paired() {
        let timeseries = collected();
        let received = find_metric(&timeseries, "agemon_network_received_dropped_total");
        let transmitted = find_metric(&timeseries, "agemon_network_transmitted_dropped_total");
        assert_eq!(received.len(), transmitted.len());
        for ts in received.iter().chain(transmitted.iter()) {
            assert!(get_label_value(ts, "interface").is_some());
        }
    }
}
