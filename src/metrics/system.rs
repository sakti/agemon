use prometheus_remote_write::TimeSeries;
use sysinfo::System;

use super::{create_timeseries, create_timeseries_with_labels};

pub fn collect(timestamp: i64, hostname: &str, timeseries: &mut Vec<TimeSeries>) {
    timeseries.push(create_timeseries(
        "agemon_system_uptime_seconds",
        System::uptime() as f64,
        timestamp,
        hostname,
    ));

    timeseries.push(create_timeseries(
        "agemon_system_boot_time_seconds",
        System::boot_time() as f64,
        timestamp,
        hostname,
    ));

    let load_avg = System::load_average();
    timeseries.push(create_timeseries(
        "agemon_load_average_1m",
        load_avg.one,
        timestamp,
        hostname,
    ));

    timeseries.push(create_timeseries(
        "agemon_load_average_5m",
        load_avg.five,
        timestamp,
        hostname,
    ));

    timeseries.push(create_timeseries(
        "agemon_load_average_15m",
        load_avg.fifteen,
        timestamp,
        hostname,
    ));

    let os_name = System::name().unwrap_or_else(|| "unknown".to_string());
    let os_version = System::os_version().unwrap_or_else(|| "unknown".to_string());
    let kernel_version = System::kernel_version().unwrap_or_else(|| "unknown".to_string());
    let arch = System::cpu_arch();

    let info_labels = vec![
        ("os_name", os_name.as_str()),
        ("os_version", os_version.as_str()),
        ("kernel_version", kernel_version.as_str()),
        ("arch", arch.as_str()),
    ];
    timeseries.push(create_timeseries_with_labels(
        "agemon_info",
        1.0,
        timestamp,
        hostname,
        info_labels,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::{find_metric, get_label_value};

    #[test]
    fn test_collect_produces_system_metrics() {
        let mut timeseries = vec![];
        collect(1000, "testhost", &mut timeseries);

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
        let mut timeseries = vec![];
        collect(1000, "testhost", &mut timeseries);

        let info = find_metric(&timeseries, "agemon_info");
        let ts = info[0];
        assert_eq!(ts.samples[0].value, 1.0);
        assert!(get_label_value(ts, "os_name").is_some());
        assert!(get_label_value(ts, "arch").is_some());
        assert!(get_label_value(ts, "kernel_version").is_some());
    }

    #[test]
    fn test_uptime_is_positive() {
        let mut timeseries = vec![];
        collect(1000, "testhost", &mut timeseries);

        let uptime = find_metric(&timeseries, "agemon_system_uptime_seconds");
        assert!(uptime[0].samples[0].value > 0.0);
    }
}
