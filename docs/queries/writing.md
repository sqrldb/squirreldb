# Writing Data

This guide covers inserting, updating, and deleting documents in SquirrelDB.

## Insert

### Single Document

```typescript
// TypeScript
const user = await db.insert("users", {
  name: "Alice",
  email: "alice@example.com",
  age: 30
});

console.log(user.id);          // Auto-generated UUID
console.log(user.created_at);  // Timestamp
```

```python
# Python
user = await db.insert("users", {
    "name": "Alice",
    "email": "alice@example.com",
    "age": 30
})
```

```ruby
# Ruby
user = db.insert("users", {
  name: "Alice",
  email: "alice@example.com",
  age: 30
})
```

```elixir
# Elixir
{:ok, user} = SquirrelDB.insert(db, "users", %{
  name: "Alice",
  email: "alice@example.com",
  age: 30
})
```

### Return Value

Insert returns the complete document with system fields:

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

### Document Structure

You can insert any valid JSON structure:

```typescript
// Nested objects
await db.insert("users", {
  name: "Bob",
  address: {
    street: "123 Main St",
    city: "New York",
    country: "USA"
  },
  tags: ["developer", "admin"],
  metadata: {
    source: "signup",
    campaign: "summer2024"
  }
});

// Arrays
await db.insert("orders", {
  items: [
    { product_id: "abc", quantity: 2, price: 29.99 },
    { product_id: "def", quantity: 1, price: 49.99 }
  ],
  total: 109.97
});
```

### Creating Collections

Collections are created automatically on first insert:

```typescript
// This creates the "new_collection" if it doesn't exist
await db.insert("new_collection", { data: "value" });
```

## Update

### Update by ID

```typescript
// TypeScript
const updated = await db.update("users", "uuid-here", {
  name: "Alice Smith",
  age: 31
});
```

```python
# Python
updated = await db.update("users", "uuid-here", {
    "name": "Alice Smith",
    "age": 31
})
```

```ruby
# Ruby
updated = db.update("users", "uuid-here", {
  name: "Alice Smith",
  age: 31
})
```

```elixir
# Elixir
{:ok, updated} = SquirrelDB.update(db, "users", "uuid-here", %{
  name: "Alice Smith",
  age: 31
})
```

### Update Behavior

Updates **replace** the entire `data` field. If you want to preserve existing fields, read the document first:

```typescript
// Get current document
const users = await db.query('db.table("users").filter(r => r.id == "uuid-here").run()');
const user = users[0];

// Merge changes
const updated = await db.update("users", user.id, {
  ...user.data,      // Preserve existing fields
  age: 31            // Update specific field
});
```

### Return Value

Update returns the modified document:

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "collection": "users",
  "data": {
    "name": "Alice Smith",
    "age": 31
  },
  "created_at": "2024-01-15T10:30:00Z",
  "updated_at": "2024-01-15T11:45:00Z"
}
```

Note that `updated_at` is automatically set to the current time.

## Delete

### Delete by ID

```typescript
// TypeScript
const deleted = await db.delete("users", "uuid-here");
```

```python
# Python
deleted = await db.delete("users", "uuid-here")
```

```ruby
# Ruby
deleted = db.delete("users", "uuid-here")
```

```elixir
# Elixir
{:ok, deleted} = SquirrelDB.delete(db, "users", "uuid-here")
```

### Return Value

Delete returns the deleted document:

```json
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
```

### Delete Non-Existent Document

Deleting a document that doesn't exist returns an error or null (depending on SDK):

```typescript
try {
  await db.delete("users", "non-existent-id");
} catch (error) {
  console.error("Document not found");
}
```

## Bulk Operations

For bulk operations, use loops or parallel requests:

### Bulk Insert

```typescript
// TypeScript - parallel inserts
const users = [
  { name: "Alice", age: 30 },
  { name: "Bob", age: 25 },
  { name: "Charlie", age: 35 }
];

const results = await Promise.all(
  users.map(user => db.insert("users", user))
);
```

```python
# Python - parallel inserts
import asyncio

users = [
    {"name": "Alice", "age": 30},
    {"name": "Bob", "age": 25},
    {"name": "Charlie", "age": 35}
]

results = await asyncio.gather(*[
    db.insert("users", user) for user in users
])
```

### Bulk Delete

```typescript
// Delete all documents matching a filter
const users = await db.query('db.table("users").filter(r => r.status == "inactive").run()');

await Promise.all(
  users.map(user => db.delete("users", user.id))
);
```

## Transactions

SquirrelDB doesn't currently support multi-document transactions. Each operation is atomic:

- Insert: Atomic
- Update: Atomic
- Delete: Atomic

For operations that need to be coordinated, implement application-level logic:

```typescript
// Example: Transfer credits between users
async function transfer(fromId: string, toId: string, amount: number) {
  // Get both users
  const [fromUsers, toUsers] = await Promise.all([
    db.query(`db.table("users").filter(r => r.id == "${fromId}").run()`),
    db.query(`db.table("users").filter(r => r.id == "${toId}").run()`)
  ]);

  const from = fromUsers[0];
  const to = toUsers[0];

  // Validate
  if (from.data.credits < amount) {
    throw new Error("Insufficient credits");
  }

  // Update both (not atomic across documents)
  await Promise.all([
    db.update("users", fromId, { ...from.data, credits: from.data.credits - amount }),
    db.update("users", toId, { ...to.data, credits: to.data.credits + amount })
  ]);
}
```

## Error Handling

### Insert Errors

```typescript
try {
  await db.insert("users", { name: "Alice" });
} catch (error) {
  if (error.message.includes("duplicate")) {
    // Handle duplicate key (if using custom IDs)
  } else {
    // Handle other errors
  }
}
```

### Update Errors

```typescript
try {
  await db.update("users", "uuid-here", { name: "New Name" });
} catch (error) {
  if (error.message.includes("not found")) {
    // Document doesn't exist
  }
}
```

### Delete Errors

```typescript
try {
  await db.delete("users", "uuid-here");
} catch (error) {
  // Handle error
}
```

## Change Events

All write operations trigger change events for active subscriptions:

| Operation | Change Event Type | Event Data |
|-----------|------------------|------------|
| Insert | `insert` | `{ new: Document }` |
| Update | `update` | `{ old: data, new: Document }` |
| Delete | `delete` | `{ old: Document }` |

See [Subscriptions](./subscriptions.md) for details on receiving these events.

## Best Practices

### Validate Before Insert

```typescript
function validateUser(data: any): boolean {
  return (
    typeof data.name === "string" &&
    typeof data.email === "string" &&
    data.email.includes("@")
  );
}

if (validateUser(userData)) {
  await db.insert("users", userData);
}
```

### Use Meaningful Collection Names

```typescript
// Good
await db.insert("user_sessions", { ... });
await db.insert("order_items", { ... });

// Avoid
await db.insert("data", { ... });
await db.insert("stuff", { ... });
```

### Include Timestamps in Data

While SquirrelDB adds `created_at` and `updated_at`, you might want custom timestamps:

```typescript
await db.insert("events", {
  type: "page_view",
  page: "/home",
  occurred_at: new Date().toISOString(),  // Your timestamp
  user_id: "..."
});
```

## Next Steps

- [Subscriptions](./subscriptions.md) - Real-time change feeds
- [Query Overview](./overview.md) - Query language reference
