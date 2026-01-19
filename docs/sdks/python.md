# Python SDK

The official Python SDK for SquirrelDB provides an async client for Python 3.9+.

## Installation

```bash
pip install squirreldb
```

## Quick Start

```python
import asyncio
from squirreldb import connect

async def main():
    # Connect to the server
    db = await connect("localhost:8080")

    # Insert a document
    user = await db.insert("users", {"name": "Alice", "age": 30})
    print(f"Created: {user.id}")

    # Query documents
    users = await db.query('db.table("users").run()')
    print(f"Users: {users}")

    # Close connection
    await db.close()

asyncio.run(main())
```

## Connection

### Basic Connection

```python
from squirreldb import SquirrelDB, connect

# Using class method
db = await SquirrelDB.connect("localhost:8080")

# Using convenience function
db = await connect("localhost:8080")
```

### Connection Options

```python
db = await connect(
    "localhost:8080",
    reconnect=True,              # Auto-reconnect (default: True)
    max_reconnect_attempts=10,   # Max retries (default: 10)
    reconnect_delay=1.0,         # Base delay in seconds (default: 1.0)
)
```

### URL Formats

```python
# Without prefix
db = await connect("localhost:8080")

# With ws:// prefix
db = await connect("ws://localhost:8080")

# With wss:// for secure connections
db = await connect("wss://db.example.com")
```

## API Reference

### `query(q: str) -> list[Document]`

Execute a query and return results.

```python
# Basic query
users = await db.query('db.table("users").run()')

# With filter
active_users = await db.query(
    'db.table("users").filter(r => r.status == "active").run()'
)

# With ordering and limit
top_users = await db.query(
    'db.table("users").orderBy("score", "desc").limit(10).run()'
)
```

### `insert(collection: str, data: dict) -> Document`

Insert a new document.

```python
user = await db.insert("users", {
    "name": "Alice",
    "email": "alice@example.com",
    "age": 30,
})

print(user.id)          # UUID
print(user.collection)  # "users"
print(user.data)        # {"name": "Alice", ...}
print(user.created_at)  # ISO timestamp
print(user.updated_at)  # ISO timestamp
```

### `update(collection: str, document_id: str, data: dict) -> Document`

Update an existing document.

```python
updated = await db.update("users", "uuid-here", {
    "name": "Alice Smith",
    "email": "alice@example.com",
    "age": 31,
})
```

### `delete(collection: str, document_id: str) -> Document`

Delete a document by ID.

```python
deleted = await db.delete("users", "uuid-here")
print(f"Deleted: {deleted.id}")
```

### `list_collections() -> list[str]`

List all collections.

```python
collections = await db.list_collections()
print(collections)  # ["users", "posts", "comments"]
```

### `subscribe(q: str, callback: Callable) -> str`

Subscribe to changes.

```python
def on_change(change):
    if change.type == "initial":
        print(f"Existing: {change.document}")
    elif change.type == "insert":
        print(f"Inserted: {change.new}")
    elif change.type == "update":
        print(f"Updated: {change.old} -> {change.new}")
    elif change.type == "delete":
        print(f"Deleted: {change.old}")

sub_id = await db.subscribe('db.table("users").changes()', on_change)
```

### `unsubscribe(subscription_id: str) -> None`

Unsubscribe from changes.

```python
await db.unsubscribe(sub_id)
```

### `ping() -> None`

Check server connectivity.

```python
await db.ping()
```

### `close() -> None`

Close the connection.

```python
await db.close()
```

## Types

### Document

```python
@dataclass
class Document:
    id: str
    collection: str
    data: dict[str, Any]
    created_at: str
    updated_at: str
```

### ChangeEvent

```python
@dataclass
class ChangeEvent:
    type: str  # "initial", "insert", "update", "delete"
    document: Optional[Document] = None  # For "initial"
    new: Optional[Document] = None       # For "insert", "update"
    old: Optional[dict | Document] = None  # For "update", "delete"
```

## Examples

### CRUD Operations

```python
import asyncio
from squirreldb import connect

async def main():
    db = await connect("localhost:8080")

    # Create
    user = await db.insert("users", {
        "name": "Alice",
        "email": "alice@example.com",
    })

    # Read
    users = await db.query('db.table("users").run()')

    # Update
    await db.update("users", user.id, {
        "name": "Alice Smith",
        "email": "alice@example.com",
    })

    # Delete
    await db.delete("users", user.id)

    await db.close()

asyncio.run(main())
```

### Real-time Updates

```python
import asyncio
from squirreldb import connect

async def main():
    db = await connect("localhost:8080")
    users = {}

    def on_change(change):
        if change.type in ("initial", "insert"):
            doc = change.document if change.type == "initial" else change.new
            users[doc.id] = doc.data
        elif change.type == "update":
            users[change.new.id] = change.new.data
        elif change.type == "delete":
            users.pop(change.old.id, None)

        print(f"Current users: {list(users.values())}")

    sub_id = await db.subscribe('db.table("users").changes()', on_change)

    # Keep running
    try:
        while True:
            await asyncio.sleep(1)
    except KeyboardInterrupt:
        await db.unsubscribe(sub_id)
        await db.close()

asyncio.run(main())
```

### With FastAPI

```python
from fastapi import FastAPI, WebSocket
from squirreldb import connect, SquirrelDB

app = FastAPI()
db: SquirrelDB = None

@app.on_event("startup")
async def startup():
    global db
    db = await connect("localhost:8080")

@app.on_event("shutdown")
async def shutdown():
    await db.close()

@app.get("/users")
async def get_users():
    users = await db.query('db.table("users").run()')
    return [u.data for u in users]

@app.post("/users")
async def create_user(name: str, email: str):
    user = await db.insert("users", {"name": name, "email": email})
    return user.data
```

### With Django (async views)

```python
from django.http import JsonResponse
from squirreldb import connect

async def get_users(request):
    db = await connect("localhost:8080")
    try:
        users = await db.query('db.table("users").run()')
        return JsonResponse({"users": [u.data for u in users]})
    finally:
        await db.close()
```

## Error Handling

```python
try:
    db = await connect("localhost:8080")
    users = await db.query('db.table("users").run()')
except ConnectionError as e:
    print(f"Cannot reach server: {e}")
except Exception as e:
    print(f"Query error: {e}")
```

## Testing with pytest

```python
import pytest
from squirreldb import connect

@pytest.fixture
async def db():
    client = await connect("localhost:8080")
    yield client
    await client.close()

@pytest.mark.asyncio
async def test_insert_and_query(db):
    user = await db.insert("test_users", {"name": "Test"})
    assert user.id is not None

    users = await db.query('db.table("test_users").run()')
    assert len(users) > 0
```

## Context Manager

```python
from contextlib import asynccontextmanager
from squirreldb import connect

@asynccontextmanager
async def get_db():
    db = await connect("localhost:8080")
    try:
        yield db
    finally:
        await db.close()

async def main():
    async with get_db() as db:
        users = await db.query('db.table("users").run()')
        print(users)
```
