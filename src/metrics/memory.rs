use sysinfo::System;

use super::MetricSink;

pub fn collect(sys: &System, sink: &mut MetricSink) {
    sink.record("agemon_memory_total_bytes", sys.total_memory() as f64);
    sink.record("agemon_memory_used_bytes", sys.used_memory() as f64);
    sink.record("agemon_memory_free_bytes", sys.free_memory() as f64);
    sink.record(
        "agemon_memory_available_bytes",
        sys.available_memory() as f64,
    );

    let usage_ratio = if sys.total_memory() > 0 {
        sys.used_memory() as f64 / sys.total_memory() as f64
    } else {
        0.0
    };
    sink.record("agemon_memory_usage_ratio", usage_ratio);

    sink.record("agemon_swap_total_bytes", sys.total_swap() as f64);
    sink.record("agemon_swap_used_bytes", sys.used_swap() as f64);
    sink.record("agemon_swap_free_bytes", sys.free_swap() as f64);

    let swap_ratio = if sys.total_swap() > 0 {
        sys.used_swap() as f64 / sys.total_swap() as f64
    } else {
        0.0
    };
    sink.record("agemon_swap_usage_ratio", swap_ratio);
}

#[cfg(test)]
mod tests {
    use sysinfo::{MemoryRefreshKind, System};

    use super::*;
    use crate::metrics::find_metric;

    #[test]
    fn test_collect_produces_all_memory_metrics() {
        let mut sys = System::new();
        sys.refresh_memory_specifics(MemoryRefreshKind::everything());
        let mut sink = MetricSink::new(1000, "testhost");
        collect(&sys, &mut sink);
        let timeseries = sink.into_series();

        let expected = [
            "agemon_memory_total_bytes",
            "agemon_memory_used_bytes",
            "agemon_memory_free_bytes",
            "agemon_memory_available_bytes",
            "agemon_memory_usage_ratio",
            "agemon_swap_total_bytes",
            "agemon_swap_used_bytes",
            "agemon_swap_free_bytes",
            "agemon_swap_usage_ratio",
        ];
        for name in expected {
            let found = find_metric(&timeseries, name);
            assert_eq!(found.len(), 1, "expected exactly one {name}");
        }
    }

    #[test]
    fn test_memory_usage_ratio_bounded() {
        let mut sys = System::new();
        sys.refresh_memory_specifics(MemoryRefreshKind::everything());
        let mut sink = MetricSink::new(1000, "testhost");
        collect(&sys, &mut sink);
        let timeseries = sink.into_series();

        let ratio = find_metric(&timeseries, "agemon_memory_usage_ratio");
        let value = ratio[0].samples[0].value;
        assert!((0.0..=1.0).contains(&value), "ratio {value} out of range");
    }
}
