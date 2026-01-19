# TypeScript/JavaScript SDK

The official TypeScript SDK for SquirrelDB provides a fully typed client for Node.js, Bun, Deno, and browsers.

## Installation

```bash
# npm
npm install squirreldb

# bun
bun add squirreldb

# yarn
yarn add squirreldb

# pnpm
pnpm add squirreldb
```

## Quick Start

```typescript
import { SquirrelDB } from "squirreldb";

// Connect to the server
const db = await SquirrelDB.connect("localhost:8080");

// Insert a document
const user = await db.insert("users", { name: "Alice", age: 30 });
console.log("Created:", user.id);

// Query documents
const users = await db.query('db.table("users").run()');
console.log("Users:", users);

// Close connection
db.close();
```

## Connection

### Basic Connection

```typescript
import { SquirrelDB, connect } from "squirreldb";

// Using class method
const db = await SquirrelDB.connect("localhost:8080");

// Using convenience function
const db = await connect("localhost:8080");
```

### Connection Options

```typescript
const db = await SquirrelDB.connect("localhost:8080", {
  // Auto-reconnect on disconnect (default: true)
  reconnect: true,

  // Max reconnection attempts (default: 10)
  maxReconnectAttempts: 10,

  // Base delay between reconnects in ms (default: 1000)
  reconnectDelay: 1000,
});
```

### URL Formats

```typescript
// Without prefix
const db = await connect("localhost:8080");

// With ws:// prefix
const db = await connect("ws://localhost:8080");

// With wss:// for secure connections
const db = await connect("wss://db.example.com");
```

## API Reference

### `query<T>(query: string): Promise<T[]>`

Execute a query and return results.

```typescript
// Basic query
const users = await db.query('db.table("users").run()');

// With filter
const activeUsers = await db.query(
  'db.table("users").filter(r => r.status == "active").run()'
);

// With ordering and limit
const topUsers = await db.query(
  'db.table("users").orderBy("score", "desc").limit(10).run()'
);

// Type parameter for better typing
interface User {
  name: string;
  age: number;
}
const users = await db.query<User>('db.table("users").run()');
```

### `insert<T>(collection: string, data: T): Promise<Document<T>>`

Insert a new document.

```typescript
const user = await db.insert("users", {
  name: "Alice",
  email: "alice@example.com",
  age: 30,
});

console.log(user.id);         // UUID
console.log(user.collection); // "users"
console.log(user.data);       // { name: "Alice", ... }
console.log(user.created_at); // ISO timestamp
console.log(user.updated_at); // ISO timestamp
```

### `update<T>(collection: string, id: string, data: T): Promise<Document<T>>`

Update an existing document.

```typescript
const updated = await db.update("users", "uuid-here", {
  name: "Alice Smith",
  email: "alice@example.com",
  age: 31,
});
```

### `delete(collection: string, id: string): Promise<Document>`

Delete a document by ID.

```typescript
const deleted = await db.delete("users", "uuid-here");
console.log("Deleted:", deleted.id);
```

### `listCollections(): Promise<string[]>`

List all collections.

```typescript
const collections = await db.listCollections();
console.log(collections); // ["users", "posts", "comments"]
```

### `subscribe(query: string, callback: ChangeCallback): Promise<string>`

Subscribe to changes.

```typescript
const subId = await db.subscribe(
  'db.table("users").changes()',
  (change) => {
    switch (change.type) {
      case "initial":
        console.log("Existing:", change.document);
        break;
      case "insert":
        console.log("Inserted:", change.new);
        break;
      case "update":
        console.log("Updated:", change.old, "->", change.new);
        break;
      case "delete":
        console.log("Deleted:", change.old);
        break;
    }
  }
);
```

### `unsubscribe(subscriptionId: string): Promise<void>`

Unsubscribe from changes.

```typescript
await db.unsubscribe(subId);
```

### `ping(): Promise<void>`

Check server connectivity.

```typescript
await db.ping();
```

### `close(): void`

Close the connection.

