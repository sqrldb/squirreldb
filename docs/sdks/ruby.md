# Ruby SDK

The official Ruby SDK for SquirrelDB provides a synchronous client for Ruby 3.0+.

## Installation

Add to your Gemfile:

```ruby
gem 'squirreldb'
```

Then run:

```bash
bundle install
```

Or install directly:

```bash
gem install squirreldb
```

## Quick Start

```ruby
require "squirreldb"

# Connect to the server
db = SquirrelDB.connect("localhost:8080")

# Insert a document
user = db.insert("users", { name: "Alice", age: 30 })
puts "Created: #{user.id}"

# Query documents
users = db.query('db.table("users").run()')
puts "Users: #{users.inspect}"

# Close connection
db.close
```

## Connection

### Basic Connection

```ruby
require "squirreldb"

# Using module method
db = SquirrelDB.connect("localhost:8080")

# Using class directly
db = SquirrelDB::Client.connect("localhost:8080")
```

### Connection Options

```ruby
db = SquirrelDB.connect(
  "localhost:8080",
  reconnect: true,              # Auto-reconnect (default: true)
  max_reconnect_attempts: 10,   # Max retries (default: 10)
  reconnect_delay: 1.0          # Base delay in seconds (default: 1.0)
)
```

### URL Formats

```ruby
# Without prefix
db = SquirrelDB.connect("localhost:8080")

# With ws:// prefix
db = SquirrelDB.connect("ws://localhost:8080")

# With wss:// for secure connections
db = SquirrelDB.connect("wss://db.example.com")
```

## API Reference

### `query(q) -> Array<Document>`

Execute a query and return results.

```ruby
# Basic query
users = db.query('db.table("users").run()')

# With filter
active_users = db.query(
  'db.table("users").filter(r => r.status == "active").run()'
)

# With ordering and limit
top_users = db.query(
  'db.table("users").orderBy("score", "desc").limit(10).run()'
)
```

### `insert(collection, data) -> Document`

Insert a new document.

```ruby
user = db.insert("users", {
  name: "Alice",
  email: "alice@example.com",
  age: 30
})

puts user.id          # UUID
puts user.collection  # "users"
puts user.data        # {"name" => "Alice", ...}
puts user.created_at  # ISO timestamp
puts user.updated_at  # ISO timestamp
```

### `update(collection, document_id, data) -> Document`

Update an existing document.

```ruby
updated = db.update("users", "uuid-here", {
  name: "Alice Smith",
  email: "alice@example.com",
  age: 31
})
```

### `delete(collection, document_id) -> Document`

Delete a document by ID.

```ruby
deleted = db.delete("users", "uuid-here")
puts "Deleted: #{deleted.id}"
```

### `list_collections -> Array<String>`

List all collections.

```ruby
collections = db.list_collections
puts collections  # ["users", "posts", "comments"]
```

### `subscribe(q, &block) -> String`

Subscribe to changes.

```ruby
sub_id = db.subscribe('db.table("users").changes()') do |change|
  case change.type
  when "initial"
    puts "Existing: #{change.document.inspect}"
  when "insert"
    puts "Inserted: #{change.new.inspect}"
  when "update"
    puts "Updated: #{change.old.inspect} -> #{change.new.inspect}"
  when "delete"
    puts "Deleted: #{change.old.inspect}"
  end
end
```

### `unsubscribe(subscription_id) -> nil`

Unsubscribe from changes.

```ruby
db.unsubscribe(sub_id)
```

### `ping -> nil`

Check server connectivity.

```ruby
db.ping
```

### `close -> nil`

Close the connection.

```ruby
db.close
```

## Types

### Document

```ruby
SquirrelDB::Document = Struct.new(
  :id,
  :collection,
  :data,
  :created_at,
  :updated_at,
  keyword_init: true
)
```

### ChangeEvent

