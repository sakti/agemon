use prometheus_remote_write::TimeSeries;
use sysinfo::Networks;

use super::create_timeseries_with_labels;

pub fn collect(
    networks: &Networks,
    timestamp: i64,
    hostname: &str,
    timeseries: &mut Vec<TimeSeries>,
) {
    for (interface_name, data) in networks.list() {
        let labels = vec![("interface", interface_name.as_str())];

        timeseries.push(create_timeseries_with_labels(
            "agemon_network_received_bytes_total",
            data.total_received() as f64,
            timestamp,
            hostname,
            labels.clone(),
        ));

        timeseries.push(create_timeseries_with_labels(
            "agemon_network_transmitted_bytes_total",
            data.total_transmitted() as f64,
            timestamp,
            hostname,
            labels.clone(),
        ));

        timeseries.push(create_timeseries_with_labels(
            "agemon_network_received_packets_total",
            data.total_packets_received() as f64,
            timestamp,
            hostname,
            labels.clone(),
        ));

        timeseries.push(create_timeseries_with_labels(
            "agemon_network_transmitted_packets_total",
            data.total_packets_transmitted() as f64,
            timestamp,
            hostname,
            labels.clone(),
        ));

        timeseries.push(create_timeseries_with_labels(
            "agemon_network_received_errors_total",
            data.total_errors_on_received() as f64,
            timestamp,
            hostname,
            labels.clone(),
        ));

        timeseries.push(create_timeseries_with_labels(
            "agemon_network_transmitted_errors_total",
            data.total_errors_on_transmitted() as f64,
            timestamp,
            hostname,
            labels,
        ));
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
        let mut timeseries = vec![];
        collect(&networks, 1000, "testhost", &mut timeseries);

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
        let mut timeseries = vec![];
        collect(&networks, 1000, "testhost", &mut timeseries);

        let interface_count = networks.list().len();
        if interface_count > 0 {
            assert_eq!(timeseries.len(), interface_count * 6);
        }
    }
}
