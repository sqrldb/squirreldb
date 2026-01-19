# Protocol Configuration

SquirrelDB supports multiple protocols for client communication. You can enable or disable each protocol based on your needs.

## Overview

SquirrelDB provides three protocol options:

| Protocol | Port | Purpose | Default |
|----------|------|---------|---------|
| REST API | 8081 | HTTP-based CRUD operations | Enabled |
| WebSocket | 8080 | Real-time bidirectional communication | Enabled |
| SSE | 8081 | Server-Sent Events (future) | Disabled |

## Configuration

Configure protocols in `squirreldb.yaml`:

```yaml
server:
  host: "0.0.0.0"
  port: 8080
  admin_port: 8081
  protocols:
    rest: true
    websocket: true
    sse: false
```

### Protocol Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `rest` | bool | `true` | Enable REST API endpoints |
| `websocket` | bool | `true` | Enable WebSocket server |
| `sse` | bool | `false` | Enable Server-Sent Events (not yet implemented) |

## REST API

The REST API provides HTTP endpoints for CRUD operations on the admin port.

### When to Use REST

- Simple request/response patterns
- Stateless integrations
- Load balancer compatibility
- Debugging with curl/Postman

### Endpoints Available

When REST is enabled:

```
GET    /api/status                         # Server status
GET    /api/collections                    # List collections
GET    /api/collections/{name}             # Get documents
DELETE /api/collections/{name}             # Drop collection
POST   /api/collections/{name}/documents   # Insert document
GET    /api/collections/{name}/documents/{id}    # Get document
PUT    /api/collections/{name}/documents/{id}    # Update document
DELETE /api/collections/{name}/documents/{id}    # Delete document
POST   /api/query                          # Execute query
```

### Disabling REST

```yaml
server:
  protocols:
    rest: false
```

When disabled, only WebSocket and Admin UI are available.

## WebSocket

WebSocket provides real-time bidirectional communication on the main server port.

### When to Use WebSocket

- Real-time subscriptions
- Live data feeds
- Persistent connections
- High-frequency updates

### Features

- Full query language support
- Change subscriptions
- Multiplexed requests
- Automatic reconnection (client-side)

### Connection URL

```
ws://localhost:8080/ws
wss://localhost:8080/ws  # With TLS
```

With authentication:
```
ws://localhost:8080/ws?token=sqrl_your_token
```

### Message Protocol

```javascript
// Client -> Server
{
  "type": "query",
  "id": "unique-request-id",
  "query": "db.table(\"users\").run()"
}

// Server -> Client
{
  "type": "result",
  "id": "unique-request-id",
  "data": [...]
}
```

### Disabling WebSocket

```yaml
server:
  protocols:
    websocket: false
```

When disabled:
- Main port (8080) is not used
- Subscriptions are unavailable
- Use REST API only

## Server-Sent Events (SSE)

**Note**: SSE is planned but not yet implemented.

SSE will provide one-way streaming from server to client.

### Planned Use Cases

- Change feed streaming
- Log streaming
- Notification delivery

### Planned Configuration

```yaml
server:
  protocols:
    sse: true  # Future
```

## Protocol Comparison

| Feature | REST | WebSocket | SSE |
|---------|------|-----------|-----|
| Bidirectional | No | Yes | No |
| Real-time | No | Yes | Yes |
| Subscriptions | No | Yes | Yes |
| Connection | Per-request | Persistent | Persistent |
| Load balancer | Easy | Sticky sessions | Easy |
| Browser support | All | All | All modern |
| Proxy friendly | Yes | Sometimes | Yes |

## Use Case Scenarios

### Scenario 1: Traditional Web App

REST-only for simple CRUD:

```yaml
server:
  protocols:
    rest: true
    websocket: false
```

### Scenario 2: Real-time Dashboard

WebSocket for live updates:

```yaml
server:
  protocols:
    rest: true      # For initial data load
    websocket: true # For live updates
```

### Scenario 3: Microservice Backend

Both protocols for flexibility:

```yaml
server:
  protocols:
    rest: true      # For service-to-service calls
    websocket: true # For event-driven updates
```

### Scenario 4: Mobile App

WebSocket primary, REST fallback:

```yaml
server:
  protocols:
    rest: true
    websocket: true
```

## Network Configuration

### Firewall Rules

Allow only needed ports:

```bash
# WebSocket only
ufw allow 8080/tcp

# REST/Admin only
ufw allow 8081/tcp

# Both
ufw allow 8080:8081/tcp
```

### Load Balancer

#### REST (stateless)

```nginx
upstream squirreldb_rest {
    server backend1:8081;
    server backend2:8081;
    server backend3:8081;
}
```

#### WebSocket (sticky sessions)

```nginx
upstream squirreldb_ws {
    ip_hash;  # Sticky sessions
    server backend1:8080;
    server backend2:8080;
    server backend3:8080;
}

location /ws {
    proxy_pass http://squirreldb_ws;
    proxy_http_version 1.1;
    proxy_set_header Upgrade $http_upgrade;
    proxy_set_header Connection "upgrade";
}
```

### Docker Compose

```yaml
services:
  squirreldb:
    image: squirreldb:latest
    ports:
      - "8080:8080"  # WebSocket
      - "8081:8081"  # REST/Admin
    environment:
      - SQUIRRELDB_CONFIG=/config/squirreldb.yaml
```

## Monitoring

### Health Checks

Available regardless of protocol settings:

```bash
# Liveness
curl http://localhost:8081/health

# Readiness
curl http://localhost:8081/ready
```

### Connection Metrics

Monitor active connections per protocol:

```bash
# Check WebSocket connections (netstat)
netstat -an | grep :8080 | wc -l

# Check HTTP connections
netstat -an | grep :8081 | wc -l
```

## Troubleshooting

### REST returns 404

Verify REST is enabled:

```yaml
server:
  protocols:
    rest: true  # Must be true
```

### WebSocket connection refused

1. Check WebSocket is enabled
2. Verify port 8080 is accessible
3. Check firewall rules
4. Verify no proxy blocking upgrades

### Slow WebSocket connections

1. Check network latency
2. Reduce message size
3. Use connection pooling
4. Monitor server resources

## Security Considerations

### REST Security

- Use HTTPS in production
- Set appropriate CORS headers
- Rate limit requests
- Validate all input

### WebSocket Security

- Use WSS (WebSocket Secure)
- Authenticate on connect
- Validate message format
- Implement heartbeat/timeout

### Protocol Selection Security

Disable unused protocols to reduce attack surface:

```yaml
server:
  protocols:
    rest: false      # If not needed
    websocket: true  # Primary protocol
```
