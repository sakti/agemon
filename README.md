# AgeMon

A lightweight agent that collects system metrics and pushes them to Prometheus via remote write.

## Metrics

- **CPU**: `agemon_cpu_usage_percent`
- **Memory**: `agemon_memory_total_bytes`, `agemon_memory_used_bytes`, `agemon_swap_total_bytes`, `agemon_swap_used_bytes`
- **Disk**: `agemon_disk_total_bytes`, `agemon_disk_available_bytes` (with `mount_point` label)
- **Network**: `agemon_network_received_bytes`, `agemon_network_transmitted_bytes` (with `interface` label)

## Installation

```bash
cargo build --release
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
