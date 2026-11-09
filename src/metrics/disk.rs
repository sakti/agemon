use std::time::Duration;

use sysinfo::{Disks, System};
#[cfg(target_os = "linux")]
use tracing::debug;

use super::MetricSink;

pub fn collect(disks: &Disks, sink: &mut MetricSink) {
    for disk in disks {
        let mount_point = disk.mount_point().to_string_lossy().into_owned();
        let device = disk.name().to_string_lossy().into_owned();
        let fs_type = disk.file_system().to_string_lossy().into_owned();

        let labels = [
            ("mount_point", mount_point.as_str()),
            ("device", device.as_str()),
            ("fs_type", fs_type.as_str()),
        ];

        sink.record_with(
            "agemon_disk_total_bytes",
            disk.total_space() as f64,
            &labels,
        );
        sink.record_with(
            "agemon_disk_available_bytes",
            disk.available_space() as f64,
            &labels,
        );

        let used = disk.total_space().saturating_sub(disk.available_space());
        sink.record_with("agemon_disk_used_bytes", used as f64, &labels);

        let usage_ratio = if disk.total_space() > 0 {
            used as f64 / disk.total_space() as f64
        } else {
            0.0
        };
        sink.record_with("agemon_disk_usage_ratio", usage_ratio, &labels);

        sink.record_with(
            "agemon_disk_is_removable",
            if disk.is_removable() { 1.0 } else { 0.0 },
            &labels,
        );
    }
}

/// Per-second disk I/O rates, summed over all processes.
///
/// sysinfo reports per-process `read_bytes`/`written_bytes` as the delta since the
/// previous refresh, so the sum is divided by the measured time between refreshes.
pub fn collect_io(sys: &System, elapsed: Duration, sink: &mut MetricSink) {
    let (read, written) = sys
        .processes()
        .values()
        .filter(|p| p.thread_kind().is_none())
        .map(|p| p.disk_usage())
        .fold((0u64, 0u64), |(r, w), u| {
            (r + u.read_bytes, w + u.written_bytes)
        });

    let secs = elapsed.as_secs_f64();
    let (read_rate, written_rate) = if secs > 0.0 {
        (read as f64 / secs, written as f64 / secs)
    } else {
        (0.0, 0.0)
    };

    sink.record("agemon_disk_io_read_bytes_per_sec", read_rate);
    sink.record("agemon_disk_io_written_bytes_per_sec", written_rate);
}

/// Monotonic per-device byte counters from `/proc/diskstats`.
///
/// The kernel always reports sectors in 512-byte units, regardless of the device's
/// physical sector size. Partitions are reported alongside their parent disk, so
/// summing across `device` double-counts.
#[cfg(target_os = "linux")]
pub fn collect_diskstats(sink: &mut MetricSink) {
    const SECTOR_SIZE: f64 = 512.0;

    let stats = match procfs::diskstats() {
        Ok(stats) => stats,
        Err(err) => {
            debug!("failed to read /proc/diskstats: {err}");
            return;
        }
    };

    for stat in stats {
        if stat.name.starts_with("loop") || stat.name.starts_with("ram") {
            continue;
        }
        let labels = [("device", stat.name.as_str())];
        sink.record_with(
            "agemon_disk_io_read_bytes_total",
            stat.sectors_read as f64 * SECTOR_SIZE,
            &labels,
        );
        sink.record_with(
            "agemon_disk_io_written_bytes_total",
            stat.sectors_written as f64 * SECTOR_SIZE,
            &labels,
        );
    }
}

#[cfg(test)]
mod tests {
    use sysinfo::{Disks, ProcessRefreshKind, System};

    use super::*;
    use crate::metrics::{find_metric, get_label_value};

    #[test]
    fn test_disk_metrics_have_required_labels() {
        let disks = Disks::new_with_refreshed_list().unwrap();
        let mut sink = MetricSink::new(1000, "testhost");
        collect(&disks, &mut sink);
        let timeseries = sink.into_series();

        for ts in &timeseries {
            assert!(get_label_value(ts, "mount_point").is_some());
            assert!(get_label_value(ts, "device").is_some());
            assert!(get_label_value(ts, "fs_type").is_some());
        }
    }

    #[test]
    fn test_five_metrics_per_disk() {
        let disks = Disks::new_with_refreshed_list().unwrap();
        let mut sink = MetricSink::new(1000, "testhost");
        collect(&disks, &mut sink);
        let timeseries = sink.into_series();

        let disk_count = disks.len();
        if disk_count > 0 {
            assert_eq!(timeseries.len(), disk_count * 5);
        }
    }

    #[test]
    fn test_disk_io_rate_metrics() {
        let mut sys = System::new().unwrap();
        sys.refresh_processes_specifics(
            sysinfo::ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing().with_disk_usage(),
        );
        let mut sink = MetricSink::new(1000, "testhost");
        collect_io(&sys, Duration::from_secs(15), &mut sink);
        let timeseries = sink.into_series();

        for name in [
            "agemon_disk_io_read_bytes_per_sec",
            "agemon_disk_io_written_bytes_per_sec",
        ] {
            let found = find_metric(&timeseries, name);
            assert_eq!(found.len(), 1, "expected exactly one {name}");
        }
    }

    #[test]
    fn test_disk_io_rate_zero_elapsed_is_zero() {
        let sys = System::new().unwrap();
        let mut sink = MetricSink::new(1000, "testhost");
        collect_io(&sys, Duration::ZERO, &mut sink);
        for ts in sink.into_series() {
            assert_eq!(ts.samples[0].value, 0.0);
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_diskstats_metrics_have_device_label() {
        let mut sink = MetricSink::new(1000, "testhost");
        collect_diskstats(&mut sink);
        let timeseries = sink.into_series();

        let read = find_metric(&timeseries, "agemon_disk_io_read_bytes_total");
        let written = find_metric(&timeseries, "agemon_disk_io_written_bytes_total");
        assert_eq!(read.len(), written.len());
        for ts in read.iter().chain(written.iter()) {
            let device = get_label_value(ts, "device").expect("device label");
            assert!(!device.starts_with("loop") && !device.starts_with("ram"));
        }
    }
}
