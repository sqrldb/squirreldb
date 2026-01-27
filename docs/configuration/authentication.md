# Authentication

SquirrelDB provides token-based authentication to secure the **Admin UI**. When enabled, access to the admin panel, settings, and token management requires a valid token.

> **Note**: Authentication protects only the Admin UI and admin-specific endpoints. The data API (REST, WebSocket) is public and does not require authentication. Use network-level security (firewall, VPN, reverse proxy) to restrict data API access if needed.

## Overview

Authentication in SquirrelDB uses bearer tokens with the `sqrl_` prefix. Tokens are:

- 36 characters total (`sqrl_` + 32 random alphanumeric characters)
- Stored as SHA-256 hashes in the database (never plaintext)
- Shown only once when created - cannot be retrieved later
- Managed via the Admin UI

## What's Protected

| Endpoint | Auth Required | Description |
|----------|---------------|-------------|
| `/` (Admin UI) | Yes | Admin dashboard |
| `/api/settings` | Yes | Settings management |
| `/api/tokens` | Yes | Token management |
| `/ws/logs` | Yes | Server log streaming |
| `/api/collections` | No | Data API |
| `/api/query` | No | Query API |
| `/ws` | No | Data WebSocket |
| `/health`, `/ready` | No | Health checks |

## Configuration

Enable authentication in your `squirreldb.yaml`:

```yaml
auth:
  enabled: true
  admin_token: "your-admin-token-here"  # Optional: separate admin access
```

### Configuration Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `enabled` | bool | `false` | Enable/disable admin authentication |
| `admin_token` | string | `""` | Optional static token for admin access |

## First-Time Setup

When authentication is enabled but no tokens exist, SquirrelDB presents a setup page:

1. Navigate to the Admin UI (`http://localhost:8081`)
2. You'll be redirected to `/setup`
3. Enter a name for your first admin token
4. Click **Create Admin Token**
5. **Important**: Copy the displayed token immediately - it won't be shown again!
6. Click **Continue to Login**
7. Enter your token to access the Admin UI

## Login Flow

After setup, accessing the Admin UI requires authentication:

1. Navigate to the Admin UI
2. You'll be redirected to `/login` if not authenticated
3. Enter your admin token
4. Click **Sign In**
5. Token is stored in browser localStorage for future sessions

## Token Format

Tokens follow this format:

```
sqrl_a1b2c3d4e5f6g7h8i9j0k1l2m3n4o5p6
     └─────────────────────────────────┘
           32 random characters
```

Example tokens:
- `sqrl_x7k9m2p4n8r1t5w3y6q0v2s8u4e7a9c1`
- `sqrl_h3j6l9o2q5t8w1z4b7d0f3g6i9k2m5p8`

## Using Tokens

### Admin UI

Tokens are automatically included via localStorage after login. To log out, click the logout button or clear localStorage.

### Admin API (Settings/Tokens)

Include the token in the `Authorization` header:

```bash
curl -X GET http://localhost:8081/api/settings \
  -H "Authorization: Bearer sqrl_your_token_here"
```

Or use the `token` query parameter:

```bash
curl "http://localhost:8081/api/settings?token=sqrl_your_token_here"
```

### Log Streaming WebSocket

The admin-only log streaming endpoint requires authentication:

```javascript
const logWs = new WebSocket('ws://localhost:8081/ws/logs?token=sqrl_your_token_here');
```

## Creating Additional Tokens

### Via Admin UI

1. Navigate to **Settings** page
2. Scroll to **API Tokens** section
3. Click **Generate Token**
4. Enter a descriptive name (e.g., "CI/CD Admin", "Backup Script")
5. Click **Create**
6. **Important**: Copy the token immediately - it won't be shown again!

### Via REST API

```bash
curl -X POST http://localhost:8081/api/tokens \
  -H "Authorization: Bearer sqrl_admin_token" \
  -H "Content-Type: application/json" \
  -d '{"name": "my-new-token"}'
```

Response:
```json
{
  "token": "sqrl_x7k9m2p4n8r1t5w3y6q0v2s8u4e7a9c1",
  "info": {
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "name": "my-new-token",
    "created_at": "2024-01-15T10:30:00Z"
  }
}
```

## Managing Tokens

### List Tokens

```bash
curl -X GET http://localhost:8081/api/tokens \
  -H "Authorization: Bearer sqrl_admin_token"
```

