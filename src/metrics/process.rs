use std::collections::HashMap;

use sysinfo::System;

use super::MetricSink;

pub fn collect(sys: &System, top_n: usize, sink: &mut MetricSink) {
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

    sink.record("agemon_process_count", sys.processes().len() as f64);

    // Top N by CPU
    let mut cpu_sorted: Vec<_> = cpu_by_name.into_iter().collect();
    cpu_sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let mut other_cpu = 0.0;
    for (i, (name, cpu)) in cpu_sorted.iter().enumerate() {
        if i < top_n {
            sink.record_with(
                "agemon_process_cpu_usage_percent",
                *cpu,
                &[("process", name.as_str())],
            );
        } else {
            other_cpu += cpu;
        }
    }
    if cpu_sorted.len() > top_n {
        sink.record_with(
            "agemon_process_cpu_usage_percent",
            other_cpu,
            &[("process", "other")],
        );
    }

    // Top N by memory
    let mut mem_sorted: Vec<_> = mem_by_name.into_iter().collect();
    mem_sorted.sort_by_key(|(_, mem)| std::cmp::Reverse(*mem));

    let mut other_mem: u64 = 0;
    for (i, (name, mem)) in mem_sorted.iter().enumerate() {
        if i < top_n {
            sink.record_with(
                "agemon_process_memory_bytes",
                *mem as f64,
                &[("process", name.as_str())],
            );
        } else {
            other_mem += mem;
        }
    }
    if mem_sorted.len() > top_n {
        sink.record_with(
            "agemon_process_memory_bytes",
            other_mem as f64,
            &[("process", "other")],
        );
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
        let mut sink = MetricSink::new(1000, "testhost");
        collect(&sys, 5, &mut sink);
        let timeseries = sink.into_series();

        let count = find_metric(&timeseries, "agemon_process_count");
        assert_eq!(count.len(), 1);
        assert!(count[0].samples[0].value > 0.0);
    }

    #[test]
    fn test_top_n_limits_cpu_metrics() {
        let sys = make_sys();
        let mut sink = MetricSink::new(1000, "testhost");
        collect(&sys, 3, &mut sink);
        let timeseries = sink.into_series();

        let cpu_metrics = find_metric(&timeseries, "agemon_process_cpu_usage_percent");
        // At most top_n + 1 (the "other" bucket)
        assert!(
            cpu_metrics.len() <= 4,
            "got {} cpu metrics",
            cpu_metrics.len()
        );
    }

    #[test]
    fn test_top_n_limits_memory_metrics() {
        let sys = make_sys();
        let mut sink = MetricSink::new(1000, "testhost");
        collect(&sys, 3, &mut sink);
        let timeseries = sink.into_series();

        let mem_metrics = find_metric(&timeseries, "agemon_process_memory_bytes");
        assert!(
            mem_metrics.len() <= 4,
            "got {} mem metrics",
            mem_metrics.len()
        );
    }

    #[test]
    fn test_process_metrics_have_process_label() {
        let sys = make_sys();
        let mut sink = MetricSink::new(1000, "testhost");
        collect(&sys, 5, &mut sink);
        let timeseries = sink.into_series();

        let cpu_metrics = find_metric(&timeseries, "agemon_process_cpu_usage_percent");
        for ts in cpu_metrics {
            assert!(get_label_value(ts, "process").is_some());
        }
    }
}
