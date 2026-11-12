use std::time::Duration;

use sysinfo::{Disks, System};

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
        sink.record_with(
            "agemon_disk_is_read_only",
            if disk.is_read_only() { 1.0 } else { 0.0 },
            &labels,
        );

        #[cfg(unix)]
        if let Some((total, free)) = inode_counts(disk.mount_point()) {
            sink.record_with("agemon_disk_inodes_total", total as f64, &labels);
            sink.record_with("agemon_disk_inodes_free", free as f64, &labels);
        }
    }
}

/// Total and free inodes of the filesystem at `mount_point`.
///
/// Returns `None` when `statvfs` fails or when the filesystem has no fixed inode
/// table and reports a total of 0 (for example btrfs and vfat).
#[cfg(unix)]
fn inode_counts(mount_point: &std::path::Path) -> Option<(u64, u64)> {
    use std::{ffi::CString, mem::MaybeUninit, os::unix::ffi::OsStrExt};

    let path = CString::new(mount_point.as_os_str().as_bytes()).ok()?;
    let mut stat = MaybeUninit::<libc::statvfs>::uninit();
    // SAFETY: `path` is a valid NUL-terminated string and `stat` points to writable
    // memory of the right size. `statvfs` fills it completely when it returns 0.
    let stat = unsafe {
        if libc::statvfs(path.as_ptr(), stat.as_mut_ptr()) != 0 {
            return None;
        }
        stat.assume_init()
    };

    // The field types differ between platforms.
    #[allow(clippy::unnecessary_cast)]
    let (total, free) = (stat.f_files as u64, stat.f_ffree as u64);
    (total > 0).then_some((total, free))
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

/// Whether a `/proc/diskstats` entry is a real whole device.
///
/// Partitions are skipped because their parent disk already counts their I/O, so a
/// sum across `device` would double-count. `loop`, `ram` and `zram` devices are not
/// physical storage. A whole device has a directory in `/sys/block` (where a `/` in
/// the name is written as `!`); a partition does not.
#[cfg(target_os = "linux")]
fn is_whole_disk(name: &str) -> bool {
    if ["loop", "ram", "zram"]
        .iter()
        .any(|prefix| name.starts_with(prefix))
    {
        return false;
    }
    std::path::Path::new("/sys/block")
        .join(name.replace('/', "!"))
        .exists()
}

/// Monotonic per-device counters from `/proc/diskstats`.
///
/// The kernel always reports sectors in 512-byte units, regardless of the device's
/// physical sector size, and times in milliseconds.
#[cfg(target_os = "linux")]
pub fn collect_diskstats(sink: &mut MetricSink) {
    const SECTOR_SIZE: f64 = 512.0;
    const MILLIS_PER_SEC: f64 = 1e3;

    let Some(stats) = sink.record_source("/proc/diskstats", procfs::diskstats()) else {
        return;
    };

    for stat in stats {
        if !is_whole_disk(&stat.name) {
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
        sink.record_with(
            "agemon_disk_io_reads_completed_total",
            stat.reads as f64,
            &labels,
        );
        sink.record_with(
            "agemon_disk_io_writes_completed_total",
            stat.writes as f64,
            &labels,
        );
        sink.record_with(
            "agemon_disk_io_read_time_seconds_total",
            stat.time_reading as f64 / MILLIS_PER_SEC,
            &labels,
        );
        sink.record_with(
            "agemon_disk_io_write_time_seconds_total",
            stat.time_writing as f64 / MILLIS_PER_SEC,
            &labels,
        );
        sink.record_with(
            "agemon_disk_io_time_seconds_total",
            stat.time_in_progress as f64 / MILLIS_PER_SEC,
            &labels,
        );
        sink.record_with(
            "agemon_disk_io_time_weighted_seconds_total",
            stat.weighted_time_in_progress as f64 / MILLIS_PER_SEC,
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
    fn test_one_series_per_disk_for_each_space_metric() {
        let disks = Disks::new_with_refreshed_list().unwrap();
        let mut sink = MetricSink::new(1000, "testhost");
        collect(&disks, &mut sink);
        let timeseries = sink.into_series();

        for name in [
            "agemon_disk_total_bytes",
            "agemon_disk_available_bytes",
            "agemon_disk_used_bytes",
            "agemon_disk_usage_ratio",
            "agemon_disk_is_removable",
            "agemon_disk_is_read_only",
        ] {
            assert_eq!(find_metric(&timeseries, name).len(), disks.len(), "{name}");
        }
    }

    #[test]
    fn test_inode_metrics_are_paired_and_consistent() {
        let disks = Disks::new_with_refreshed_list().unwrap();
        let mut sink = MetricSink::new(1000, "testhost");
        collect(&disks, &mut sink);
        let timeseries = sink.into_series();

        let total = find_metric(&timeseries, "agemon_disk_inodes_total");
        let free = find_metric(&timeseries, "agemon_disk_inodes_free");
        assert_eq!(total.len(), free.len());
        for (total, free) in total.iter().zip(free.iter()) {
            assert!(total.samples[0].value > 0.0);
            assert!(free.samples[0].value <= total.samples[0].value);
        }
    }

    #[cfg(unix)]
    #[test]
    fn test_inode_counts_missing_path_is_none() {
        assert!(inode_counts(std::path::Path::new("/nonexistent/agemon-mount")).is_none());
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
            assert!(is_whole_disk(device), "{device} is not a whole disk");
        }

        for name in [
            "agemon_disk_io_reads_completed_total",
            "agemon_disk_io_writes_completed_total",
            "agemon_disk_io_read_time_seconds_total",
            "agemon_disk_io_write_time_seconds_total",
            "agemon_disk_io_time_seconds_total",
            "agemon_disk_io_time_weighted_seconds_total",
        ] {
            assert_eq!(find_metric(&timeseries, name).len(), read.len(), "{name}");
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_virtual_devices_are_not_whole_disks() {
        for name in ["loop0", "ram0", "zram0", "agemon-no-such-device"] {
            assert!(!is_whole_disk(name), "{name}");
        }
    }
}
