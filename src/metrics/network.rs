use sysinfo::Networks;

use super::MetricSink;

pub fn collect(networks: &Networks, sink: &mut MetricSink) {
    for (interface_name, data) in networks.list() {
        let labels = [("interface", interface_name.as_str())];

        sink.record_with(
            "agemon_network_received_bytes_total",
            data.total_received() as f64,
            &labels,
        );
        sink.record_with(
            "agemon_network_transmitted_bytes_total",
            data.total_transmitted() as f64,
            &labels,
        );
        sink.record_with(
            "agemon_network_received_packets_total",
            data.total_packets_received() as f64,
            &labels,
        );
        sink.record_with(
            "agemon_network_transmitted_packets_total",
            data.total_packets_transmitted() as f64,
            &labels,
        );
        sink.record_with(
            "agemon_network_received_errors_total",
            data.total_errors_on_received() as f64,
            &labels,
        );
        sink.record_with(
            "agemon_network_transmitted_errors_total",
            data.total_errors_on_transmitted() as f64,
            &labels,
        );
    }
}

#[cfg(test)]
mod tests {
    use sysinfo::Networks;

    use super::*;
    use crate::metrics::get_label_value;

    #[test]
    fn test_collect_network_metrics_have_interface_label() {
        let networks = Networks::new_with_refreshed_list();
        let mut sink = MetricSink::new(1000, "testhost");
        collect(&networks, &mut sink);
        let timeseries = sink.into_series();

        // Every network metric should have an interface label
        for ts in &timeseries {
            assert!(
                get_label_value(ts, "interface").is_some(),
                "network metric missing interface label"
            );
        }
    }

    #[test]
    fn test_six_metrics_per_interface() {
        let networks = Networks::new_with_refreshed_list();
        let mut sink = MetricSink::new(1000, "testhost");
        collect(&networks, &mut sink);
        let timeseries = sink.into_series();

        let interface_count = networks.list().len();
        if interface_count > 0 {
            assert_eq!(timeseries.len(), interface_count * 6);
        }
    }
}