```typescript
db.close();
```

## Types

### Document

```typescript
interface Document<T = Record<string, unknown>> {
  id: string;
  collection: string;
  data: T;
  created_at: string;
  updated_at: string;
}
```

### ChangeEvent

```typescript
type ChangeEvent =
  | { type: "initial"; document: Document }
  | { type: "insert"; new: Document }
  | { type: "update"; old: unknown; new: Document }
  | { type: "delete"; old: Document };
```

### ConnectOptions

```typescript
interface ConnectOptions {
  reconnect?: boolean;
  maxReconnectAttempts?: number;
  reconnectDelay?: number;
}
```

## Examples

### CRUD Operations

```typescript
import { SquirrelDB } from "squirreldb";

async function main() {
  const db = await SquirrelDB.connect("localhost:8080");

  // Create
  const user = await db.insert("users", {
    name: "Alice",
    email: "alice@example.com",
  });

  // Read
  const users = await db.query('db.table("users").run()');

  // Update
  await db.update("users", user.id, {
    name: "Alice Smith",
    email: "alice@example.com",
  });

  // Delete
  await db.delete("users", user.id);

  db.close();
}
```

### Real-time Updates

```typescript
import { SquirrelDB } from "squirreldb";

async function main() {
  const db = await SquirrelDB.connect("localhost:8080");

  // Track users in memory
  const users = new Map();

  const subId = await db.subscribe(
    'db.table("users").changes()',
    (change) => {
      if (change.type === "initial" || change.type === "insert") {
        const doc = change.type === "initial" ? change.document : change.new;
        users.set(doc.id, doc.data);
      } else if (change.type === "update") {
        users.set(change.new.id, change.new.data);
      } else if (change.type === "delete") {
        users.delete(change.old.id);
      }

      console.log("Current users:", Array.from(users.values()));
    }
  );

  // Keep running
  process.on("SIGINT", async () => {
    await db.unsubscribe(subId);
    db.close();
    process.exit(0);
  });
}
```

### With React

```tsx
import { useEffect, useState } from "react";
import { SquirrelDB, Document } from "squirreldb";

function useUsers() {
  const [users, setUsers] = useState<Document[]>([]);
  const [db, setDb] = useState<SquirrelDB | null>(null);

  useEffect(() => {
    let client: SquirrelDB;
    let subId: string;

    (async () => {
      client = await SquirrelDB.connect("localhost:8080");
      setDb(client);

      subId = await client.subscribe(
        'db.table("users").changes()',
        (change) => {
          setUsers((prev) => {
            const next = [...prev];
            // Handle change...
            return next;
          });
        }
      );
    })();

    return () => {
      if (subId && client) client.unsubscribe(subId);
      client?.close();
    };
  }, []);

  return { users, db };
}
```

## Error Handling

```typescript
try {
  const db = await SquirrelDB.connect("localhost:8080");
  const users = await db.query('db.table("users").run()');
} catch (error) {
  if (error.message.includes("Failed to connect")) {
    console.error("Cannot reach server");
  } else {
    console.error("Query error:", error.message);
  }
}
```

## Browser Usage

The SDK works in browsers with WebSocket support:

```html
<script type="module">
  import { SquirrelDB } from "https://unpkg.com/squirreldb/dist/index.js";

  const db = await SquirrelDB.connect("localhost:8080");
  const users = await db.query('db.table("users").run()');
  console.log(users);
</script>
```

## Testing

```typescript
import { describe, test, expect, beforeAll, afterAll } from "bun:test";
import { SquirrelDB } from "squirreldb";

describe("My App", () => {
  let db: SquirrelDB;

  beforeAll(async () => {
    db = await SquirrelDB.connect("localhost:8080");
  });

  afterAll(() => {
    db.close();
  });

  test("insert and query", async () => {
    const user = await db.insert("test_users", { name: "Test" });
    expect(user.id).toBeDefined();

    const users = await db.query('db.table("test_users").run()');
    expect(users.length).toBeGreaterThan(0);
  });
});
```
