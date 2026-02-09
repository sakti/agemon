# AgeMon

A lightweight agent that collects system metrics and pushes them to Prometheus via remote write.

## Metrics

### CPU

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `agemon_cpu_usage_percent` | gauge | - | Global CPU usage percentage (0-100) |
| `agemon_cpu_count` | gauge | - | Number of logical CPU cores |
| `agemon_cpu_core_usage_percent` | gauge | `cpu` | Per-core CPU usage percentage |

### Memory

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `agemon_memory_total_bytes` | gauge | - | Total physical memory in bytes |
| `agemon_memory_used_bytes` | gauge | - | Used physical memory in bytes |
| `agemon_memory_free_bytes` | gauge | - | Free physical memory in bytes |
| `agemon_memory_available_bytes` | gauge | - | Available memory (includes cached/buffered) |
| `agemon_memory_usage_ratio` | gauge | - | Memory usage ratio (0.0-1.0) |
| `agemon_swap_total_bytes` | gauge | - | Total swap space in bytes |
| `agemon_swap_used_bytes` | gauge | - | Used swap space in bytes |
| `agemon_swap_free_bytes` | gauge | - | Free swap space in bytes |
| `agemon_swap_usage_ratio` | gauge | - | Swap usage ratio (0.0-1.0) |

### Disk

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `agemon_disk_total_bytes` | gauge | `mount_point`, `device`, `fs_type` | Total disk space in bytes |
| `agemon_disk_available_bytes` | gauge | `mount_point`, `device`, `fs_type` | Available disk space in bytes |
| `agemon_disk_used_bytes` | gauge | `mount_point`, `device`, `fs_type` | Used disk space in bytes |
| `agemon_disk_usage_ratio` | gauge | `mount_point`, `device`, `fs_type` | Disk usage ratio (0.0-1.0) |
| `agemon_disk_is_removable` | gauge | `mount_point`, `device`, `fs_type` | Whether disk is removable (1=yes, 0=no) |

### Disk I/O

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `agemon_disk_io_read_bytes_total` | counter | - | Total bytes read from disk (aggregated from all processes) |
| `agemon_disk_io_written_bytes_total` | counter | - | Total bytes written to disk (aggregated from all processes) |
| `agemon_disk_io_read_bytes_per_sec` | gauge | - | Bytes read per second since last refresh |
| `agemon_disk_io_written_bytes_per_sec` | gauge | - | Bytes written per second since last refresh |

### Network

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `agemon_network_received_bytes_total` | counter | `interface` | Total bytes received on interface |
| `agemon_network_transmitted_bytes_total` | counter | `interface` | Total bytes transmitted on interface |
| `agemon_network_received_packets_total` | counter | `interface` | Total packets received on interface |
| `agemon_network_transmitted_packets_total` | counter | `interface` | Total packets transmitted on interface |
| `agemon_network_received_errors_total` | counter | `interface` | Total receive errors on interface |
| `agemon_network_transmitted_errors_total` | counter | `interface` | Total transmit errors on interface |

### Temperature

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `agemon_temperature_celsius` | gauge | `sensor` | Current temperature of the sensor |
| `agemon_temperature_max_celsius` | gauge | `sensor` | Maximum observed temperature of the sensor |
| `agemon_temperature_critical_celsius` | gauge | `sensor` | Critical threshold temperature (only emitted if available) |

### System

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `agemon_system_uptime_seconds` | gauge | - | System uptime in seconds |
| `agemon_system_boot_time_seconds` | gauge | - | System boot time as Unix timestamp |
| `agemon_load_average_1m` | gauge | - | 1-minute load average |
| `agemon_load_average_5m` | gauge | - | 5-minute load average |
| `agemon_load_average_15m` | gauge | - | 15-minute load average |
| `agemon_info` | gauge | `os_name`, `os_version`, `kernel_version`, `arch` | System information (always 1) |

All metrics include a `hostname` label.

## Installation

### Using Cargo

```bash
cargo build --release
```

### Using Nix

```bash
nix build
```

Or run directly:

```bash
nix run
```

## Usage

```bash
agemon [OPTIONS]
```

### Options

| Option | Environment Variable | Description | Default |
|--------|---------------------|-------------|---------|
| `-i, --interval` | - | Interval between metric collections in seconds | `15` |
| `-r, --remote-write-url` | `AGEMON_REMOTE_WRITE_URL` | Prometheus remote write endpoint URL | `http://localhost:9090/api/v1/write` |
| `-u, --username` | `AGEMON_REMOTE_WRITE_USERNAME` | Username for Basic authentication (optional) | - |
| `-p, --password` | `AGEMON_REMOTE_WRITE_PASSWORD` | Password for Basic authentication (optional) | - |

### Examples

Push metrics to local Prometheus every 15 seconds (no auth):

```bash
agemon
```

Push metrics with authentication:

```bash
agemon -u myuser -p mypass -r https://prometheus.example.com/api/v1/write
```

Using environment variables:

```bash
export AGEMON_REMOTE_WRITE_URL=https://prometheus.example.com/api/v1/write
export AGEMON_REMOTE_WRITE_USERNAME=myuser
export AGEMON_REMOTE_WRITE_PASSWORD=mypass
agemon -i 30
```

## Home Manager Module

Add to your flake inputs:

```nix
{
  inputs.agemon.url = "github:sakti/agemon";
}
```

Then enable the service:

```nix
{
  imports = [inputs.agemon.homeManagerModules.default];

  services.agemon = {
    enable = true;
    interval = 15;
    remoteWriteUrl = "https://prometheus.example.com/api/v1/write";
    username = "myuser";
    passwordFile = "/run/secrets/agemon-password";
  };
}
```

## Grafana Dashboard

Import `grafana-dashboard.json` into Grafana for a pre-built dashboard with:

- System overview (CPU, memory, swap, uptime, cores)
- CPU usage and per-core breakdown
- Load averages (1m, 5m, 15m)
- Memory and swap usage over time
- Disk usage by mount point
- Disk I/O read/write rates
- Network receive/transmit rates, packets, and errors

The dashboard includes variables for datasource and hostname filtering.

## Development

Enter the development shell:

```bash
nix develop
```

### Formatting

Format code using rustfmt:

```bash
cargo fmt
```

Check formatting without applying changes:

```bash
cargo fmt --check
```
