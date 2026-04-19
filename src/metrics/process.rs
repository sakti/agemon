use std::collections::HashMap;

use prometheus_remote_write::TimeSeries;
use sysinfo::System;

use super::{create_timeseries, create_timeseries_with_labels};

pub fn collect(
    sys: &System,
    timestamp: i64,
    hostname: &str,
    top_n: usize,
    timeseries: &mut Vec<TimeSeries>,
) {
    let mut cpu_by_name: HashMap<String, f64> = HashMap::new();
    let mut mem_by_name: HashMap<String, u64> = HashMap::new();

    for process in sys.processes().values() {
        let name = process.name().to_string_lossy().into_owned();
        *cpu_by_name.entry(name.clone()).or_default() += process.cpu_usage() as f64;
        // Only count memory for non-thread processes to avoid double-counting RSS
        // (threads share address space, so each thread reports the same RSS as its parent)
        if process.thread_kind().is_none() {
            *mem_by_name.entry(name).or_default() += process.memory();
        }
    }

    timeseries.push(create_timeseries(
        "agemon_process_count",
        sys.processes().len() as f64,
        timestamp,
        hostname,
    ));

    // Top N by CPU
    let mut cpu_sorted: Vec<_> = cpu_by_name.into_iter().collect();
    cpu_sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let mut other_cpu = 0.0;
    for (i, (name, cpu)) in cpu_sorted.iter().enumerate() {
        if i < top_n {
            timeseries.push(create_timeseries_with_labels(
                "agemon_process_cpu_usage_percent",
                *cpu,
                timestamp,
                hostname,
                vec![("process", name.as_str())],
            ));
        } else {
            other_cpu += cpu;
        }
    }
    if cpu_sorted.len() > top_n {
        timeseries.push(create_timeseries_with_labels(
            "agemon_process_cpu_usage_percent",
            other_cpu,
            timestamp,
            hostname,
            vec![("process", "other")],
        ));
    }

    // Top N by memory
    let mut mem_sorted: Vec<_> = mem_by_name.into_iter().collect();
    mem_sorted.sort_by(|a, b| b.1.cmp(&a.1));

    let mut other_mem: u64 = 0;
    for (i, (name, mem)) in mem_sorted.iter().enumerate() {
        if i < top_n {
            timeseries.push(create_timeseries_with_labels(
                "agemon_process_memory_bytes",
                *mem as f64,
                timestamp,
                hostname,
                vec![("process", name.as_str())],
            ));
        } else {
            other_mem += mem;
        }
    }
    if mem_sorted.len() > top_n {
        timeseries.push(create_timeseries_with_labels(
            "agemon_process_memory_bytes",
            other_mem as f64,
            timestamp,
            hostname,
            vec![("process", "other")],
        ));
    }
}

#[cfg(test)]
mod tests {
    use sysinfo::{ProcessRefreshKind, System};

    use super::*;
    use crate::metrics::{find_metric, get_label_value};

    fn make_sys() -> System {
        let mut sys = System::new_all();
        sys.refresh_processes_specifics(
            sysinfo::ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing().with_cpu().with_memory(),
        );
        sys
    }

    #[test]
    fn test_collect_produces_process_count() {
        let sys = make_sys();
        let mut timeseries = vec![];
        collect(&sys, 1000, "testhost", 5, &mut timeseries);

        let count = find_metric(&timeseries, "agemon_process_count");
        assert_eq!(count.len(), 1);
        assert!(count[0].samples[0].value > 0.0);
    }

    #[test]
    fn test_top_n_limits_cpu_metrics() {
        let sys = make_sys();
        let mut timeseries = vec![];
        collect(&sys, 1000, "testhost", 3, &mut timeseries);

        let cpu_metrics = find_metric(&timeseries, "agemon_process_cpu_usage_percent");
        // At most top_n + 1 (the "other" bucket)
        assert!(cpu_metrics.len() <= 4, "got {} cpu metrics", cpu_metrics.len());
    }

    #[test]
    fn test_top_n_limits_memory_metrics() {
        let sys = make_sys();
        let mut timeseries = vec![];
        collect(&sys, 1000, "testhost", 3, &mut timeseries);

        let mem_metrics = find_metric(&timeseries, "agemon_process_memory_bytes");
        assert!(mem_metrics.len() <= 4, "got {} mem metrics", mem_metrics.len());
    }

    #[test]
    fn test_process_metrics_have_process_label() {
        let sys = make_sys();
        let mut timeseries = vec![];
        collect(&sys, 1000, "testhost", 5, &mut timeseries);

        let cpu_metrics = find_metric(&timeseries, "agemon_process_cpu_usage_percent");
        for ts in cpu_metrics {
            assert!(get_label_value(ts, "process").is_some());
        }
    }
}
