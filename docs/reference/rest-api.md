# REST API Reference

The Admin UI exposes a REST API on the admin port (default 8081). This API can be used for management tasks and integration.

## Base URL

```
http://localhost:8081/api
```

## Authentication

Currently, the REST API has no authentication. Restrict access via network controls.

## Endpoints

### Server Status

Get server information.

```
GET /api/status
```

**Response:**

```json
{
  "name": "SquirrelDB",
  "version": "0.0.1",
  "backend": "Postgres",
  "uptime_secs": 3600
}
```

| Field | Type | Description |
|-------|------|-------------|
| `name` | string | Server name |
| `version` | string | Server version |
| `backend` | string | Backend type (Postgres/Sqlite) |
| `uptime_secs` | number | Uptime in seconds |

---

### List Collections

Get all collections with document counts.

```
GET /api/collections
```

**Response:**

```json
[
  { "name": "users", "count": 150 },
  { "name": "posts", "count": 500 },
  { "name": "comments", "count": 1200 }
]
```

| Field | Type | Description |
|-------|------|-------------|
| `name` | string | Collection name |
| `count` | number | Document count |

---

### Get Collection Documents

Get documents in a collection.

```
GET /api/collections/{name}
```

**Parameters:**

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `name` | path | required | Collection name |
| `limit` | query | none | Max documents to return |
| `offset` | query | 0 | Number of documents to skip |

**Example:**

```
GET /api/collections/users?limit=10&offset=20
```

**Response:**

```json
[
  {
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "collection": "users",
    "data": {
      "name": "Alice",
      "email": "alice@example.com"
    },
    "created_at": "2024-01-15T10:30:00Z",
    "updated_at": "2024-01-15T10:30:00Z"
  }
]
```

---

### Drop Collection

Delete all documents in a collection.

```
DELETE /api/collections/{name}
```

**Parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `name` | path | Collection name |

**Response:**

```json
{
  "deleted": 150
}
```

---

### Insert Document

Create a new document.

```
POST /api/collections/{name}/documents
Content-Type: application/json
```

**Parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `name` | path | Collection name |

**Body:**

```json
{
  "name": "Alice",
  "email": "alice@example.com",
  "age": 30
}
```

**Response:**

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "collection": "users",
  "data": {
    "name": "Alice",
    "email": "alice@example.com",
    "age": 30
  },
  "created_at": "2024-01-15T10:30:00Z",
  "updated_at": "2024-01-15T10:30:00Z"
}
```

---

### Get Document

Get a single document by ID.

```
GET /api/collections/{name}/documents/{id}
```

**Parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `name` | path | Collection name |
| `id` | path | Document UUID |

**Response:**

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "collection": "users",
  "data": {
    "name": "Alice",
    "email": "alice@example.com"
  },
  "created_at": "2024-01-15T10:30:00Z",
  "updated_at": "2024-01-15T10:30:00Z"
}
```

**Errors:**

- `404 Not Found` - Document doesn't exist
- `400 Bad Request` - Invalid UUID format

---

### Delete Document

Delete a document by ID.

```
DELETE /api/collections/{name}/documents/{id}
```

**Parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `name` | path | Collection name |
| `id` | path | Document UUID |

**Response:**

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "collection": "users",
  "data": {
    "name": "Alice",
    "email": "alice@example.com"
  },
  "created_at": "2024-01-15T10:30:00Z",
  "updated_at": "2024-01-15T10:30:00Z"
}
```

---

### Execute Query

Run a query.

```
POST /api/query
Content-Type: application/json
```

**Body:**

```json
{
  "query": "db.table(\"users\").filter(r => r.age > 25).run()"
}
```

**Response:**

```json
[
  {
    "id": "...",
    "collection": "users",
    "data": { "name": "Alice", "age": 30 },
    "created_at": "...",
    "updated_at": "..."
  }
]
```

---

## Health Endpoints

These endpoints are at the root path, not under `/api`.

### Liveness

```
GET /health
```

**Response:** `200 OK` (empty body)

### Readiness

```
GET /ready
```

**Response:**
- `200 OK` - Database accessible
- `503 Service Unavailable` - Database unreachable

---

## Error Responses

Errors return JSON with an `error` field:

```json
{
  "error": "Not found"
}
```

### HTTP Status Codes

| Code | Description |
|------|-------------|
| `200` | Success |
| `400` | Bad request (invalid input) |
| `404` | Not found |
| `500` | Internal server error |
| `503` | Service unavailable |

---

## CORS

The API allows cross-origin requests (CORS is permissive). This enables browser-based tools to access the API.

---

## Examples

### cURL

```bash
# Get status
curl http://localhost:8081/api/status

# List collections
curl http://localhost:8081/api/collections

# Insert document
curl -X POST http://localhost:8081/api/collections/users/documents \
  -H "Content-Type: application/json" \
  -d '{"name": "Alice", "age": 30}'

# Query
curl -X POST http://localhost:8081/api/query \
  -H "Content-Type: application/json" \
  -d '{"query": "db.table(\"users\").run()"}'

# Delete document
curl -X DELETE http://localhost:8081/api/collections/users/documents/uuid-here
```

### JavaScript (fetch)

```javascript
// Get collections
const collections = await fetch('http://localhost:8081/api/collections')
  .then(r => r.json());

// Insert document
const user = await fetch('http://localhost:8081/api/collections/users/documents', {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({ name: 'Alice', age: 30 })
}).then(r => r.json());

// Query
const users = await fetch('http://localhost:8081/api/query', {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({ query: 'db.table("users").run()' })
}).then(r => r.json());
```

### Python (requests)

```python
import requests

# Get status
status = requests.get('http://localhost:8081/api/status').json()

# Insert document
user = requests.post(
    'http://localhost:8081/api/collections/users/documents',
    json={'name': 'Alice', 'age': 30}
).json()

# Query
users = requests.post(
    'http://localhost:8081/api/query',
    json={'query': 'db.table("users").run()'}
).json()
```
