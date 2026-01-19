# Elixir SDK

The official Elixir SDK for SquirrelDB provides a GenServer-based client for Elixir 1.14+.

## Installation

Add to your `mix.exs`:

```elixir
defp deps do
  [
    {:squirreldb, "~> 0.0.1"}
  ]
end
```

Then run:

```bash
mix deps.get
```

## Quick Start

```elixir
# Connect to the server
{:ok, db} = SquirrelDB.connect("localhost:8080")

# Insert a document
{:ok, user} = SquirrelDB.insert(db, "users", %{name: "Alice", age: 30})
IO.puts("Created: #{user.id}")

# Query documents
{:ok, users} = SquirrelDB.query(db, ~s|db.table("users").run()|)
IO.inspect(users, label: "Users")

# Close connection
SquirrelDB.close(db)
```

## Connection

### Basic Connection

```elixir
# Returns {:ok, pid} or {:error, reason}
{:ok, db} = SquirrelDB.connect("localhost:8080")
```

### URL Formats

```elixir
# Without prefix
{:ok, db} = SquirrelDB.connect("localhost:8080")

# With ws:// prefix
{:ok, db} = SquirrelDB.connect("ws://localhost:8080")

# With wss:// for secure connections
{:ok, db} = SquirrelDB.connect("wss://db.example.com")
```

### Supervision

Add to your supervision tree:

```elixir
# In your Application module
def start(_type, _args) do
  children = [
    {SquirrelDB, url: "localhost:8080", name: MyApp.DB}
  ]

  opts = [strategy: :one_for_one, name: MyApp.Supervisor]
  Supervisor.start_link(children, opts)
end
```

Then use the named process:

```elixir
{:ok, users} = SquirrelDB.query(MyApp.DB, ~s|db.table("users").run()|)
```

## API Reference

### `query(client, query) -> {:ok, list} | {:error, reason}`

Execute a query and return results.

```elixir
# Basic query
{:ok, users} = SquirrelDB.query(db, ~s|db.table("users").run()|)

# With filter
{:ok, active_users} = SquirrelDB.query(
  db,
  ~s|db.table("users").filter(r => r.status == "active").run()|
)

# With ordering and limit
{:ok, top_users} = SquirrelDB.query(
  db,
  ~s|db.table("users").orderBy("score", "desc").limit(10).run()|
)
```

### `insert(client, collection, data) -> {:ok, Document.t()} | {:error, reason}`

Insert a new document.

```elixir
{:ok, user} = SquirrelDB.insert(db, "users", %{
  name: "Alice",
  email: "alice@example.com",
  age: 30
})

IO.puts(user.id)          # UUID
IO.puts(user.collection)  # "users"
IO.inspect(user.data)     # %{"name" => "Alice", ...}
IO.puts(user.created_at)  # ISO timestamp
IO.puts(user.updated_at)  # ISO timestamp
```

### `update(client, collection, document_id, data) -> {:ok, Document.t()} | {:error, reason}`

Update an existing document.

```elixir
{:ok, updated} = SquirrelDB.update(db, "users", "uuid-here", %{
  name: "Alice Smith",
  email: "alice@example.com",
  age: 31
})
```

### `delete(client, collection, document_id) -> {:ok, Document.t()} | {:error, reason}`

Delete a document by ID.

```elixir
{:ok, deleted} = SquirrelDB.delete(db, "users", "uuid-here")
IO.puts("Deleted: #{deleted.id}")
```

### `list_collections(client) -> {:ok, list} | {:error, reason}`

List all collections.

```elixir
{:ok, collections} = SquirrelDB.list_collections(db)
IO.inspect(collections)  # ["users", "posts", "comments"]
```

### `subscribe(client, query, callback) -> {:ok, :subscribed} | {:error, reason}`

Subscribe to changes.

```elixir
callback = fn change ->
  case change.type do
    :initial -> IO.inspect(change.document, label: "Existing")
    :insert -> IO.inspect(change.new, label: "Inserted")
    :update -> IO.inspect({change.old, change.new}, label: "Updated")
    :delete -> IO.inspect(change.old, label: "Deleted")
  end
end

{:ok, :subscribed} = SquirrelDB.subscribe(
  db,
  ~s|db.table("users").changes()|,
  callback
)
```

### `ping(client) -> :ok | {:error, reason}`

Check server connectivity.

```elixir
:ok = SquirrelDB.ping(db)
```

### `close(client) -> :ok`

Close the connection.

```elixir
SquirrelDB.close(db)
```

## Types

### Document

```elixir
defmodule SquirrelDB.Document do
  defstruct [:id, :collection, :data, :created_at, :updated_at]
end
```

### ChangeEvent

```elixir
defmodule SquirrelDB.ChangeEvent do
  defstruct [:type, :document, :new, :old]
  # type is :initial | :insert | :update | :delete
end
```

## Examples

### CRUD Operations

