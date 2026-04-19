use prometheus_remote_write::TimeSeries;
use sysinfo::System;

use super::{create_timeseries, create_timeseries_with_labels};

pub fn collect(sys: &System, timestamp: i64, hostname: &str, timeseries: &mut Vec<TimeSeries>) {
    timeseries.push(create_timeseries(
        "agemon_cpu_usage_percent",
        sys.global_cpu_usage() as f64,
        timestamp,
        hostname,
    ));

    timeseries.push(create_timeseries(
        "agemon_cpu_count",
        sys.cpus().len() as f64,
        timestamp,
        hostname,
    ));

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

#[cfg(test)]
mod tests {
    use sysinfo::System;

    use super::*;
    use crate::metrics::{find_metric, get_label_value};

    #[test]
    fn test_collect_produces_cpu_metrics() {
        let sys = System::new_all();
        let mut timeseries = vec![];
        collect(&sys, 1000, "testhost", &mut timeseries);

        let usage = find_metric(&timeseries, "agemon_cpu_usage_percent");
        assert_eq!(usage.len(), 1);

        let count = find_metric(&timeseries, "agemon_cpu_count");
        assert_eq!(count.len(), 1);
        assert!(count[0].samples[0].value >= 1.0);
    }

    #[test]
    fn test_per_core_metrics_have_cpu_label() {
        let sys = System::new_all();
        let mut timeseries = vec![];
        collect(&sys, 1000, "testhost", &mut timeseries);

        let per_core = find_metric(&timeseries, "agemon_cpu_core_usage_percent");
        assert!(!per_core.is_empty());
        for ts in per_core {
            assert!(
                get_label_value(ts, "cpu").is_some(),
                "per-core metric missing cpu label"
            );
        }
    }
}
