use prometheus_remote_write::TimeSeries;
use sysinfo::Components;

use super::create_timeseries_with_labels;

pub fn collect(
    components: &Components,
    timestamp: i64,
    hostname: &str,
    timeseries: &mut Vec<TimeSeries>,
) {
    for component in components.list() {
        let sensor = component.label();
        let labels = vec![("sensor", sensor)];

        if let Some(temp) = component.temperature() {
            timeseries.push(create_timeseries_with_labels(
                "agemon_temperature_celsius",
                temp as f64,
                timestamp,
                hostname,
                labels.clone(),
            ));
        }

        if let Some(max) = component.max() {
            timeseries.push(create_timeseries_with_labels(
                "agemon_temperature_max_celsius",
                max as f64,
                timestamp,
                hostname,
                labels.clone(),
            ));
        }

        if let Some(critical) = component.critical() {
            timeseries.push(create_timeseries_with_labels(
                "agemon_temperature_critical_celsius",
                critical as f64,
                timestamp,
                hostname,
                labels,
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use sysinfo::Components;

    use super::*;
    use crate::metrics::get_label_value;

    #[test]
    fn test_temperature_metrics_have_sensor_label() {
        let components = Components::new_with_refreshed_list();
        let mut timeseries = vec![];
        collect(&components, 1000, "testhost", &mut timeseries);

        for ts in &timeseries {
            assert!(
                get_label_value(ts, "sensor").is_some(),
                "temperature metric missing sensor label"
            );
        }
    }
}
