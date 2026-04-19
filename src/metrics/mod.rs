mod cpu;
mod disk;
mod memory;
mod network;
mod process;
#[cfg(target_os = "linux")]
mod procfs;
mod system;
mod temperature;

use prometheus_remote_write::{LABEL_NAME, Label, Sample, TimeSeries};
use sysinfo::{Components, Disks, MemoryRefreshKind, Networks, ProcessRefreshKind, System};

pub fn create_timeseries(
    metric_name: &str,
    value: f64,
    timestamp: i64,
    hostname: &str,
) -> TimeSeries {
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

pub fn create_timeseries_with_labels(
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

pub fn collect_metrics(
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

    cpu::collect(sys, timestamp, &hostname, &mut timeseries);
    memory::collect(sys, timestamp, &hostname, &mut timeseries);
    disk::collect(disks, timestamp, &hostname, &mut timeseries);
    disk::collect_io(sys, timestamp, &hostname, &mut timeseries);
    network::collect(networks, timestamp, &hostname, &mut timeseries);
    temperature::collect(components, timestamp, &hostname, &mut timeseries);
    system::collect(timestamp, &hostname, &mut timeseries);
    #[cfg(target_os = "linux")]
    procfs::collect(timestamp, &hostname, &mut timeseries);

    if top_processes > 0 {
        process::collect(sys, timestamp, &hostname, top_processes, &mut timeseries);
    }

    timeseries
}

#[cfg(test)]
pub fn find_metric<'a>(
    timeseries: &'a [TimeSeries],
    name: &str,
) -> Vec<&'a TimeSeries> {
    timeseries
        .iter()
        .filter(|ts| {
            ts.labels
                .iter()
                .any(|l| l.name == LABEL_NAME && l.value == name)
        })
        .collect()
}

#[cfg(test)]
pub fn get_label_value<'a>(ts: &'a TimeSeries, label_name: &str) -> Option<&'a str> {
    ts.labels
        .iter()
        .find(|l| l.name == label_name)
        .map(|l| l.value.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_timeseries() {
        let ts = create_timeseries("agemon_test_metric", 42.0, 1000, "testhost");
        assert_eq!(ts.labels.len(), 2);
        assert_eq!(ts.labels[0].name, "hostname");
        assert_eq!(ts.labels[0].value, "testhost");
        assert_eq!(ts.labels[1].name, LABEL_NAME);
        assert_eq!(ts.labels[1].value, "agemon_test_metric");
        assert_eq!(ts.samples.len(), 1);
        assert_eq!(ts.samples[0].value, 42.0);
        assert_eq!(ts.samples[0].timestamp, 1000);
    }

    #[test]
    fn test_create_timeseries_with_labels() {
        let ts = create_timeseries_with_labels(
            "agemon_test_metric",
            99.5,
            2000,
            "testhost",
            vec![("env", "prod"), ("region", "us-east")],
        );
        assert_eq!(ts.labels.len(), 4);
        assert_eq!(get_label_value(&ts, "hostname"), Some("testhost"));
        assert_eq!(get_label_value(&ts, LABEL_NAME), Some("agemon_test_metric"));
        assert_eq!(get_label_value(&ts, "env"), Some("prod"));
        assert_eq!(get_label_value(&ts, "region"), Some("us-east"));
        assert_eq!(ts.samples[0].value, 99.5);
    }

    #[test]
    fn test_create_timeseries_with_empty_extra_labels() {
        let ts = create_timeseries_with_labels("agemon_test", 1.0, 1000, "host", vec![]);
        assert_eq!(ts.labels.len(), 2);
    }

    #[test]
    fn test_collect_metrics_returns_nonempty() {
        let mut sys = System::new_all();
        let mut disks = Disks::new_with_refreshed_list();
        let mut networks = Networks::new_with_refreshed_list();
        let mut components = Components::new_with_refreshed_list();

        let timeseries = collect_metrics(&mut sys, &mut disks, &mut networks, &mut components, 10);
        assert!(!timeseries.is_empty());
    }

    #[test]
    fn test_collect_metrics_all_have_hostname() {
        let mut sys = System::new_all();
        let mut disks = Disks::new_with_refreshed_list();
        let mut networks = Networks::new_with_refreshed_list();
        let mut components = Components::new_with_refreshed_list();

        let timeseries = collect_metrics(&mut sys, &mut disks, &mut networks, &mut components, 0);
        for ts in &timeseries {
            assert!(
                ts.labels.iter().any(|l| l.name == "hostname"),
                "missing hostname label on metric: {:?}",
                ts.labels
            );
        }
    }

    #[test]
    fn test_collect_metrics_all_have_name() {
        let mut sys = System::new_all();
        let mut disks = Disks::new_with_refreshed_list();
        let mut networks = Networks::new_with_refreshed_list();
        let mut components = Components::new_with_refreshed_list();

        let timeseries = collect_metrics(&mut sys, &mut disks, &mut networks, &mut components, 0);
        for ts in &timeseries {
            assert!(
                ts.labels.iter().any(|l| l.name == LABEL_NAME),
                "missing __name__ label on metric: {:?}",
                ts.labels
            );
        }
    }

    #[test]
    fn test_collect_metrics_with_processes_disabled() {
        let mut sys = System::new_all();
        let mut disks = Disks::new_with_refreshed_list();
        let mut networks = Networks::new_with_refreshed_list();
        let mut components = Components::new_with_refreshed_list();

        let timeseries = collect_metrics(&mut sys, &mut disks, &mut networks, &mut components, 0);
        let process_metrics = find_metric(&timeseries, "agemon_process_cpu_usage_percent");
        assert!(process_metrics.is_empty());
    }
}