```elixir
{:ok, db} = SquirrelDB.connect("localhost:8080")

# Create
{:ok, user} = SquirrelDB.insert(db, "users", %{
  name: "Alice",
  email: "alice@example.com"
})

# Read
{:ok, users} = SquirrelDB.query(db, ~s|db.table("users").run()|)

# Update
{:ok, _} = SquirrelDB.update(db, "users", user.id, %{
  name: "Alice Smith",
  email: "alice@example.com"
})

# Delete
{:ok, _} = SquirrelDB.delete(db, "users", user.id)

SquirrelDB.close(db)
```

### Real-time Updates with GenServer

```elixir
defmodule MyApp.UserTracker do
  use GenServer

  def start_link(opts) do
    GenServer.start_link(__MODULE__, opts, name: __MODULE__)
  end

  def get_users do
    GenServer.call(__MODULE__, :get_users)
  end

  @impl true
  def init(_opts) do
    {:ok, db} = SquirrelDB.connect("localhost:8080")

    callback = fn change ->
      GenServer.cast(__MODULE__, {:change, change})
    end

    {:ok, _} = SquirrelDB.subscribe(
      db,
      ~s|db.table("users").changes()|,
      callback
    )

    {:ok, %{db: db, users: %{}}}
  end

  @impl true
  def handle_call(:get_users, _from, state) do
    {:reply, Map.values(state.users), state}
  end

  @impl true
  def handle_cast({:change, change}, state) do
    users =
      case change.type do
        type when type in [:initial, :insert] ->
          doc = if type == :initial, do: change.document, else: change.new
          Map.put(state.users, doc.id, doc.data)

        :update ->
          Map.put(state.users, change.new.id, change.new.data)

        :delete ->
          Map.delete(state.users, change.old.id)
      end

    {:noreply, %{state | users: users}}
  end
end
```

### With Phoenix

```elixir
# lib/my_app/application.ex
def start(_type, _args) do
  children = [
    MyAppWeb.Endpoint,
    {SquirrelDB, url: "localhost:8080", name: MyApp.DB}
  ]

  Supervisor.start_link(children, strategy: :one_for_one)
end
```

```elixir
# lib/my_app_web/controllers/user_controller.ex
defmodule MyAppWeb.UserController do
  use MyAppWeb, :controller

  def index(conn, _params) do
    {:ok, users} = SquirrelDB.query(MyApp.DB, ~s|db.table("users").run()|)
    json(conn, Enum.map(users, & &1.data))
  end

  def create(conn, %{"user" => user_params}) do
    {:ok, user} = SquirrelDB.insert(MyApp.DB, "users", user_params)
    conn
    |> put_status(:created)
    |> json(user.data)
  end

  def delete(conn, %{"id" => id}) do
    {:ok, _} = SquirrelDB.delete(MyApp.DB, "users", id)
    send_resp(conn, :no_content, "")
  end
end
```

### Phoenix LiveView with Subscriptions

```elixir
defmodule MyAppWeb.UsersLive do
  use MyAppWeb, :live_view

  @impl true
  def mount(_params, _session, socket) do
    if connected?(socket) do
      callback = fn change ->
        send(self(), {:user_change, change})
      end

      {:ok, _} = SquirrelDB.subscribe(
        MyApp.DB,
        ~s|db.table("users").changes()|,
        callback
      )
    end

    {:ok, assign(socket, users: %{})}
  end

  @impl true
  def handle_info({:user_change, change}, socket) do
    users =
      case change.type do
        type when type in [:initial, :insert] ->
          doc = if type == :initial, do: change.document, else: change.new
          Map.put(socket.assigns.users, doc.id, doc.data)

        :update ->
          Map.put(socket.assigns.users, change.new.id, change.new.data)

        :delete ->
          Map.delete(socket.assigns.users, change.old.id)
      end

    {:noreply, assign(socket, users: users)}
  end

  @impl true
  def render(assigns) do
    ~H"""
    <ul>
      <%= for {_id, user} <- @users do %>
        <li><%= user["name"] %></li>
      <% end %>
    </ul>
    """
  end
end
```

## Error Handling

```elixir
case SquirrelDB.connect("localhost:8080") do
  {:ok, db} ->
    case SquirrelDB.query(db, ~s|db.table("users").run()|) do
      {:ok, users} -> IO.inspect(users)
      {:error, reason} -> IO.puts("Query error: #{reason}")
    end

  {:error, reason} ->
    IO.puts("Connection error: #{reason}")
end
```

## Testing with ExUnit

```elixir
defmodule SquirrelDBTest do
  use ExUnit.Case, async: false

  setup do
    {:ok, db} = SquirrelDB.connect("localhost:8080")
    on_exit(fn -> SquirrelDB.close(db) end)
    %{db: db}
  end

  test "insert and query", %{db: db} do
    {:ok, user} = SquirrelDB.insert(db, "test_users", %{name: "Test"})
    assert user.id != nil

    {:ok, users} = SquirrelDB.query(db, ~s|db.table("test_users").run()|)
    assert length(users) > 0
  end
end
```

## OTP Patterns

The SquirrelDB client is a GenServer, so it integrates naturally with OTP:

```elixir
# Start under a supervisor
children = [
  {SquirrelDB, url: "localhost:8080", name: MyApp.DB}
]

# The client will restart if it crashes
Supervisor.start_link(children, strategy: :one_for_one)
```
