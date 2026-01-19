# Subscriptions (Change Feeds)

SquirrelDB's real-time subscriptions let you receive instant notifications when documents change.

## Overview

Subscriptions allow you to:
- Watch all changes in a collection
- Watch changes matching a filter
- Receive initial state when subscribing
- Handle inserts, updates, and deletes

## Basic Usage

### Subscribe to All Changes

```typescript
// TypeScript
const subId = await db.subscribe(
  'db.table("users").changes()',
  (change) => {
    console.log("Change type:", change.type);
    console.log("Change data:", change);
  }
);
```

```python
# Python
def on_change(change):
    print(f"Change type: {change.type}")

sub_id = await db.subscribe('db.table("users").changes()', on_change)
```

```ruby
# Ruby
sub_id = db.subscribe('db.table("users").changes()') do |change|
  puts "Change type: #{change.type}"
end
```

```elixir
# Elixir
callback = fn change ->
  IO.inspect(change.type, label: "Change type")
end

{:ok, _} = SquirrelDB.subscribe(db, ~s|db.table("users").changes()|, callback)
```

### Subscribe with Filter

Only receive changes for documents matching a filter:

```javascript
// Only active users
db.table("users").filter(r => r.status == "active").changes()

// Products in a category
db.table("products").filter(r => r.category == "electronics").changes()

// High-value orders
db.table("orders").filter(r => r.total > 1000).changes()
```

## Change Events

### Event Types

| Type | When | Fields |
|------|------|--------|
| `initial` | On subscribe (for existing docs) | `document` |
| `insert` | New document created | `new` |
| `update` | Document modified | `old`, `new` |
| `delete` | Document removed | `old` |

### Initial Events

When you subscribe, you receive `initial` events for all existing documents that match:

```typescript
await db.subscribe('db.table("users").changes()', (change) => {
  if (change.type === "initial") {
    // Existing document - build initial state
    console.log("Existing user:", change.document);
  }
});
```

This is useful for building your initial state without a separate query.

### Insert Events

When a new document is created:

```typescript
{
  type: "insert",
  new: {
    id: "...",
    collection: "users",
    data: { name: "Alice", age: 30 },
    created_at: "...",
    updated_at: "..."
  }
}
```

### Update Events

When a document is modified:

```typescript
{
  type: "update",
  old: { name: "Alice", age: 30 },  // Previous data (just the data field)
  new: {
    id: "...",
    collection: "users",
    data: { name: "Alice", age: 31 },  // New full document
    created_at: "...",
    updated_at: "..."
  }
}
```

### Delete Events

When a document is removed:

```typescript
{
  type: "delete",
  old: {
    id: "...",
    collection: "users",
    data: { name: "Alice", age: 30 },
    created_at: "...",
    updated_at: "..."
  }
}
```

## Unsubscribing

Always unsubscribe when done to free resources:

```typescript
// TypeScript
const subId = await db.subscribe('db.table("users").changes()', callback);

// Later...
await db.unsubscribe(subId);
```

```python
# Python
sub_id = await db.subscribe('db.table("users").changes()', callback)

# Later...
await db.unsubscribe(sub_id)
```

## Patterns

### Real-time List

Keep a list synchronized with the database:

```typescript
const users = new Map<string, User>();

await db.subscribe('db.table("users").changes()', (change) => {
  switch (change.type) {
    case "initial":
      users.set(change.document.id, change.document.data);
      break;
    case "insert":
      users.set(change.new.id, change.new.data);
      break;
    case "update":
      users.set(change.new.id, change.new.data);
      break;
    case "delete":
      users.delete(change.old.id);
      break;
  }

  // Update UI
  renderUsers(Array.from(users.values()));
});
```

### Notification System

Show notifications for specific events:

```typescript
await db.subscribe(
  'db.table("notifications").filter(r => r.user_id == "current-user").changes()',
  (change) => {
    if (change.type === "insert") {
      showNotification(change.new.data.message);
    }
  }
);
```

### Activity Feed

Build a real-time activity feed:

```typescript
const activities: Activity[] = [];

await db.subscribe(
  'db.table("activities").changes()',
  (change) => {
    if (change.type === "initial" || change.type === "insert") {
      const doc = change.type === "initial" ? change.document : change.new;
      activities.unshift(doc.data);

      // Keep only last 50
      if (activities.length > 50) {
        activities.pop();
      }

      renderActivityFeed(activities);
    }
  }
);
```

### Presence System

Track online users:

```typescript
const onlineUsers = new Set<string>();

await db.subscribe(
  'db.table("presence").filter(r => r.status == "online").changes()',
  (change) => {
    switch (change.type) {
      case "initial":
      case "insert":
        const doc = change.type === "initial" ? change.document : change.new;
        onlineUsers.add(doc.data.user_id);
        break;
      case "update":
        if (change.new.data.status === "online") {
          onlineUsers.add(change.new.data.user_id);
        } else {
          onlineUsers.delete(change.new.data.user_id);
        }
        break;
      case "delete":
        onlineUsers.delete(change.old.data.user_id);
        break;
    }

    updateOnlineCount(onlineUsers.size);
  }
);
```

## Multiple Subscriptions

You can have multiple active subscriptions:

```typescript
// Subscribe to users
const usersSub = await db.subscribe(
  'db.table("users").changes()',
  handleUserChange
);

// Subscribe to orders
const ordersSub = await db.subscribe(
  'db.table("orders").changes()',
  handleOrderChange
);

// Cleanup
await db.unsubscribe(usersSub);
await db.unsubscribe(ordersSub);
```

## Error Handling

Handle subscription errors:

```typescript
try {
  const subId = await db.subscribe('db.table("users").changes()', callback);
} catch (error) {
  console.error("Subscription failed:", error);
  // Retry or notify user
}
```

## Reconnection

The SDKs handle reconnection automatically. After reconnecting:
1. Active subscriptions are **not** automatically restored
2. You need to re-subscribe after reconnection

```typescript
// Handle reconnection in your app
async function setupSubscriptions() {
  return await db.subscribe('db.table("users").changes()', callback);
}

// Re-setup on connection
db.on("reconnect", async () => {
  await setupSubscriptions();
});
```

## Performance Considerations

### Filter on Server

Filter as much as possible on the server:

```javascript
// Good - server filters
db.table("orders").filter(r => r.user_id == "123").changes()

// Less efficient - client filters everything
db.table("orders").changes()  // Then filter in callback
```

### Limit Subscriptions

Each subscription uses server resources. Consolidate when possible:

```typescript
// Instead of multiple subscriptions
await db.subscribe('db.table("users").filter(r => r.role == "admin").changes()', ...);
await db.subscribe('db.table("users").filter(r => r.role == "user").changes()', ...);

// Use one subscription and filter in callback
await db.subscribe('db.table("users").changes()', (change) => {
  const role = getRole(change);
  if (role === "admin") handleAdmin(change);
  if (role === "user") handleUser(change);
});
```

### Unsubscribe When Not Needed

Always clean up subscriptions:

```typescript
// Component lifecycle (React example)
useEffect(() => {
  let subId: string;

  (async () => {
    subId = await db.subscribe('db.table("users").changes()', setUsers);
  })();

  return () => {
    if (subId) db.unsubscribe(subId);
  };
}, []);
```

## Debugging

Log subscription activity:

```typescript
await db.subscribe('db.table("users").changes()', (change) => {
  console.log(`[${new Date().toISOString()}] ${change.type}:`, change);
  // Handle change...
});
```

## Next Steps

- [Query Overview](./overview.md) - Query language reference
- [Writing Data](./writing.md) - Operations that trigger changes
