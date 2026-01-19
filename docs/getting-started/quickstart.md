# Quick Start

Get up and running with SquirrelDB in 5 minutes.

## 1. Start the Server

The fastest way to start is with SQLite:

```bash
sqrld
```

This creates a `squirreldb.db` file and starts:
- WebSocket server on port 8080
- Admin UI on port 8081

## 2. Open the Admin UI

Navigate to http://localhost:8081 in your browser. You'll see the dashboard showing:

- Number of tables (collections)
- Total documents
- Backend type
- Server uptime

## 3. Use the Data Explorer

Click on **Data Explorer** in the sidebar. Try these queries:

```javascript
// Create some users
db.table("users").insert({ name: "Alice", age: 30 }).run()
db.table("users").insert({ name: "Bob", age: 25 }).run()
db.table("users").insert({ name: "Charlie", age: 35 }).run()

// Query all users
db.table("users").run()

// Filter users
db.table("users").filter(r => r.age > 28).run()

// Order by age
db.table("users").orderBy("age").run()

// Limit results
db.table("users").limit(2).run()
```

## 4. Connect with an SDK

### TypeScript

```typescript
import { SquirrelDB } from "squirreldb";

async function main() {
  // Connect
  const db = await SquirrelDB.connect("localhost:8080");

  // Insert
  const alice = await db.insert("users", { name: "Alice", age: 30 });
  console.log("Created:", alice);

  // Query
  const users = await db.query('db.table("users").run()');
  console.log("Users:", users);

  // Subscribe to changes
  const subId = await db.subscribe(
    'db.table("users").changes()',
    (change) => {
      console.log("Change:", change.type, change);
    }
  );

  // Insert another user (triggers subscription)
  await db.insert("users", { name: "Bob", age: 25 });

  // Cleanup
  await db.unsubscribe(subId);
  db.close();
}

main();
```

### Python

```python
import asyncio
from squirreldb import connect

async def main():
    # Connect
    db = await connect("localhost:8080")

    # Insert
    alice = await db.insert("users", {"name": "Alice", "age": 30})
    print(f"Created: {alice}")

    # Query
    users = await db.query('db.table("users").run()')
    print(f"Users: {users}")

    # Subscribe to changes
    def on_change(change):
        print(f"Change: {change.type}")

    sub_id = await db.subscribe('db.table("users").changes()', on_change)

    # Insert another user (triggers subscription)
    await db.insert("users", {"name": "Bob", "age": 25})

    # Wait for change to arrive
    await asyncio.sleep(0.1)

    # Cleanup
    await db.unsubscribe(sub_id)
    await db.close()

asyncio.run(main())
```

### Ruby

```ruby
require "squirreldb"

# Connect
db = SquirrelDB.connect("localhost:8080")

# Insert
alice = db.insert("users", { name: "Alice", age: 30 })
puts "Created: #{alice.inspect}"

# Query
users = db.query('db.table("users").run()')
puts "Users: #{users.inspect}"

# Subscribe to changes
sub_id = db.subscribe('db.table("users").changes()') do |change|
  puts "Change: #{change.type}"
end

# Insert another user (triggers subscription)
db.insert("users", { name: "Bob", age: 25 })

# Wait for change to arrive
sleep 0.1

# Cleanup
db.unsubscribe(sub_id)
db.close
```

### Elixir

```elixir
# Connect
{:ok, db} = SquirrelDB.connect("localhost:8080")

# Insert
{:ok, alice} = SquirrelDB.insert(db, "users", %{name: "Alice", age: 30})
IO.inspect(alice, label: "Created")

# Query
{:ok, users} = SquirrelDB.query(db, ~s|db.table("users").run()|)
IO.inspect(users, label: "Users")

# Subscribe to changes
callback = fn change ->
  IO.inspect(change, label: "Change")
end
{:ok, _} = SquirrelDB.subscribe(db, ~s|db.table("users").changes()|, callback)

# Insert another user (triggers subscription)
{:ok, _} = SquirrelDB.insert(db, "users", %{name: "Bob", age: 25})

# Wait for change to arrive
Process.sleep(100)

# Cleanup
SquirrelDB.close(db)
```

## 5. Use the CLI

The `sqrl` command provides a REPL for interactive queries:

```bash
$ sqrl
SquirrelDB Shell (v0.0.1)
Connected to localhost:8080
Type .help for commands

squirreldb> db.table("users").run()
┌──────────────────────────────────────┬────────────┬─────────────────────────┐
│ id                                   │ collection │ data                    │
├──────────────────────────────────────┼────────────┼─────────────────────────┤
│ a1b2c3d4-e5f6-7890-abcd-ef1234567890 │ users      │ {"name":"Alice","age":30}│
└──────────────────────────────────────┴────────────┴─────────────────────────┘

squirreldb> .collections
users

squirreldb> .exit
```

## Next Steps

- [Concepts](./concepts.md) - Understand core concepts
- [Query Language](../queries/overview.md) - Learn the full query syntax
- [Subscriptions](../queries/subscriptions.md) - Real-time change feeds
