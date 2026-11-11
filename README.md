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
| `agemon_disk_io_read_bytes_total` | counter | `device` | Bytes read from the block device since boot, from `/proc/diskstats` (Linux only; partitions are listed alongside their disk, so don't sum across `device`) |
| `agemon_disk_io_written_bytes_total` | counter | `device` | Bytes written to the block device since boot, from `/proc/diskstats` (Linux only) |
| `agemon_disk_io_read_bytes_per_sec` | gauge | - | Bytes read per second by all processes, averaged over the last collection interval |
| `agemon_disk_io_written_bytes_per_sec` | gauge | - | Bytes written per second by all processes, averaged over the last collection interval |

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

### Processes

Emitted only when `--top-processes` is greater than `0`. Processes are grouped by name.

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `agemon_process_count` | gauge | - | Number of processes (threads are not counted) |
| `agemon_process_cpu_usage_percent` | gauge | `process` | CPU usage of the top N process names (100 = one full core). The remaining processes are summed in `process="other"` |
| `agemon_process_memory_bytes` | gauge | `process` | Resident memory of the top N process names. The remaining processes are summed in `process="other"` |

### Linux only (`/proc`)

A metric is skipped when its `/proc` file is not available (for example, PSI on old kernels).

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `agemon_tcp_connections` | gauge | `state` | IPv4 TCP sockets by state (`established`, `listen`, `time_wait`, `close_wait`, `other`) |
| `agemon_tcp6_connections` | gauge | `state` | IPv6 TCP sockets by state |
| `agemon_file_descriptors_allocated` | gauge | - | Allocated file handles (`/proc/sys/fs/file-nr`) |
| `agemon_file_descriptors_max` | gauge | - | Maximum number of file handles |
| `agemon_context_switches_total` | counter | - | Context switches since boot |
| `agemon_processes_forked_total` | counter | - | Processes and threads created since boot |
| `agemon_procs_running` | gauge | - | Runnable processes |
| `agemon_procs_blocked` | gauge | - | Processes blocked on I/O |
| `agemon_psi_cpu_some_avg10`, `_avg60`, `_avg300` | gauge | - | CPU pressure stall averages (percent) |
| `agemon_psi_cpu_some_total_us` | counter | - | Total CPU stall time in microseconds |
| `agemon_psi_memory_{some,full}_avg10`, `_avg60`, `_avg300` | gauge | - | Memory pressure stall averages (percent) |
| `agemon_psi_memory_{some,full}_total_us` | counter | - | Total memory stall time in microseconds |
| `agemon_psi_io_{some,full}_avg10`, `_avg60`, `_avg300` | gauge | - | I/O pressure stall averages (percent) |
| `agemon_psi_io_{some,full}_total_us` | counter | - | Total I/O stall time in microseconds |
| `agemon_vmstat_pgfault_total`, `agemon_vmstat_pgmajfault_total` | counter | - | Page faults and major page faults |
| `agemon_vmstat_pgpgin_total`, `agemon_vmstat_pgpgout_total` | counter | - | Pages paged in and out |
| `agemon_vmstat_pswpin_total`, `agemon_vmstat_pswpout_total` | counter | - | Pages swapped in and out |
| `agemon_vmstat_oom_kill_total` | counter | - | OOM kills |
| `agemon_tcp_retrans_segs_total` | counter | - | TCP segments retransmitted |
| `agemon_tcp_in_segs_total`, `agemon_tcp_out_segs_total` | counter | - | TCP segments received and sent |
| `agemon_tcp_active_opens_total`, `agemon_tcp_passive_opens_total` | counter | - | TCP connections opened actively and passively |
| `agemon_tcp_curr_estab` | gauge | - | Currently established TCP connections |
| `agemon_udp_in_datagrams_total`, `agemon_udp_out_datagrams_total` | counter | - | UDP datagrams received and sent |
| `agemon_udp_in_errors_total` | counter | - | UDP receive errors |
| `agemon_entropy_available` | gauge | - | Available entropy in bits |

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
| `-i, --interval` | - | Interval between metric collections in seconds (minimum `1`) | `15` |
| `-r, --remote-write-url` | `AGEMON_REMOTE_WRITE_URL` | Prometheus remote write endpoint URL | `http://localhost:9090/api/v1/write` |
| `-u, --username` | `AGEMON_REMOTE_WRITE_USERNAME` | Username for Basic authentication (optional) | - |
| `-p, --password` | `AGEMON_REMOTE_WRITE_PASSWORD` | Password for Basic authentication (optional) | - |
| `--password-file` | `AGEMON_REMOTE_WRITE_PASSWORD_FILE` | File containing only the password (trailing newline ignored); conflicts with `--password` | - |
| `-t, --top-processes` | `AGEMON_TOP_PROCESSES` | Number of top processes to report by CPU and memory (`0` disables) | `10` |
| `--hostname` | `AGEMON_HOSTNAME` | Value of the `hostname` label | system hostname |
| `--max-queue-bytes` | `AGEMON_MAX_QUEUE_BYTES` | Maximum size of the in-memory queue for failed pushes, in bytes (`0` keeps only the newest block) | `8388608` |

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

### Failed pushes

Each collection is encoded as one block. AgeMon handles a failed push almost the same as
`vmagent`:

- A block that the remote storage did not receive (network error, timeout, `429`, `5xx` or
  other unexpected status) stays in an in-memory queue. AgeMon sends the queue again on
  a subsequent collection, oldest block first, so the samples arrive in timestamp order.
- After a failure, AgeMon waits before the next attempt. The delay starts at 1 second and
  doubles on each consecutive failure, to a maximum of 60 seconds. A `Retry-After` response
  header in seconds overrides the delay.
- A block that gets a `400`, `409` or `415` response is dropped, because a retry cannot
  succeed. The log shows the response body.
- When the queue is larger than `--max-queue-bytes`, AgeMon drops the oldest blocks.
- The queue is not written to disk. A restart loses the queued blocks.

The remote storage must accept old samples for late blocks to be stored.

### Logging

AgeMon logs at `info` level by default. Set `RUST_LOG` to change the level, for example
`RUST_LOG=agemon=debug` to see skipped `/proc` sources and push response statuses.

## Docker

On Linux, the flake builds a static (musl) Docker image:

```bash
nix build .#docker
docker load < result
```

A container sees only its own namespaces. To monitor the host, share the host namespaces
and set the hostname label:

```bash
docker run -d --name agemon \
  --pid=host --net=host --uts=host \
  -e AGEMON_REMOTE_WRITE_URL=https://prometheus.example.com/api/v1/write \
  -e AGEMON_HOSTNAME="$(hostname)" \
  agemon:latest
```

- `--pid=host` shows the host processes (process metrics).
- `--net=host` shows the host network interfaces, TCP/UDP connections and SNMP counters.
- `--uts=host` gives the host hostname. You can also set `AGEMON_HOSTNAME`.
- Disk metrics show the mounts that the container can see. Bind-mount the host
  filesystems (read-only) if you need them.

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
    passwordFile = "/run/secrets/agemon-password"; # raw secret, e.g. from sops-nix/agenix
    # hostname = "my-host"; # optional, default is the system hostname
    # topProcesses = 10; # optional, 0 disables the process metrics
    # maxQueueBytes = 8388608; # optional, in-memory queue limit for failed pushes
    # environmentFile = "/run/secrets/agemon-env"; # optional, KEY=value lines
  };
}
```

## Grafana Dashboard

Import `grafana-dashboard.json` into Grafana for a pre-built dashboard with:

- System overview (CPU, memory, swap, uptime, cores, process count)
- CPU usage and per-core breakdown
- Load averages (1m, 5m, 15m)
- Memory and swap usage over time
- Disk usage by mount point
- Disk I/O read/write rates
- Network receive/transmit rates, packets, and errors
- Top processes by CPU and memory
- Temperature sensors

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

### Tests and lints

```bash
cargo test
cargo clippy --all-targets -- -D warnings
```

`nix flake check` runs the build, the tests, clippy and the format check together.
