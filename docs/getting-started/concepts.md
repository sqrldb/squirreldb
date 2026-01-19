# Core Concepts

Understanding SquirrelDB's core concepts will help you use it effectively.

## Documents

SquirrelDB stores data as **documents**. A document is a JSON object with system-managed metadata:

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "collection": "users",
  "data": {
    "name": "Alice",
    "age": 30,
    "email": "alice@example.com"
  },
  "created_at": "2024-01-15T10:30:00Z",
  "updated_at": "2024-01-15T10:30:00Z"
}
```

| Field | Description |
|-------|-------------|
| `id` | Unique identifier (UUID v4, auto-generated) |
| `collection` | The table/collection this document belongs to |
| `data` | Your actual data (any valid JSON) |
| `created_at` | Timestamp when document was created |
| `updated_at` | Timestamp when document was last modified |

## Collections

A **collection** (also called a table) is a group of documents. Collections are created automatically when you insert your first document:

```javascript
// This creates the "users" collection if it doesn't exist
db.table("users").insert({ name: "Alice" }).run()
```

Collections in SquirrelDB:
- Are schema-less (documents can have different fields)
- Are created on first insert
- Can be listed with `listCollections()`
- Cannot be explicitly created or dropped (drop by deleting all documents)

## Queries

SquirrelDB uses a **chainable query language** inspired by RethinkDB:

```javascript
db.table("users")           // Select collection
  .filter(r => r.age > 25)  // Filter documents
  .orderBy("name")          // Sort results
  .limit(10)                // Limit count
  .run()                    // Execute query
```

### Query Chain Methods

| Method | Description |
|--------|-------------|
| `db.table(name)` | Select a collection |
| `.filter(predicate)` | Filter documents |
| `.orderBy(field, direction?)` | Sort results |
| `.limit(n)` | Limit result count |
| `.changes()` | Subscribe to changes |
| `.run()` | Execute the query |

### Filter Predicates

Filters use JavaScript arrow functions:

```javascript
// Equality
.filter(r => r.status == "active")

// Comparison
.filter(r => r.age > 25)
.filter(r => r.price <= 100)

// String matching
.filter(r => r.name == "Alice")

// Complex expressions (evaluated in JavaScript)
.filter(r => r.age > 25 && r.status == "active")
```

## Change Feeds

SquirrelDB's killer feature is **real-time change feeds**. Subscribe to any query and receive updates when matching documents change:

```javascript
db.table("users").changes()
```

### Change Event Types

| Type | Description | Fields |
|------|-------------|--------|
| `initial` | Existing document on subscribe | `document` |
| `insert` | New document created | `new` |
| `update` | Document modified | `old`, `new` |
| `delete` | Document removed | `old` |

### Change Event Example

```json
{
  "type": "update",
  "old": { "name": "Alice", "age": 30 },
  "new": {
    "id": "...",
    "collection": "users",
    "data": { "name": "Alice", "age": 31 },
    "created_at": "...",
    "updated_at": "..."
  }
}
```

## Backends

SquirrelDB supports two database backends:

### PostgreSQL

Best for production deployments:
- Full ACID transactions
- Scales horizontally with read replicas
- JSONB indexing for fast queries
- Reliable change detection via triggers

### SQLite

Best for development and embedded use:
- Zero configuration
- Single file database
- Fast for small datasets
- Portable and easy to backup

Both backends implement the same interface, so your application code works identically with either.

## Architecture Overview

```
┌─────────────────┐
│   Your App      │
│   (SDK)         │
└────────┬────────┘
         │ WebSocket (JSON)
         ▼
┌─────────────────┐
│  SquirrelDB     │
│  Server         │
├─────────────────┤
│  Query Engine   │  ← Parses queries, compiles to SQL
├─────────────────┤
│  Subscription   │  ← Manages change feeds
│  Manager        │
├─────────────────┤
│  Backend        │  ← PostgreSQL or SQLite
└─────────────────┘
```

## Protocol

SquirrelDB uses a simple JSON-over-WebSocket protocol:

**Client → Server:**
```json
{"type": "query", "id": "abc123", "query": "db.table(\"users\").run()"}
```

**Server → Client:**
```json
{"type": "result", "id": "abc123", "data": [...]}
```

Every message has a unique `id` to correlate requests with responses. See the [Protocol Reference](../reference/protocol.md) for details.

## Next Steps

- [Reading Data](../queries/reading.md) - Learn to query documents
- [Writing Data](../queries/writing.md) - Insert, update, delete operations
- [Subscriptions](../queries/subscriptions.md) - Real-time change feeds