```ruby
SquirrelDB::ChangeEvent = Struct.new(
  :type,      # "initial", "insert", "update", "delete"
  :document,  # For "initial"
  :new,       # For "insert", "update"
  :old,       # For "update", "delete"
  keyword_init: true
)
```

## Examples

### CRUD Operations

```ruby
require "squirreldb"

db = SquirrelDB.connect("localhost:8080")

# Create
user = db.insert("users", {
  name: "Alice",
  email: "alice@example.com"
})

# Read
users = db.query('db.table("users").run()')

# Update
db.update("users", user.id, {
  name: "Alice Smith",
  email: "alice@example.com"
})

# Delete
db.delete("users", user.id)

db.close
```

### Real-time Updates

```ruby
require "squirreldb"

db = SquirrelDB.connect("localhost:8080")
users = {}

sub_id = db.subscribe('db.table("users").changes()') do |change|
  case change.type
  when "initial", "insert"
    doc = change.type == "initial" ? change.document : change.new
    users[doc.id] = doc.data
  when "update"
    users[change.new.id] = change.new.data
  when "delete"
    users.delete(change.old.id)
  end

  puts "Current users: #{users.values}"
end

# Keep running
trap("INT") do
  db.unsubscribe(sub_id)
  db.close
  exit
end

sleep
```

### With Rails

```ruby
# config/initializers/squirreldb.rb
require "squirreldb"

SQUIRREL_DB = SquirrelDB.connect(
  ENV.fetch("SQUIRRELDB_URL", "localhost:8080")
)

at_exit { SQUIRREL_DB.close }
```

```ruby
# app/controllers/users_controller.rb
class UsersController < ApplicationController
  def index
    users = SQUIRREL_DB.query('db.table("users").run()')
    render json: users.map(&:data)
  end

  def create
    user = SQUIRREL_DB.insert("users", user_params.to_h)
    render json: user.data, status: :created
  end

  def destroy
    SQUIRREL_DB.delete("users", params[:id])
    head :no_content
  end

  private

  def user_params
    params.require(:user).permit(:name, :email)
  end
end
```

### With Sinatra

```ruby
require "sinatra"
require "squirreldb"
require "json"

configure do
  set :db, SquirrelDB.connect("localhost:8080")
end

get "/users" do
  users = settings.db.query('db.table("users").run()')
  content_type :json
  users.map(&:data).to_json
end

post "/users" do
  data = JSON.parse(request.body.read)
  user = settings.db.insert("users", data)
  content_type :json
  status 201
  user.data.to_json
end
```

## Error Handling

```ruby
begin
  db = SquirrelDB.connect("localhost:8080")
  users = db.query('db.table("users").run()')
rescue SocketError, Errno::ECONNREFUSED => e
  puts "Cannot reach server: #{e.message}"
rescue RuntimeError => e
  puts "Query error: #{e.message}"
end
```

## Testing with Minitest

```ruby
require "minitest/autorun"
require "squirreldb"

class SquirrelDBTest < Minitest::Test
  def setup
    @db = SquirrelDB.connect("localhost:8080")
  end

  def teardown
    @db.close
  end

  def test_insert_and_query
    user = @db.insert("test_users", { name: "Test" })
    refute_nil user.id

    users = @db.query('db.table("test_users").run()')
    refute_empty users
  end
end
```

## Testing with RSpec

```ruby
require "squirreldb"

RSpec.describe "SquirrelDB" do
  let(:db) { SquirrelDB.connect("localhost:8080") }

  after { db.close }

  it "inserts and queries documents" do
    user = db.insert("test_users", { name: "Test" })
    expect(user.id).not_to be_nil

    users = db.query('db.table("test_users").run()')
    expect(users).not_to be_empty
  end
end
```

## Thread Safety

The Ruby client is thread-safe. You can share a single connection across threads:

```ruby
db = SquirrelDB.connect("localhost:8080")

threads = 10.times.map do |i|
  Thread.new do
    db.insert("items", { thread: i, value: rand(100) })
  end
end

threads.each(&:join)
db.close
```