Response:
```json
[
  {
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "name": "production-admin",
    "created_at": "2024-01-15T10:30:00Z"
  },
  {
    "id": "550e8400-e29b-41d4-a716-446655440001",
    "name": "backup-script",
    "created_at": "2024-01-16T14:20:00Z"
  }
]
```

Note: Token values (hashes) are never returned for security.

### Delete Token

```bash
curl -X DELETE http://localhost:8081/api/tokens/550e8400-e29b-41d4-a716-446655440000 \
  -H "Authorization: Bearer sqrl_admin_token"
```

## Admin Token (Config File)

The `admin_token` in configuration provides a static token for administrative access:

```yaml
auth:
  enabled: true
  admin_token: "my-secure-admin-password"
```

This token:
- Bypasses normal token validation
- Is useful for automation and initial setup
- Can be any string (doesn't need `sqrl_` prefix)
- Should be kept secure and not shared

## Security Best Practices

### 1. Use Strong Tokens

Generated tokens are cryptographically random. Never:
- Create predictable tokens manually
- Reuse tokens across environments
- Share tokens in code or version control

### 2. Rotate Tokens Regularly

Create new tokens and delete old ones periodically:

```bash
# Create new token
NEW_TOKEN=$(curl -s -X POST http://localhost:8081/api/tokens \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name": "admin-v2"}' | jq -r .token)

# Update your configurations with $NEW_TOKEN
# Then delete the old token
curl -X DELETE http://localhost:8081/api/tokens/$OLD_TOKEN_ID \
  -H "Authorization: Bearer $ADMIN_TOKEN"
```

### 3. Use HTTPS in Production

Tokens are sent in headers/URLs. Without HTTPS, they can be intercepted:

```yaml
# Use a reverse proxy with TLS
server:
  host: "127.0.0.1"  # Bind to localhost only
  port: 8080
  admin_port: 8081
```

Then use nginx/caddy with SSL:

```nginx
server {
    listen 443 ssl;
    ssl_certificate /path/to/cert.pem;
    ssl_certificate_key /path/to/key.pem;

    location / {
        proxy_pass http://127.0.0.1:8081;
    }
}
```

### 4. Secure the Data API Separately

Since the data API is public, use network-level security:

```bash
# Firewall rules to restrict access
ufw allow from 10.0.0.0/8 to any port 8080

# Or bind to localhost and use SSH tunnels
server:
  host: "127.0.0.1"
```

### 5. Store Tokens Securely

Use environment variables or secret management:

```bash
# Environment variable
export SQUIRRELDB_ADMIN_TOKEN="sqrl_..."

# In scripts
curl -H "Authorization: Bearer $SQUIRRELDB_ADMIN_TOKEN" ...
```

Never commit tokens to version control.

## Error Responses

### Missing Token

```json
{
  "error": "Authentication required"
}
```
Status: `401 Unauthorized`

### Invalid Token

```json
{
  "error": "Invalid token"
}
```
Status: `401 Unauthorized`

## Database Storage

Tokens are stored in the `api_tokens` table:

```sql
CREATE TABLE api_tokens (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    token_hash TEXT NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);
```

The `token_hash` column stores SHA-256 hashes, never the actual tokens.

## Disabling Authentication

To disable authentication (development only):

```yaml
auth:
  enabled: false
```

When disabled:
- Admin UI is accessible without login
- No setup or login pages are shown
- All admin endpoints are public

**Warning**: Only disable authentication in development environments.

## Troubleshooting

### "Invalid token" with correct token

1. Ensure you're using the complete token including `sqrl_` prefix
2. Check for extra whitespace or newlines
3. Verify the token hasn't been deleted
4. Try regenerating a new token

### Redirected to /setup but I already set up

This happens when all tokens have been deleted. Create a new token through the setup page.

### Can't access Admin UI after enabling auth

1. Use the `admin_token` if configured in yaml
2. Access `/setup` directly if no tokens exist
3. Connect directly to database to add a token manually:

```sql
INSERT INTO api_tokens (id, name, token_hash, created_at)
VALUES (
  uuid(),
  'emergency-admin',
  -- SHA-256 hash of 'sqrl_emergency123...'
  'your_hash_here',
  NOW()
);
```

### Logged out unexpectedly

Tokens are stored in browser localStorage. This can happen if:
- localStorage was cleared
- Different browser/device
- Token was deleted from server

Simply log in again with a valid token.
