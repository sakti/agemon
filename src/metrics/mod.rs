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

/// Accumulates timeseries for one collection cycle, stamping every metric
/// with the shared timestamp and hostname labels.
pub struct MetricSink<'a> {
    timestamp: i64,
    hostname: &'a str,
    series: Vec<TimeSeries>,
}

impl<'a> MetricSink<'a> {
    pub fn new(timestamp: i64, hostname: &'a str) -> Self {
        Self {
            timestamp,
            hostname,
            series: Vec::new(),
        }
    }

    pub fn record(&mut self, name: &str, value: f64) {
        self.record_with(name, value, &[]);
    }

    pub fn record_with(&mut self, name: &str, value: f64, extra: &[(&str, &str)]) {
        let mut labels = Vec::with_capacity(2 + extra.len());
        labels.push(Label {
            name: "hostname".to_string(),
            value: self.hostname.to_string(),
        });
        labels.push(Label {
            name: LABEL_NAME.to_string(),
            value: name.to_string(),
        });
        for (k, v) in extra {
            labels.push(Label {
                name: (*k).to_string(),
                value: (*v).to_string(),
            });
        }
        self.series.push(TimeSeries {
            labels,
            samples: vec![Sample {
                value,
                timestamp: self.timestamp,
            }],
        });
    }

    pub fn into_series(self) -> Vec<TimeSeries> {
        self.series
    }
}

/// Owns the sysinfo handles and knows how to refresh them between collections.
pub struct SystemSources {
    sys: System,
    disks: Disks,
    networks: Networks,
    components: Components,
}

impl SystemSources {
    pub fn new() -> Result<Self, sysinfo::Error> {
        Ok(Self {
            sys: System::new_all()?,
            disks: Disks::new_with_refreshed_list()?,
            networks: Networks::new_with_refreshed_list()?,
            components: Components::new_with_refreshed_list()?,
        })
    }

    fn refresh(&mut self) {
        self.sys
            .refresh_memory_specifics(MemoryRefreshKind::everything());
        self.sys.refresh_cpu_usage();
        self.sys.refresh_processes_specifics(
            sysinfo::ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing()
                .with_disk_usage()
                .with_cpu()
                .with_memory(),
        );
        self.disks.refresh(true);
        self.networks.refresh(true);
        self.components.refresh(true);
    }
}

pub fn collect_metrics(
    sources: &mut SystemSources,
    hostname: &str,
    top_processes: usize,
) -> Vec<TimeSeries> {
    let timestamp: i64 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;

    sources.refresh();

    let mut sink = MetricSink::new(timestamp, hostname);

    cpu::collect(&sources.sys, &mut sink);
    memory::collect(&sources.sys, &mut sink);
    disk::collect(&sources.disks, &mut sink);
    disk::collect_io(&sources.sys, &mut sink);
    network::collect(&sources.networks, &mut sink);
    temperature::collect(&sources.components, &mut sink);
    system::collect(&mut sink);
    #[cfg(target_os = "linux")]
    procfs::collect(&mut sink);

    if top_processes > 0 {
        process::collect(&sources.sys, top_processes, &mut sink);
    }

    sink.into_series()
}

#[cfg(test)]
pub fn find_metric<'a>(timeseries: &'a [TimeSeries], name: &str) -> Vec<&'a TimeSeries> {
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
    fn test_record_adds_name_and_hostname_labels() {
        let mut sink = MetricSink::new(1000, "testhost");
        sink.record("agemon_test_metric", 42.0);
        let series = sink.into_series();

        assert_eq!(series.len(), 1);
        let ts = &series[0];
        assert_eq!(ts.labels.len(), 2);
        assert_eq!(get_label_value(ts, "hostname"), Some("testhost"));
        assert_eq!(get_label_value(ts, LABEL_NAME), Some("agemon_test_metric"));
        assert_eq!(ts.samples.len(), 1);
        assert_eq!(ts.samples[0].value, 42.0);
        assert_eq!(ts.samples[0].timestamp, 1000);
    }

    #[test]
    fn test_record_with_extra_labels() {
        let mut sink = MetricSink::new(2000, "testhost");
        sink.record_with(
            "agemon_test_metric",
            99.5,
            &[("env", "prod"), ("region", "us-east")],
        );
        let series = sink.into_series();

        let ts = &series[0];
        assert_eq!(ts.labels.len(), 4);
        assert_eq!(get_label_value(ts, "hostname"), Some("testhost"));
        assert_eq!(get_label_value(ts, LABEL_NAME), Some("agemon_test_metric"));
        assert_eq!(get_label_value(ts, "env"), Some("prod"));
        assert_eq!(get_label_value(ts, "region"), Some("us-east"));
        assert_eq!(ts.samples[0].value, 99.5);
    }

    #[test]
    fn test_record_with_empty_extra_labels() {
        let mut sink = MetricSink::new(1000, "host");
        sink.record_with("agemon_test", 1.0, &[]);
        let series = sink.into_series();
        assert_eq!(series[0].labels.len(), 2);
    }

    #[test]
    fn test_collect_metrics_returns_nonempty() {
        let mut sources = SystemSources::new().unwrap();
        let timeseries = collect_metrics(&mut sources, "testhost", 10);
        assert!(!timeseries.is_empty());
    }

    #[test]
    fn test_collect_metrics_all_have_hostname() {
        let mut sources = SystemSources::new().unwrap();
        let timeseries = collect_metrics(&mut sources, "testhost", 0);
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
        let mut sources = SystemSources::new().unwrap();
        let timeseries = collect_metrics(&mut sources, "testhost", 0);
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
        let mut sources = SystemSources::new().unwrap();
        let timeseries = collect_metrics(&mut sources, "testhost", 0);
        let process_metrics = find_metric(&timeseries, "agemon_process_cpu_usage_percent");
        assert!(process_metrics.is_empty());
    }
}
