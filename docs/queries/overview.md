# Query Language Overview

SquirrelDB uses a chainable query language inspired by RethinkDB. Queries are JavaScript-like expressions that get compiled to efficient SQL.

## Basic Structure

Every query follows this pattern:

```javascript
db.table("collection_name")
  .operation1()
  .operation2()
  .run()
```

- `db.table(name)` - Select a collection
- `.operation()` - Chain operations (filter, orderBy, limit, etc.)
- `.run()` - Execute the query

## Query Methods

### Selecting Data

```javascript
// Get all documents in a collection
db.table("users").run()

// Filter documents
db.table("users").filter(r => r.age > 25).run()

// Order results
db.table("users").orderBy("name").run()
db.table("users").orderBy("age", "desc").run()

// Limit results
db.table("users").limit(10).run()

// Combine operations
db.table("users")
  .filter(r => r.status == "active")
  .orderBy("created_at", "desc")
  .limit(20)
  .run()
```

### Writing Data

```javascript
// Insert a document
db.table("users").insert({ name: "Alice", age: 30 }).run()

// Update a document (by ID)
db.table("users").get("uuid-here").update({ age: 31 }).run()

// Delete a document (by ID)
db.table("users").get("uuid-here").delete().run()
```

### Subscriptions

```javascript
// Subscribe to all changes in a collection
db.table("users").changes()

// Subscribe to filtered changes
db.table("users").filter(r => r.status == "active").changes()
```

## Filter Expressions

Filters use JavaScript arrow function syntax:

### Comparison Operators

```javascript
// Equality
.filter(r => r.name == "Alice")

// Not equal
.filter(r => r.status != "deleted")

// Greater than
.filter(r => r.age > 25)

// Greater than or equal
.filter(r => r.age >= 25)

// Less than
.filter(r => r.age < 30)

// Less than or equal
.filter(r => r.age <= 30)
```

### Logical Operators

```javascript
// AND
.filter(r => r.age > 25 && r.status == "active")

// OR
.filter(r => r.role == "admin" || r.role == "moderator")

// Complex expressions
.filter(r => (r.age > 25 && r.status == "active") || r.role == "admin")
```

### String Values

```javascript
// Single quotes
.filter(r => r.name == 'Alice')

// Double quotes
.filter(r => r.name == "Alice")
```

## How Queries Work

SquirrelDB compiles your queries in two phases:

### 1. SQL Compilation (Fast Path)

Simple expressions are compiled directly to SQL:

```javascript
// This query:
db.table("users").filter(r => r.age > 25).run()

// Compiles to SQL:
SELECT * FROM documents
WHERE collection = 'users'
  AND CAST(data->>'age' AS REAL) > 25
```

### 2. JavaScript Evaluation (Fallback)

Complex expressions that can't be compiled to SQL are evaluated in JavaScript:

```javascript
// This query uses JavaScript evaluation:
db.table("users").filter(r => r.name.startsWith("A")).run()
```

The SQL-compiled path is much faster, so prefer simple comparisons when possible.

## SQL-Compilable Expressions

These expressions compile to efficient SQL:

| Expression | SQL (PostgreSQL) | SQL (SQLite) |
|------------|------------------|--------------|
| `r.field == value` | `data->>'field' = 'value'` | `json_extract(data, '$.field') = 'value'` |
| `r.field > number` | `(data->>'field')::numeric > N` | `CAST(json_extract(data, '$.field') AS REAL) > N` |
| `r.field != value` | `data->>'field' != 'value'` | `json_extract(data, '$.field') != 'value'` |

## Best Practices

### Use Simple Filters

```javascript
// Good - compiles to SQL
.filter(r => r.status == "active")

// Slower - requires JavaScript evaluation
.filter(r => r.status.toLowerCase() == "active")
```

### Index Your Data

For PostgreSQL, create indexes on frequently filtered fields:

```sql
CREATE INDEX idx_users_status ON documents ((data->>'status'))
WHERE collection = 'users';
```

### Limit Results

Always use `.limit()` when you don't need all results:

```javascript
// Good
db.table("logs").orderBy("timestamp", "desc").limit(100).run()

// Bad - fetches all logs
db.table("logs").orderBy("timestamp", "desc").run()
```

### Use Specific Collections

Query specific collections instead of filtering:

```javascript
// Good
db.table("active_users").run()

// Less efficient
db.table("users").filter(r => r.status == "active").run()
```

## Error Handling

Invalid queries return errors:

```javascript
// Missing table name
db.table().run()
// Error: Table name required

// Invalid syntax
db.table("users").filter(r => r.age >).run()
// Error: Parse error

// Unknown method
db.table("users").unknown().run()
// Error: Unknown method 'unknown'
```

## Next Steps

- [Reading Data](./reading.md) - Detailed guide to querying
- [Writing Data](./writing.md) - Insert, update, delete operations
- [Subscriptions](./subscriptions.md) - Real-time change feeds
