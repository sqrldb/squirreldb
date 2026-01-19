# Deployment Guide

This guide covers deploying SquirrelDB in production environments.

## Deployment Checklist

Before deploying:

- [ ] Choose backend (PostgreSQL for production)
- [ ] Configure environment variables for secrets
- [ ] Set up health check endpoints
- [ ] Configure logging level
- [ ] Plan for backups
- [ ] Set up monitoring

## Docker Deployment

### Basic Docker Run

```bash
docker run -d \
  --name squirreldb \
  -p 8080:8080 \
  -p 8081:8081 \
  -e DATABASE_URL="postgres://user:pass@host:5432/squirreldb" \
  -e LOG_LEVEL=info \
  squirreldb/squirreldb:latest
```

### Docker Compose

```yaml
version: '3.8'

services:
  squirreldb:
    image: squirreldb/squirreldb:latest
    restart: unless-stopped
    ports:
      - "8080:8080"
      - "8081:8081"
    environment:
      - DATABASE_URL=postgres://postgres:postgres@db:5432/squirreldb
      - LOG_LEVEL=info
    depends_on:
      db:
        condition: service_healthy
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8081/health"]
      interval: 30s
      timeout: 10s
      retries: 3

  db:
    image: postgres:16-alpine
    restart: unless-stopped
    environment:
      - POSTGRES_USER=postgres
      - POSTGRES_PASSWORD=postgres
      - POSTGRES_DB=squirreldb
    volumes:
      - pgdata:/var/lib/postgresql/data
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U postgres"]
      interval: 10s
      timeout: 5s
      retries: 5

volumes:
  pgdata:
```

### Building Custom Image

```dockerfile
FROM squirreldb/squirreldb:latest

# Add custom config
COPY squirreldb.yaml /app/squirreldb.yaml

# Custom entrypoint if needed
# ENTRYPOINT ["sqrld", "--config", "/app/squirreldb.yaml"]
```

## Kubernetes Deployment

### Deployment Manifest

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: squirreldb
  labels:
    app: squirreldb
spec:
  replicas: 1
  selector:
    matchLabels:
      app: squirreldb
  template:
    metadata:
      labels:
        app: squirreldb
    spec:
      containers:
        - name: squirreldb
          image: squirreldb/squirreldb:latest
          ports:
            - containerPort: 8080
              name: websocket
            - containerPort: 8081
              name: admin
          env:
            - name: DATABASE_URL
              valueFrom:
                secretKeyRef:
                  name: squirreldb-secrets
                  key: database-url
            - name: LOG_LEVEL
              value: "info"
          livenessProbe:
            httpGet:
              path: /health
              port: admin
            initialDelaySeconds: 10
            periodSeconds: 30
          readinessProbe:
            httpGet:
              path: /ready
              port: admin
            initialDelaySeconds: 5
            periodSeconds: 10
          resources:
            requests:
              memory: "128Mi"
              cpu: "100m"
            limits:
              memory: "512Mi"
              cpu: "500m"
```

### Service

```yaml
apiVersion: v1
kind: Service
metadata:
  name: squirreldb
spec:
  selector:
    app: squirreldb
  ports:
    - name: websocket
      port: 8080
      targetPort: 8080
    - name: admin
      port: 8081
      targetPort: 8081
  type: ClusterIP
```

### Secret

```yaml
apiVersion: v1
kind: Secret
metadata:
  name: squirreldb-secrets
type: Opaque
stringData:
  database-url: "postgres://user:password@postgres-host:5432/squirreldb"
```

### Ingress (for Admin UI)

```yaml
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: squirreldb-admin
  annotations:
    nginx.ingress.kubernetes.io/backend-protocol: "HTTP"
spec:
  rules:
    - host: squirreldb-admin.example.com
      http:
        paths:
          - path: /
            pathType: Prefix
            backend:
              service:
                name: squirreldb
                port:
                  number: 8081
```

## Systemd Service

For bare-metal or VM deployments:

```ini
# /etc/systemd/system/squirreldb.service
[Unit]
Description=SquirrelDB Server
After=network.target postgresql.service
Wants=postgresql.service

[Service]
Type=simple
User=squirreldb
Group=squirreldb
WorkingDirectory=/opt/squirreldb
ExecStart=/opt/squirreldb/sqrld --config /etc/squirreldb/config.yaml
Restart=always
RestartSec=5
StandardOutput=journal
StandardError=journal

