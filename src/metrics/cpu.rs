use sysinfo::System;

use super::MetricSink;

pub fn collect(sys: &System, sink: &mut MetricSink) {
    sink.record("agemon_cpu_usage_percent", sys.global_cpu_usage() as f64);
    sink.record("agemon_cpu_count", sys.cpus().len() as f64);

    for cpu in sys.cpus() {
        sink.record_with(
            "agemon_cpu_core_usage_percent",
            cpu.usage() as f64,
            &[("cpu", cpu.name())],
        );
    }
}

#[cfg(test)]
mod tests {
    use sysinfo::System;

    use super::*;
    use crate::metrics::{find_metric, get_label_value};

    #[test]
    fn test_collect_produces_cpu_metrics() {
        let sys = System::new_all().unwrap();
        let mut sink = MetricSink::new(1000, "testhost");
        collect(&sys, &mut sink);
        let timeseries = sink.into_series();

        let usage = find_metric(&timeseries, "agemon_cpu_usage_percent");
        assert_eq!(usage.len(), 1);

        let count = find_metric(&timeseries, "agemon_cpu_count");
        assert_eq!(count.len(), 1);
        assert!(count[0].samples[0].value >= 1.0);
    }

    #[test]
    fn test_per_core_metrics_have_cpu_label() {
        let sys = System::new_all().unwrap();
        let mut sink = MetricSink::new(1000, "testhost");
        collect(&sys, &mut sink);
        let timeseries = sink.into_series();

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
