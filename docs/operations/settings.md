# Settings

The Settings page provides a centralized interface for viewing and managing SquirrelDB configuration.

## Accessing Settings

1. Open the Admin UI at `http://localhost:8081`
2. Click **Settings** in the sidebar

## Sections

### Protocols

View currently enabled protocols:

| Protocol | Description | Status |
|----------|-------------|--------|
| REST API | HTTP endpoints for CRUD operations | Toggle |
| WebSocket | Real-time bidirectional communication | Toggle |
| SSE | Server-Sent Events (coming soon) | Disabled |

**Note**: Protocol toggles show current state but require server restart to change.

### Authentication

View authentication status:

- **Enabled/Disabled**: Current auth state
- **Warning**: Displayed when auth is disabled

When authentication is disabled, a warning appears:

> Authentication is disabled. The API is publicly accessible.

### API Tokens

Manage API tokens for authentication:

- **Token List**: View all created tokens
- **Generate Token**: Create new tokens
- **Delete Token**: Remove existing tokens

## Managing API Tokens

### Viewing Tokens

The token list displays:

| Column | Description |
|--------|-------------|
| Name | Descriptive token name |
| Created | Token creation date |
| Actions | Delete button |

Token values are never shown after creation for security.

### Creating a Token

1. Click **Generate Token** button
2. Enter a descriptive name:
   - `production-api`
   - `ci-cd-pipeline`
   - `monitoring-service`
3. Click **Create**
4. **Copy the token immediately** - it won't be shown again!

The token appears in a modal:

```
Token created successfully!

Warning: Copy this token now. You won't be able to see it again!

sqrl_x7k9m2p4n8r1t5w3y6q0v2s8u4e7a9c1

[Copy] [Close]
```

### Deleting a Token

1. Find the token in the list
2. Click the delete (trash) icon
3. Confirm deletion in the modal
4. Token is immediately invalidated

**Warning**: Deleting a token immediately revokes access for any application using it.

## Configuration File

Settings shown in the UI come from `squirreldb.yaml`:

```yaml
server:
  host: "0.0.0.0"
  port: 8080
  admin_port: 8081
  protocols:
    rest: true
    websocket: true
    sse: false

auth:
  enabled: true
  admin_token: "optional-admin-password"
```

### Changing Settings

Currently, settings changes require:

1. Edit `squirreldb.yaml`
2. Restart SquirrelDB

Runtime configuration updates are planned for future releases.

## Protocol Settings

### REST API

When enabled:
- All `/api/*` endpoints are available
- CRUD operations via HTTP
- Query execution via POST

When disabled:
- API endpoints return 404
- Use WebSocket for all operations

### WebSocket

When enabled:
- `/ws` endpoint available on main port
- Real-time subscriptions supported
- Full query language access

When disabled:
- Main port is not used
- No subscription support
- REST-only mode

### SSE (Coming Soon)

Server-Sent Events will provide:
- One-way streaming
- Change notifications
- Lower overhead than WebSocket

## Authentication Settings

### Enabling Authentication

In `squirreldb.yaml`:

```yaml
auth:
  enabled: true
```

Effects:
- All API endpoints require tokens
- WebSocket requires token in URL
- Admin operations require admin token or API token

### Admin Token

The admin token provides a master password:

```yaml
auth:
  enabled: true
  admin_token: "super-secret-admin-password"
```

Use cases:
- Initial token creation
- Emergency access
- Admin-only operations

Best practices:
- Keep it secret
- Use a strong, unique value
- Consider removing after creating API tokens

### Token Storage

Tokens are stored in the database:

- **Table**: `api_tokens`
- **Fields**: id, name, token_hash, created_at
- **Security**: Only SHA-256 hashes stored

## Security Considerations

### Token Visibility

- Full token shown only once at creation
- Never logged or stored in plaintext
- Cannot be retrieved after creation

### Token Rotation

Regularly rotate tokens:

1. Create new token
2. Update applications
3. Delete old token
4. Verify functionality

### Access Control

- Limit who can access Settings page
- Use network-level restrictions
- Enable authentication in production

## Troubleshooting

### Can't Create Token

1. Check authentication is not blocking you
2. Verify database connectivity
3. Check for unique name constraint violations

### Token Not Working

1. Verify full token copied (including `sqrl_` prefix)
2. Check for whitespace
3. Confirm token wasn't deleted
4. Try creating a new token

### Settings Not Updating

1. Settings require server restart
2. Edit `squirreldb.yaml`
3. Restart SquirrelDB service
4. Refresh the Settings page

### Authentication Locked Out

If you can't access after enabling auth:

1. Use `admin_token` if configured
2. Or modify database directly:

```sql
-- Add a token hash manually
INSERT INTO api_tokens (id, name, token_hash, created_at)
VALUES (
  uuid(),
  'emergency',
  'hash-of-known-token',
  NOW()
);
```

3. Or disable auth in config and restart

## API Reference

Settings are managed via REST API:

### Get Settings

```bash
GET /api/settings
Authorization: Bearer sqrl_xxx
```

Response:
```json
{
  "protocols": {
    "rest": true,
    "websocket": true,
    "sse": false
  },
  "auth": {
    "enabled": true
  }
}
```

### List Tokens

```bash
GET /api/tokens
Authorization: Bearer sqrl_xxx
```

Response:
```json
[
  {
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "name": "production-api",
    "created_at": "2024-01-15T10:30:00Z"
  }
]
```

### Create Token

```bash
POST /api/tokens
Authorization: Bearer sqrl_xxx
Content-Type: application/json

{"name": "new-token"}
```

Response:
```json
{
  "token": "sqrl_x7k9m2p4n8r1t5w3y6q0v2s8u4e7a9c1",
  "info": {
    "id": "...",
    "name": "new-token",
    "created_at": "..."
  }
}
```

### Delete Token

```bash
DELETE /api/tokens/{id}
Authorization: Bearer sqrl_xxx
```

Response:
```json
{"deleted": true}
```

## Best Practices

### 1. Enable Auth in Production

Always enable authentication for production deployments:

```yaml
auth:
  enabled: true
```

### 2. Use Descriptive Token Names

Name tokens by their purpose:
- `frontend-prod`
- `backend-api-v2`
- `monitoring-readonly`

### 3. Separate Tokens by Environment

Create different tokens for:
- Production
- Staging
- Development
- CI/CD

### 4. Audit Token Usage

Periodically review:
- Which tokens exist
- When they were created
- Whether they're still needed

### 5. Document Token Owners

Maintain a record of:
- Token name
- Owner/team
- Purpose
- Rotation schedule

## Future Enhancements

Planned improvements:

- Runtime setting changes without restart
- Token permissions/scopes
- Token expiration dates
- Usage statistics per token
- Audit log for token operations
- Environment variable overrides
