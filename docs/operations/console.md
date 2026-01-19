# Console (REPL)

The Console provides an interactive REPL (Read-Eval-Print Loop) for executing queries directly in your browser.

## Accessing the Console

1. Open the Admin UI at `http://localhost:8081`
2. Click **Console** in the sidebar

## Interface

The Console has three main areas:

### Output Area

Displays:
- Welcome message with ASCII logo
- Command history
- Query results
- Error messages
- Execution info

### Input Area

- Command prompt (>)
- Text input for queries
- Run button to execute

### Controls

- **Clear** button to reset output

## Running Queries

### Basic Query

Type a query and press Enter or click Run:

```javascript
db.table("users").run()
```

Output:
```json
[
  {
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "collection": "users",
    "data": {
      "name": "Alice",
      "age": 30
    },
    "created_at": "2024-01-15T10:30:00Z",
    "updated_at": "2024-01-15T10:30:00Z"
  }
]
```

### Filtering

```javascript
db.table("users").filter(u => u.age > 25)
```

### Inserting Data

```javascript
db.table("users").insert({name: "Bob", age: 28})
```

### Complex Queries

```javascript
db.table("orders")
  .filter(o => o.status === "pending")
  .orderBy("created_at", "desc")
  .limit(10)
```

## Special Commands

The Console supports special dot-commands:

### .help

Display available commands:

```
> .help

Available commands:
  .help              Show this help
  .clear             Clear console output
  .tables            List all tables
  .collections       Alias for .tables
  .schema <table>    Show table schema (document fields)
  .count <table>     Count documents in table

Query examples:
  db.table("users").run()
  db.table("users").filter(u => u.age > 21)
  db.table("posts").insert({title: "Hello"})
  db.table("users").get("uuid-here")
  db.table("users").delete("uuid-here")
```

### .tables / .collections

List all tables with document counts:

```
> .tables

Tables:
  users (150 docs)
  posts (500 docs)
  comments (1200 docs)
```

### .schema <table>

Show fields found in documents:

```
> .schema users

Fields in "users":
  - age
  - email
  - name
```

Note: This samples up to 10 documents to discover fields.

### .count <table>

Get document count:

```
> .count users

150 documents in "users"
```

### .clear

Clear the console output:

```
> .clear
```

## Command History

Navigate through previous commands:

- **Up Arrow**: Previous command
- **Down Arrow**: Next command

History is maintained for the session.

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| Enter | Execute command |
| Up Arrow | Previous command |
| Down Arrow | Next command |
| Escape | Clear input |

## Output Formatting

### Commands

Shown in blue with `>` prefix:

```
> db.table("users").run()
```

### Results

Shown as formatted JSON:

```json
[
  {"id": "...", "data": {...}}
]
```

### Errors

Shown in red with error message:

```
Error: Table "nonexistent" not found
```

### Info

Shown in gray after results:

```
3 result(s) in 12.5ms
```

## Query Language

The Console supports the full SquirrelDB query language:

### Read Operations

```javascript
// Get all documents
db.table("users").run()

// Filter documents
db.table("users").filter(u => u.age > 25)

// Order results
db.table("users").orderBy("name")
db.table("users").orderBy("age", "desc")

// Limit results
db.table("users").limit(10)

// Combine operations
db.table("users")
  .filter(u => u.active === true)
  .orderBy("created_at", "desc")
  .limit(20)
```

### Write Operations

```javascript
// Insert document
db.table("users").insert({name: "Alice", age: 30})

// The result includes the new document with ID
```

### Field Access

Access nested fields:

```javascript
db.table("users").filter(u => u.address.city === "NYC")
```

### Comparison Operators

```javascript
// Equality
db.table("users").filter(u => u.age === 30)

// Greater than / less than
db.table("users").filter(u => u.age > 25)
db.table("users").filter(u => u.age < 40)
db.table("users").filter(u => u.age >= 25)
db.table("users").filter(u => u.age <= 40)

// String matching (exact)
db.table("users").filter(u => u.name === "Alice")
```

## Examples

### Example 1: Explore Database

```
> .tables
Tables:
  users (5 docs)
  posts (12 docs)

> .schema users
Fields in "users":
  - age
  - email
  - name

> db.table("users").run()
[...]
```

### Example 2: Find Specific Data

```
> db.table("orders").filter(o => o.status === "pending").limit(5)
[
  {"id": "...", "data": {"status": "pending", "total": 99.99}},
  ...
]

5 result(s) in 8.3ms
```

### Example 3: Insert and Verify

```
> db.table("products").insert({name: "Widget", price: 19.99, stock: 100})
{
  "id": "abc-123-...",
  "collection": "products",
  "data": {"name": "Widget", "price": 19.99, "stock": 100},
  ...
}

1 result(s) in 15.2ms

> .count products
101 documents in "products"
```

### Example 4: Complex Query

```
> db.table("logs").filter(l => l.level === "error").orderBy("timestamp", "desc").limit(10)
[
  {"data": {"level": "error", "message": "Connection timeout", ...}},
  ...
]

10 result(s) in 23.1ms
```

## Error Handling

### Parse Errors

```
> db.table("users").filter(x =>)

Error: Parse error: Unexpected token )
```

### Table Not Found

```
> db.table("nonexistent").run()

Error: Table "nonexistent" not found
```

### Invalid Syntax

```
> this is not valid

Error: Invalid query syntax
```

## Tips

### 1. Use Tab Completion

Tab completion is not yet available but planned for future releases.

### 2. Multi-line Queries

Currently, queries must be single-line. For complex queries, consider using the Data Explorer page.

### 3. Copy Results

Select and copy JSON results for use elsewhere.

### 4. Refresh Data

Run `.tables` after modifications to see updated counts.

### 5. Check Execution Time

The info line shows query timing - useful for performance analysis.

## Limitations

- No tab completion (yet)
- Single-line queries only
- No syntax highlighting in input
- Limited to 1000 results per query

## Comparison with sqrl CLI

| Feature | Console | sqrl CLI |
|---------|---------|----------|
| Location | Browser | Terminal |
| History | Session | Persistent |
| Multi-line | No | Yes |
| Autocomplete | No | Yes |
| Scripting | No | Yes |
| Offline | No | N/A |

Use the Console for quick queries; use `sqrl` CLI for scripting and advanced use.
