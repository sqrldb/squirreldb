# WebSocket Protocol

This document describes the low-level WebSocket protocol used by SquirrelDB. Use this reference when building custom clients or debugging.

## Connection

Connect via WebSocket to the server (default port 8080):

```
ws://localhost:8080
```

or with TLS:

```
wss://example.com:8080
```

## Message Format

All messages are JSON objects with a `type` field and an `id` field for request/response correlation.

## Client Messages

Messages sent from client to server.

### Query

Execute a query and receive results.

```json
{
  "type": "query",
  "id": "unique-request-id",
  "query": "db.table(\"users\").run()"
}
```

### Subscribe

Subscribe to real-time changes.

```json
{
  "type": "subscribe",
  "id": "unique-subscription-id",
  "query": "db.table(\"users\").changes()"
}
```

### Unsubscribe

Stop receiving changes for a subscription.

```json
{
  "type": "unsubscribe",
  "id": "subscription-id-to-cancel"
}
```

### Insert

Insert a new document.

```json
{
  "type": "insert",
  "id": "unique-request-id",
  "collection": "users",
  "data": {
    "name": "Alice",
    "age": 30
  }
}
```

### Update

Update an existing document.

```json
{
  "type": "update",
  "id": "unique-request-id",
  "collection": "users",
  "document_id": "550e8400-e29b-41d4-a716-446655440000",
  "data": {
    "name": "Alice Smith",
    "age": 31
  }
}
```

### Delete

Delete a document.

```json
{
  "type": "delete",
  "id": "unique-request-id",
  "collection": "users",
  "document_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

### List Collections

Get all collection names.

```json
{
  "type": "listcollections",
  "id": "unique-request-id"
}
```

### Ping

Check server connectivity.

```json
{
  "type": "ping",
  "id": "unique-request-id"
}
```

## Server Messages

Messages sent from server to client.

### Result

Successful query/operation result.

```json
{
  "type": "result",
  "id": "request-id",
  "data": [
    {
      "id": "550e8400-e29b-41d4-a716-446655440000",
      "collection": "users",
      "data": { "name": "Alice", "age": 30 },
      "created_at": "2024-01-15T10:30:00Z",
      "updated_at": "2024-01-15T10:30:00Z"
    }
  ]
}
```

### Error

Operation failed.

```json
{
  "type": "error",
  "id": "request-id",
  "error": "Parse error: unexpected token"
}
```

### Subscribed

Subscription created successfully.

```json
{
  "type": "subscribed",
  "id": "subscription-id"
}
```

### Unsubscribed

Subscription cancelled.

```json
{
  "type": "unsubscribed",
  "id": "subscription-id"
}
```

### Change

Real-time change event (for active subscriptions).

```json
{
  "type": "change",
  "id": "subscription-id",
  "change": {
    "type": "insert",
    "new": {
      "id": "...",
      "collection": "users",
      "data": { "name": "Bob" },
      "created_at": "...",
      "updated_at": "..."
    }
  }
}
```

### Pong

Response to ping.

```json
{
  "type": "pong",
  "id": "request-id"
}
```

## Change Event Types

### Initial

Sent for existing documents when subscribing.

```json
{
  "type": "initial",
  "document": {
    "id": "...",
    "collection": "users",
    "data": { "name": "Alice" },
    "created_at": "...",
    "updated_at": "..."
  }
}
```

### Insert

New document created.

```json
{
  "type": "insert",
  "new": {
    "id": "...",
    "collection": "users",
    "data": { "name": "Bob" },
    "created_at": "...",
    "updated_at": "..."
  }
}
```

### Update

Document modified.

```json
{
  "type": "update",
  "old": { "name": "Alice" },
  "new": {
    "id": "...",
    "collection": "users",
    "data": { "name": "Alice Smith" },
    "created_at": "...",
    "updated_at": "..."
  }
}
```

### Delete

Document removed.

```json
{
  "type": "delete",
  "old": {
    "id": "...",
    "collection": "users",
    "data": { "name": "Alice" },
    "created_at": "...",
    "updated_at": "..."
  }
}
```

## Document Structure

All documents have this structure:

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "collection": "users",
  "data": { ... },
  "created_at": "2024-01-15T10:30:00Z",
  "updated_at": "2024-01-15T10:30:00Z"
}
```

| Field | Type | Description |
|-------|------|-------------|
| `id` | UUID string | Unique document identifier |
| `collection` | string | Collection name |
| `data` | object | User data (any valid JSON) |
| `created_at` | ISO 8601 string | Creation timestamp |
| `updated_at` | ISO 8601 string | Last modification timestamp |

## Request ID

Every client message must include a unique `id` field. This ID:

- Correlates requests with responses
- Identifies subscriptions for unsubscribe
- Should be unique per connection

Recommended: Use UUID v4 for request IDs.

## Error Codes

Errors are returned as human-readable strings:

| Error | Cause |
|-------|-------|
| `Parse error: ...` | Invalid query syntax |
| `Unknown table: ...` | Collection doesn't exist |
| `Not found` | Document doesn't exist |
| `Invalid UUID` | Malformed document ID |
| `Connection closed` | WebSocket disconnected |

## Example Session

```
Client → {"type":"ping","id":"1"}
Server → {"type":"pong","id":"1"}

Client → {"type":"insert","id":"2","collection":"users","data":{"name":"Alice"}}
Server → {"type":"result","id":"2","data":{"id":"abc...","collection":"users",...}}

Client → {"type":"subscribe","id":"sub1","query":"db.table(\"users\").changes()"}
Server → {"type":"subscribed","id":"sub1"}
Server → {"type":"change","id":"sub1","change":{"type":"initial","document":{...}}}

Client → {"type":"insert","id":"3","collection":"users","data":{"name":"Bob"}}
Server → {"type":"result","id":"3","data":{...}}
Server → {"type":"change","id":"sub1","change":{"type":"insert","new":{...}}}

Client → {"type":"unsubscribe","id":"sub1"}
Server → {"type":"unsubscribed","id":"sub1"}
```

## Implementing a Client

1. **Connect** via WebSocket
2. **Generate unique IDs** for each request
3. **Track pending requests** by ID
4. **Handle responses** by matching IDs
5. **Route change events** to subscription callbacks
6. **Handle reconnection** and resubscription
