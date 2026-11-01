use sysinfo::Components;

use super::MetricSink;

pub fn collect(components: &Components, sink: &mut MetricSink) {
    for component in components {
        let labels = [("sensor", component.label())];

        if let Some(temp) = component.temperature() {
            sink.record_with("agemon_temperature_celsius", temp as f64, &labels);
        }

        if let Some(max) = component.max() {
            sink.record_with("agemon_temperature_max_celsius", max as f64, &labels);
        }

        if let Some(critical) = component.critical() {
            sink.record_with(
                "agemon_temperature_critical_celsius",
                critical as f64,
                &labels,
            );
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
        let components = Components::new_with_refreshed_list().unwrap();
        let mut sink = MetricSink::new(1000, "testhost");
        collect(&components, &mut sink);
        let timeseries = sink.into_series();

        for ts in &timeseries {
            assert!(
                get_label_value(ts, "sensor").is_some(),
                "temperature metric missing sensor label"
            );
        }
    }
}
