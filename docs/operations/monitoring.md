# Monitoring

This guide covers monitoring SquirrelDB in production.

## Health Endpoints

SquirrelDB provides health check endpoints on the admin port (default 8081):

### Liveness Probe

```
GET /health
```

Returns `200 OK` if the server process is running. Use this for:
- Kubernetes liveness probes
- Load balancer health checks
- Basic uptime monitoring

### Readiness Probe

```
GET /ready
```

Returns:
- `200 OK` if the database is accessible
- `503 Service Unavailable` if the database connection fails

Use this for:
- Kubernetes readiness probes
- Traffic routing decisions
- Database connectivity monitoring

## Logging

SquirrelDB logs to stdout/stderr using structured logging.

### Log Levels

Configure via `logging.level` in config or `--log-level` CLI flag:

| Level | Description |
|-------|-------------|
| `error` | Errors only |
| `warn` | Warnings and errors |
| `info` | General operational info (default) |
| `debug` | Detailed debugging info |

### Log Format

```
2024-01-15T10:30:00Z INFO squirreldb: Admin UI at http://0.0.0.0:8081
2024-01-15T10:30:00Z INFO squirreldb: SquirrelDB WebSocket on 0.0.0.0:8080
2024-01-15T10:30:05Z DEBUG squirreldb: Client connected: abc123
2024-01-15T10:30:05Z DEBUG squirreldb: Query: db.table("users").run()
```

### Viewing Logs

```bash
# Docker
docker logs -f squirreldb

# Docker Compose
docker-compose logs -f squirreldb

# Kubernetes
kubectl logs -f deployment/squirreldb

# Systemd
journalctl -u squirreldb -f
```

### Log Aggregation

Send logs to your preferred aggregation system:

#### Fluentd (Kubernetes)

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: fluentd-config
data:
  fluent.conf: |
    <source>
      @type tail
      path /var/log/containers/squirreldb*.log
      tag squirreldb
      <parse>
        @type json
      </parse>
    </source>

    <match squirreldb>
      @type elasticsearch
      host elasticsearch
      port 9200
      index_name squirreldb-logs
    </match>
```

#### Loki (Grafana)

```yaml
# promtail config
scrape_configs:
  - job_name: squirreldb
    static_configs:
      - targets:
          - localhost
        labels:
          job: squirreldb
          __path__: /var/log/squirreldb/*.log
```

## Metrics

### Key Metrics to Monitor

| Metric | Description | Alert Threshold |
|--------|-------------|-----------------|
| Active connections | WebSocket client count | > 80% of max |
| Query latency | Time to execute queries | p99 > 500ms |
| Error rate | Failed queries / total | > 1% |
| Database pool usage | Active connections / pool size | > 80% |
| Memory usage | Process memory | > 80% of limit |
| CPU usage | Process CPU | > 80% sustained |

### Status Endpoint

```
GET /api/status
```

Returns:

```json
{
  "name": "SquirrelDB",
  "version": "0.0.1",
  "backend": "Postgres",
  "uptime_secs": 86400
}
```

### Custom Monitoring

Query the status endpoint periodically:

```bash
#!/bin/bash
# monitor.sh
while true; do
  STATUS=$(curl -s http://localhost:8081/api/status)
  UPTIME=$(echo $STATUS | jq '.uptime_secs')
  echo "$(date): Uptime ${UPTIME}s"
  sleep 60
done
```

## Alerting

### Critical Alerts

Set up alerts for:

1. **Health check failures**
   - Liveness probe returns non-200
   - Readiness probe returns non-200

2. **High error rate**
   - Query errors > 1% of total
   - Connection failures

3. **Resource exhaustion**
   - Memory > 90%
   - Database connections exhausted

### Warning Alerts

1. **Performance degradation**
   - Query latency p99 > 500ms
   - Connection pool > 70%

2. **Approaching limits**
   - Memory > 70%
   - Connections > 80% of max

### PagerDuty Integration

```yaml
# Example alerting rule
- alert: SquirrelDBDown
  expr: up{job="squirreldb"} == 0
  for: 1m
  labels:
    severity: critical
  annotations:
    summary: "SquirrelDB is down"
    description: "SquirrelDB has been down for more than 1 minute"
```

## Dashboard Examples

### Grafana Dashboard

Create a Grafana dashboard with:

1. **Overview panel**
   - Current status (up/down)
   - Uptime
   - Backend type

2. **Connections panel**
   - Active connections over time
   - Connection rate

3. **Performance panel**
   - Query latency histogram
   - Queries per second

4. **Resources panel**
   - Memory usage
   - CPU usage
   - Database pool usage

### Simple Status Page

```html
<!DOCTYPE html>
<html>
<head>
  <title>SquirrelDB Status</title>
  <script>
    async function checkStatus() {
      try {
        const res = await fetch('/api/status');
        const data = await res.json();
        document.getElementById('status').textContent = 'Online';
        document.getElementById('status').style.color = 'green';
        document.getElementById('uptime').textContent =
          Math.floor(data.uptime_secs / 3600) + ' hours';
      } catch (e) {
        document.getElementById('status').textContent = 'Offline';
        document.getElementById('status').style.color = 'red';
      }
    }
    setInterval(checkStatus, 30000);
    checkStatus();
  </script>
</head>
<body>
  <h1>SquirrelDB Status</h1>
  <p>Status: <span id="status">Checking...</span></p>
  <p>Uptime: <span id="uptime">-</span></p>
</body>
</html>
```

## Troubleshooting with Logs

### Connection Issues

Look for:
```
ERROR squirreldb: Database connection failed: ...
ERROR squirreldb: WebSocket error: ...
```

### Query Performance

Enable debug logging:
```bash
sqrld --log-level debug
```

Look for slow queries:
```
DEBUG squirreldb: Query took 1523ms: db.table("large_collection").run()
```

### Memory Issues

Monitor process memory:
```bash
# Linux
ps aux | grep sqrld

# Docker
docker stats squirreldb
```

## Best Practices

1. **Use structured logging**
   - Parse logs as JSON
   - Add request IDs for tracing

2. **Set appropriate log levels**
   - Production: `info`
   - Debugging: `debug`

3. **Monitor proactively**
   - Set up alerts before issues occur
   - Review metrics regularly

4. **Keep historical data**
   - Store logs for at least 30 days
   - Store metrics for trending

5. **Test monitoring**
   - Verify alerts fire correctly
   - Practice incident response
