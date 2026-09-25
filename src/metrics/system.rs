use sysinfo::System;

use super::MetricSink;

pub fn collect(sink: &mut MetricSink) {
    sink.record("agemon_system_uptime_seconds", System::uptime() as f64);
    sink.record(
        "agemon_system_boot_time_seconds",
        System::boot_time() as f64,
    );

    let load_avg = System::load_average();
    sink.record("agemon_load_average_1m", load_avg.one);
    sink.record("agemon_load_average_5m", load_avg.five);
    sink.record("agemon_load_average_15m", load_avg.fifteen);

    let os_name = System::name().unwrap_or_else(|| "unknown".to_string());
    let os_version = System::os_version().unwrap_or_else(|| "unknown".to_string());
    let kernel_version = System::kernel_version().unwrap_or_else(|| "unknown".to_string());
    let arch = System::cpu_arch();

    sink.record_with(
        "agemon_info",
        1.0,
        &[
            ("os_name", os_name.as_str()),
            ("os_version", os_version.as_str()),
            ("kernel_version", kernel_version.as_str()),
            ("arch", arch.as_str()),
        ],
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::{find_metric, get_label_value};

    #[test]
    fn test_collect_produces_system_metrics() {
        let mut sink = MetricSink::new(1000, "testhost");
        collect(&mut sink);
        let timeseries = sink.into_series();

        let expected = [
            "agemon_system_uptime_seconds",
            "agemon_system_boot_time_seconds",
            "agemon_load_average_1m",
            "agemon_load_average_5m",
            "agemon_load_average_15m",
            "agemon_info",
        ];
        for name in expected {
            let found = find_metric(&timeseries, name);
            assert_eq!(found.len(), 1, "expected exactly one {name}");
        }
    }

    #[test]
    fn test_info_metric_has_system_labels() {
        let mut sink = MetricSink::new(1000, "testhost");
        collect(&mut sink);
        let timeseries = sink.into_series();

        let info = find_metric(&timeseries, "agemon_info");
        let ts = info[0];
        assert_eq!(ts.samples[0].value, 1.0);
        assert!(get_label_value(ts, "os_name").is_some());
        assert!(get_label_value(ts, "arch").is_some());
        assert!(get_label_value(ts, "kernel_version").is_some());
    }

    #[test]
    fn test_uptime_is_positive() {
        let mut sink = MetricSink::new(1000, "testhost");
        collect(&mut sink);
        let timeseries = sink.into_series();

        let uptime = find_metric(&timeseries, "agemon_system_uptime_seconds");
        assert!(uptime[0].samples[0].value > 0.0);
    }
}
