# Caching

SquirrelDB includes a Redis-compatible caching layer with support for both built-in and external Redis backends.

## Configuration

Enable caching in your `squirreldb.yaml`:

```yaml
features:
  caching: true

caching:
  mode: builtin          # builtin or proxy
  port: 6379             # Redis-compatible port
  max_memory: "256mb"    # Memory limit (builtin mode)
  eviction: "lru"        # Eviction policy (builtin mode)
```

Or via environment variable:

```bash
SQRL_CACHE_ENABLED=true sqrld
```

## Cache Modes

### Built-in Mode (Default)

Uses an in-memory cache with configurable memory limits and eviction policies:

```yaml
caching:
  mode: builtin
  port: 6379
  max_memory: "256mb"
  eviction: "lru"
  default_ttl: 0
  snapshot:
    enabled: true
    path: "./cache.snapshot"
    interval: 300
```

Configuration options:

| Option | Description | Default |
|--------|-------------|---------|
| `max_memory` | Memory limit (bytes, KB, MB, GB) | 256mb |
| `eviction` | lru, lfu, random, noeviction | lru |
| `default_ttl` | Default TTL in seconds (0 = no expiry) | 0 |
| `snapshot.enabled` | Enable persistence | false |
| `snapshot.path` | Snapshot file path | ./cache.snapshot |
| `snapshot.interval` | Save interval in seconds | 300 |

### Proxy Mode

Connect to an external Redis server instead of using the built-in cache:

```yaml
caching:
  mode: proxy
  port: 6379              # Local port (passthrough)
  proxy:
    host: "redis.example.com"
    port: 6379
    password: "your-redis-password"
    database: 0
    tls_enabled: false
```

Configuration options:

| Option | Description | Default |
|--------|-------------|---------|
| `host` | Redis server hostname | localhost |
| `port` | Redis server port | 6379 |
| `password` | Redis AUTH password | (none) |
| `database` | Redis database number (0-15) | 0 |
| `tls_enabled` | Enable TLS encryption | false |

#### Provider Examples

**AWS ElastiCache:**
```yaml
caching:
  mode: proxy
  proxy:
    host: "my-cluster.xxxxx.cache.amazonaws.com"
    port: 6379
    tls_enabled: true
```

**Redis Cloud:**
```yaml
caching:
  mode: proxy
  proxy:
    host: "redis-12345.c1.us-east-1-2.ec2.cloud.redislabs.com"
    port: 12345
    password: "your-password"
    tls_enabled: true
```

**Self-hosted Redis:**
```yaml
caching:
  mode: proxy
  proxy:
    host: "redis.internal"
    port: 6379
    password: "secret"
    database: 1
```

## Admin UI Settings

Configure cache mode through the Admin UI:

1. Open Admin UI at `http://localhost:8081`
2. Navigate to **Settings** > **Caching**
3. Toggle between **Built-in** and **Proxy** modes
4. For Proxy mode, enter your Redis connection details
5. Click **Test Connection** to verify connectivity
6. Click **Save** to apply changes

### Connection Test

The test connection button verifies:
- Network connectivity to the Redis server
- Authentication (if password configured)
- TLS handshake (if TLS enabled)

A successful test shows "Connected" status. Errors display the specific failure reason.

## Redis Protocol Support

Both modes support the Redis RESP protocol. Connect with `redis-cli` or any Redis client:

```bash
# Connect to SquirrelDB cache
redis-cli -p 6379

# Basic commands
127.0.0.1:6379> PING
PONG
127.0.0.1:6379> SET mykey "Hello"
OK
127.0.0.1:6379> GET mykey
"Hello"
```

### Supported Commands

| Category | Commands |
|----------|----------|
| Basic | GET, SET, DEL, EXISTS, SETNX, SETEX, GETSET |
| TTL | EXPIRE, TTL, PTTL, PERSIST, EXPIREAT |
| Numeric | INCR, DECR, INCRBY, DECRBY, INCRBYFLOAT |
| Bulk | MGET, MSET, MSETNX, KEYS, SCAN |
| String | APPEND, STRLEN, GETRANGE, SETRANGE |
| Admin | PING, INFO, DBSIZE, FLUSHDB, FLUSHALL, SELECT |

## Eviction Policies (Built-in Mode)

| Policy | Description |
|--------|-------------|
| `lru` | Least Recently Used - evicts keys not accessed recently |
| `lfu` | Least Frequently Used - evicts keys with lowest access count |
| `random` | Random eviction when memory limit reached |
| `noeviction` | Return errors when memory limit reached |

## Persistence (Built-in Mode)

Enable snapshots to persist cache data:

```yaml
caching:
  snapshot:
    enabled: true
    path: "./cache.snapshot"
    interval: 300  # 5 minutes
```

Manual snapshot commands:
```bash
# Via redis-cli
redis-cli -p 6379 BGSAVE

# Check last save time
redis-cli -p 6379 LASTSAVE
```

## Monitoring

### INFO Command

```bash
redis-cli -p 6379 INFO
```

Returns cache statistics:
- `used_memory`: Current memory usage
- `maxmemory`: Memory limit (builtin mode)
- `keyspace_hits`: Cache hits
- `keyspace_misses`: Cache misses
- `connected_clients`: Active connections
- `evicted_keys`: Keys evicted due to memory pressure

### Admin API

```bash
GET /api/cache/stats
Authorization: Bearer YOUR_TOKEN
```

Response:
```json
{
  "keys": 1523,
  "memory_used": 52428800,
  "memory_limit": 268435456,
  "hits": 45678,
  "misses": 1234,
  "evictions": 56,
  "expired": 789
}
```

## When to Use Proxy Mode

Choose proxy mode when:

- **Shared cache**: Multiple SquirrelDB instances need shared cache
- **Persistence**: You need Redis persistence features (AOF, RDB)
- **Clustering**: You need Redis Cluster for horizontal scaling
- **Managed service**: You want a managed Redis service (ElastiCache, Redis Cloud)
- **Advanced features**: You need Redis features not in built-in mode (Pub/Sub, Streams)

Choose built-in mode when:

- **Simplicity**: Single server deployment
- **No external dependencies**: Self-contained setup
- **Development**: Local development and testing
- **Low latency**: In-process cache access

## Migration

### Built-in to Proxy

1. Export keys from built-in cache:
   ```bash
   redis-cli -p 6379 --scan > keys.txt
   for key in $(cat keys.txt); do
     redis-cli -p 6379 DUMP "$key" | xxd -p > "dump_$key.hex"
   done
   ```

2. Switch to proxy mode in configuration

3. Import keys to external Redis

### Proxy to Built-in

Note: Built-in mode doesn't support all Redis data types. Only string values are preserved.

## Troubleshooting

### Connection Failed

1. Verify Redis server is running and accessible
2. Check firewall rules allow the connection
3. Verify hostname resolves correctly
4. For TLS, ensure certificates are valid

### Authentication Failed

1. Verify password is correct
2. Check Redis ACL configuration
3. Ensure user has required permissions

### TLS Errors

1. Verify TLS is enabled on the Redis server
2. Check certificate expiration
3. Ensure certificate chain is valid
4. Verify hostname matches certificate

### High Latency

1. Check network latency to Redis server
2. Consider using a closer region
3. Enable connection pooling
4. Monitor Redis server load

### Memory Issues (Built-in)

1. Increase `max_memory` limit
2. Enable eviction policy (not `noeviction`)
3. Set TTLs on keys
4. Monitor key count growth
