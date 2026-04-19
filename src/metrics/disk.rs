use prometheus_remote_write::TimeSeries;
use sysinfo::{Disks, System};

use super::{create_timeseries, create_timeseries_with_labels};

pub fn collect(disks: &Disks, timestamp: i64, hostname: &str, timeseries: &mut Vec<TimeSeries>) {
    for disk in disks.list() {
        let mount_point = disk.mount_point().to_string_lossy().into_owned();
        let device = disk.name().to_string_lossy().into_owned();
        let fs_type = disk.file_system().to_string_lossy().into_owned();

        let labels = vec![
            ("mount_point", mount_point.as_str()),
            ("device", device.as_str()),
            ("fs_type", fs_type.as_str()),
        ];

        timeseries.push(create_timeseries_with_labels(
            "agemon_disk_total_bytes",
            disk.total_space() as f64,
            timestamp,
            hostname,
            labels.clone(),
        ));

        timeseries.push(create_timeseries_with_labels(
            "agemon_disk_available_bytes",
            disk.available_space() as f64,
            timestamp,
            hostname,
            labels.clone(),
        ));

        let used = disk.total_space().saturating_sub(disk.available_space());
        timeseries.push(create_timeseries_with_labels(
            "agemon_disk_used_bytes",
            used as f64,
            timestamp,
            hostname,
            labels.clone(),
        ));

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

        timeseries.push(create_timeseries_with_labels(
            "agemon_disk_is_removable",
            if disk.is_removable() { 1.0 } else { 0.0 },
            timestamp,
            hostname,
            labels,
        ));
    }
}

pub fn collect_io(sys: &System, timestamp: i64, hostname: &str, timeseries: &mut Vec<TimeSeries>) {
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

    timeseries.push(create_timeseries(
        "agemon_disk_io_read_bytes_total",
        total_read_bytes as f64,
        timestamp,
        hostname,
    ));

    timeseries.push(create_timeseries(
        "agemon_disk_io_written_bytes_total",
        total_written_bytes as f64,
        timestamp,
        hostname,
    ));

    timeseries.push(create_timeseries(
        "agemon_disk_io_read_bytes_per_sec",
        read_bytes_per_sec as f64,
        timestamp,
        hostname,
    ));

    timeseries.push(create_timeseries(
        "agemon_disk_io_written_bytes_per_sec",
        written_bytes_per_sec as f64,
        timestamp,
        hostname,
    ));
}

#[cfg(test)]
mod tests {
    use sysinfo::{Disks, ProcessRefreshKind, System};

    use super::*;
    use crate::metrics::{find_metric, get_label_value};

    #[test]
    fn test_disk_metrics_have_required_labels() {
        let disks = Disks::new_with_refreshed_list();
        let mut timeseries = vec![];
        collect(&disks, 1000, "testhost", &mut timeseries);

        for ts in &timeseries {
            assert!(get_label_value(ts, "mount_point").is_some());
            assert!(get_label_value(ts, "device").is_some());
            assert!(get_label_value(ts, "fs_type").is_some());
        }
    }

    #[test]
    fn test_five_metrics_per_disk() {
        let disks = Disks::new_with_refreshed_list();
        let mut timeseries = vec![];
        collect(&disks, 1000, "testhost", &mut timeseries);

        let disk_count = disks.list().len();
        if disk_count > 0 {
            assert_eq!(timeseries.len(), disk_count * 5);
        }
    }

    #[test]
    fn test_disk_io_metrics() {
        let mut sys = System::new();
        sys.refresh_processes_specifics(
            sysinfo::ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing().with_disk_usage(),
        );
        let mut timeseries = vec![];
        collect_io(&sys, 1000, "testhost", &mut timeseries);

        let expected = [
            "agemon_disk_io_read_bytes_total",
            "agemon_disk_io_written_bytes_total",
            "agemon_disk_io_read_bytes_per_sec",
            "agemon_disk_io_written_bytes_per_sec",
        ];
        for name in expected {
            let found = find_metric(&timeseries, name);
            assert_eq!(found.len(), 1, "expected exactly one {name}");
        }
    }
}
