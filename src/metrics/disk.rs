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
    }
}

pub fn collect_io(sys: &System, sink: &mut MetricSink) {
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

    sink.record("agemon_disk_io_read_bytes_total", total_read_bytes as f64);
    sink.record(
        "agemon_disk_io_written_bytes_total",
        total_written_bytes as f64,
    );
    sink.record(
        "agemon_disk_io_read_bytes_per_sec",
        read_bytes_per_sec as f64,
    );
    sink.record(
        "agemon_disk_io_written_bytes_per_sec",
        written_bytes_per_sec as f64,
    );
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
    fn test_disk_io_metrics() {
        let mut sys = System::new().unwrap();
        sys.refresh_processes_specifics(
            sysinfo::ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing().with_disk_usage(),
        );
        let mut sink = MetricSink::new(1000, "testhost");
        collect_io(&sys, &mut sink);
        let timeseries = sink.into_series();

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