# Security hardening
NoNewPrivileges=yes
ProtectSystem=strict
ProtectHome=yes
ReadWritePaths=/var/lib/squirreldb

[Install]
WantedBy=multi-user.target
```

Enable and start:

```bash
sudo systemctl daemon-reload
sudo systemctl enable squirreldb
sudo systemctl start squirreldb
sudo systemctl status squirreldb
```

## Health Checks

SquirrelDB provides two health endpoints on the admin port (default 8081):

### Liveness Probe

```
GET /health
```

Returns `200 OK` if the server is running. Use for Kubernetes liveness probes or load balancer health checks.

### Readiness Probe

```
GET /ready
```

Returns `200 OK` if the server can connect to the database. Use for Kubernetes readiness probes.

## Load Balancing

### WebSocket Considerations

WebSocket connections are long-lived. Configure your load balancer:

- **Sticky sessions**: Not required (each connection is independent)
- **Timeouts**: Set idle timeout to at least 60 seconds
- **Health checks**: Use `/health` endpoint on admin port

### nginx Configuration

```nginx
upstream squirreldb_ws {
    server squirreldb1:8080;
    server squirreldb2:8080;
}

upstream squirreldb_admin {
    server squirreldb1:8081;
    server squirreldb2:8081;
}

server {
    listen 80;
    server_name squirreldb.example.com;

    # WebSocket endpoint
    location /ws {
        proxy_pass http://squirreldb_ws;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host $host;
        proxy_read_timeout 86400;
    }

    # Admin UI
    location / {
        proxy_pass http://squirreldb_admin;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
    }
}
```

### AWS ALB

For AWS Application Load Balancer:

1. Create target group for port 8080 (WebSocket)
2. Create target group for port 8081 (Admin)
3. Configure listener rules
4. Enable WebSocket support (automatic in ALB)

## Scaling

### Horizontal Scaling

SquirrelDB instances are stateless and can be scaled horizontally:

```yaml
# Kubernetes: scale replicas
kubectl scale deployment squirreldb --replicas=3

# Docker Compose: scale service
docker-compose up -d --scale squirreldb=3
```

**Note**: Each client connects to one instance. Subscriptions are per-connection, so a change on one instance is visible to clients connected to that instance.

### Database Scaling

For high-traffic deployments, scale the PostgreSQL database:

- **Read replicas**: Route read queries to replicas
- **Connection pooling**: Use PgBouncer
- **Partitioning**: Partition large collections

## Security

### Network Security

- Run SquirrelDB in a private network
- Expose only necessary ports
- Use TLS for all connections

### Admin UI Access

Restrict admin UI access:

```nginx
# nginx: IP whitelist
location / {
    allow 10.0.0.0/8;
    deny all;
    proxy_pass http://squirreldb_admin;
}
```

### Environment Variables

Never commit secrets to version control:

```yaml
# Use environment variables
postgres:
  url: $DATABASE_URL
```

## Monitoring

### Logs

SquirrelDB logs to stdout/stderr in a structured format:

```bash
# Docker
docker logs squirreldb

# Kubernetes
kubectl logs deployment/squirreldb

# Systemd
journalctl -u squirreldb -f
```

### Metrics

Monitor these key metrics:

- WebSocket connections count
- Query latency
- Error rates
- Database connection pool usage

### Alerting

Set up alerts for:

- Health check failures
- High error rates
- Database connection issues
- High memory usage

## Backup and Recovery

### PostgreSQL Backups

```bash
# Manual backup
pg_dump squirreldb > backup.sql

# Restore
psql squirreldb < backup.sql
```

### SQLite Backups

```bash
# Copy database file (stop server first)
cp squirreldb.db backup.db

# Or use SQLite backup command
sqlite3 squirreldb.db ".backup backup.db"
```

## Troubleshooting

### Connection Refused

```
Error: Connection refused
```

- Check server is running: `systemctl status squirreldb`
- Check ports are open: `netstat -tlnp | grep 8080`
- Check firewall rules

### Database Connection Failed

```
Error: Database connection failed
```

- Verify DATABASE_URL is correct
- Check PostgreSQL is running
- Verify network connectivity to database
- Check database credentials

### Out of Memory

```
Error: Out of memory
```

- Increase container/pod memory limits
- Check for connection leaks
- Reduce connection pool size

### High Latency

- Check database query performance
- Add indexes for frequently filtered fields
- Enable query caching
- Scale horizontally
