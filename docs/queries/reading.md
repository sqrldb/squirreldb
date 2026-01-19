# Reading Data

This guide covers all the ways to query and retrieve documents from SquirrelDB.

## Basic Queries

### Get All Documents

```javascript
db.table("users").run()
```

Returns all documents in the `users` collection.

### Get Document by ID

Using the SDK's direct method:

```typescript
// TypeScript
const doc = await db.query('db.table("users").get("uuid-here").run()');
```

Or via the SDK's helper (if available):

```typescript
// Some SDKs provide a direct get method
const doc = await db.get("users", "uuid-here");
```

## Filtering

### Basic Filters

```javascript
// Equality
db.table("users").filter(r => r.name == "Alice").run()

// Numeric comparison
db.table("products").filter(r => r.price > 100).run()
db.table("products").filter(r => r.price >= 100).run()
db.table("products").filter(r => r.price < 50).run()
db.table("products").filter(r => r.price <= 50).run()

// Not equal
db.table("users").filter(r => r.status != "deleted").run()
```

### Compound Filters

```javascript
// AND - both conditions must be true
db.table("users").filter(r => r.age > 25 && r.status == "active").run()

// OR - either condition can be true
db.table("users").filter(r => r.role == "admin" || r.role == "moderator").run()

// Complex combinations
db.table("products")
  .filter(r => (r.category == "electronics" && r.price < 500) || r.featured == true)
  .run()
```

### Nested Fields

Access nested object fields:

```javascript
// If data is: { address: { city: "NYC" } }
db.table("users").filter(r => r.address.city == "NYC").run()

// Deeply nested
db.table("orders").filter(r => r.customer.address.country == "USA").run()
```

## Ordering

### Basic Ordering

```javascript
// Ascending (default)
db.table("users").orderBy("name").run()

// Descending
db.table("users").orderBy("age", "desc").run()

// Explicit ascending
db.table("users").orderBy("created_at", "asc").run()
```

### Combined with Filter

```javascript
// Filter first, then order
db.table("products")
  .filter(r => r.category == "electronics")
  .orderBy("price", "asc")
  .run()
```

## Limiting Results

### Basic Limit

```javascript
// Get first 10
db.table("users").limit(10).run()

// Combine with order for "top N"
db.table("products").orderBy("sales", "desc").limit(5).run()  // Top 5 products
```

### Pagination

For pagination, combine limit with offset (via SDK):

```typescript
// TypeScript SDK
const page1 = await db.query('db.table("users").orderBy("name").limit(20).run()');

// For page 2, use the SDK's offset support or cursor-based pagination
```

## Combining Operations

The order of operations matters:

```javascript
db.table("products")
  .filter(r => r.category == "electronics")  // 1. Filter first
  .orderBy("price", "desc")                   // 2. Then sort
  .limit(10)                                   // 3. Finally limit
  .run()
```

This query:
1. Filters to only electronics
2. Sorts by price (highest first)
3. Returns top 10

## Query Examples

### User Management

```javascript
// Active users
db.table("users").filter(r => r.status == "active").run()

// Users created today
db.table("users").filter(r => r.created_at > "2024-01-15").run()

// Admin users sorted by name
db.table("users")
  .filter(r => r.role == "admin")
  .orderBy("name")
  .run()
```

### E-commerce

```javascript
// Products in price range
db.table("products").filter(r => r.price >= 10 && r.price <= 50).run()

// Featured products
db.table("products")
  .filter(r => r.featured == true)
  .orderBy("sales", "desc")
  .limit(12)
  .run()

// Out of stock items
db.table("products").filter(r => r.inventory == 0).run()
```

### Analytics

```javascript
// Recent events
db.table("events")
  .orderBy("timestamp", "desc")
  .limit(100)
  .run()

// Error events
db.table("events")
  .filter(r => r.level == "error")
  .orderBy("timestamp", "desc")
  .limit(50)
  .run()
```

## Performance Tips

### Use Indexes

For PostgreSQL, create indexes on frequently queried fields:

```sql
-- Index on status field
CREATE INDEX idx_users_status ON documents ((data->>'status'))
WHERE collection = 'users';

-- Index on numeric field
CREATE INDEX idx_products_price ON documents (((data->>'price')::numeric))
WHERE collection = 'products';
```

### Avoid Full Table Scans

```javascript
// Bad - scans entire collection
db.table("huge_collection").run()

// Better - filter or limit
db.table("huge_collection").filter(r => r.active == true).run()
db.table("huge_collection").limit(100).run()
```

### Use SQL-Compilable Expressions

```javascript
// Fast - compiles to SQL
db.table("users").filter(r => r.age > 25).run()

// Slower - requires JavaScript evaluation
db.table("users").filter(r => r.name.includes("Alice")).run()
```

## Error Handling

Handle query errors in your SDK:

```typescript
// TypeScript
try {
  const users = await db.query('db.table("users").run()');
} catch (error) {
  console.error("Query failed:", error.message);
}
```

```python
# Python
try:
    users = await db.query('db.table("users").run()')
except Exception as e:
    print(f"Query failed: {e}")
```

## List Collections

To see what collections exist:

```typescript
const collections = await db.listCollections();
console.log(collections);  // ["users", "products", "orders"]
```

## Next Steps

- [Writing Data](./writing.md) - Insert, update, delete operations
- [Subscriptions](./subscriptions.md) - Real-time change feeds
