# Admin UI

SquirrelDB includes a built-in web administration interface for managing your database.

## Accessing the Admin UI

The Admin UI runs on port 8081 by default:

```
http://localhost:8081
```

Configure the port in your config file:

```yaml
server:
  admin_port: 8081
```

## Authentication

When authentication is enabled, the Admin UI requires a valid token to access.

### First-Time Setup

When you first enable authentication with no tokens configured:

1. Navigate to the Admin UI
2. You'll be automatically redirected to `/setup`
3. Enter a name for your admin token (e.g., "Admin")
4. Click **Create Admin Token**
5. **Important**: Copy the displayed token immediately - it will only be shown once!
6. Click **Continue to Login**
7. Enter your token on the login page

### Login

After setup, accessing the Admin UI requires authentication:

1. Navigate to the Admin UI
2. You'll be redirected to `/login` if not authenticated
3. Enter your admin token (`sqrl_...`)
4. Click **Sign In**

Your token is stored in browser localStorage for convenience. You'll stay logged in until you explicitly log out or clear browser data.

### Logout

Click the logout button in the settings page or clear your browser's localStorage.

### Bypassing Login (Development)

For development, you can disable authentication:

```yaml
auth:
  enabled: false
```

Or use a static admin token:

```yaml
auth:
  enabled: true
  admin_token: "my-dev-token"
```

## Dashboard

The dashboard provides an overview of your database:

- **Tables**: Number of collections
- **Documents**: Total document count across all collections
- **Backend**: Current backend (PostgreSQL or SQLite)
- **Uptime**: Server uptime

### Collections Table

The dashboard displays a table of all collections with:

- Collection name
- Document count
- Actions (View, Drop)

## Tables Browser

The Tables Browser allows you to explore your data:

### Collection List

The left sidebar shows all collections. Click a collection to view its documents.

### Document View

Documents are displayed as formatted JSON with syntax highlighting:

- Keys in one color
- String values in another
- Numbers, booleans, and null distinctly colored

### Actions

- **Refresh**: Reload the current collection
- **View**: Navigate to a collection from the dashboard
- **Drop**: Delete all documents in a collection (with confirmation)

## Data Explorer

The Data Explorer lets you run queries directly in the browser:

### Query Input

Enter queries in the text area:

```javascript
db.table("users").run()
db.table("users").filter(r => r.age > 25).run()
db.table("products").orderBy("price", "desc").limit(10).run()
```

### Running Queries

- Click **Run** button
- Or press `Ctrl+Enter` (Cmd+Enter on Mac)

### Results

Query results are displayed as syntax-highlighted JSON. Query execution time is shown below the results.

### Error Handling

Invalid queries show error messages in red:

```
Error: Parse error at line 1
```

## REST API

The Admin UI is powered by a REST API you can also use directly:

### Status

```
GET /api/status
```

Returns server status:

```json
{
  "name": "SquirrelDB",
  "version": "0.0.1",
  "backend": "Postgres",
  "uptime_secs": 3600
}
```

### List Collections

```
GET /api/collections
```

Returns all collections with document counts:

```json
[
  { "name": "users", "count": 150 },
  { "name": "posts", "count": 500 }
]
```

### Get Collection Documents

```
GET /api/collections/{name}?limit=100&offset=0
```

Returns documents in a collection:

```json
[
  {
    "id": "...",
    "collection": "users",
    "data": { "name": "Alice" },
    "created_at": "...",
    "updated_at": "..."
  }
]
```

### Drop Collection

```
DELETE /api/collections/{name}
```

Deletes all documents in a collection:

```json
{
  "deleted": 150
}
```

### Insert Document

```
POST /api/collections/{name}/documents
Content-Type: application/json

{ "name": "Alice", "age": 30 }
```

Returns the created document.

### Get Document

```
GET /api/collections/{name}/documents/{id}
```

Returns a single document by ID.

### Delete Document

```
DELETE /api/collections/{name}/documents/{id}
```

Deletes a document by ID.

### Execute Query

```
POST /api/query
Content-Type: application/json

{ "query": "db.table(\"users\").run()" }
```

Returns query results.

## Health Endpoints

### Liveness

```
GET /health
```

Returns `200 OK` if the server is running.

### Readiness

```
GET /ready
```

Returns `200 OK` if the database is accessible, `503 Service Unavailable` otherwise.

## Security Considerations

The Admin UI provides full access to your data and server settings.

### Authentication vs Network Security

- **Authentication** protects the Admin UI (dashboard, settings, token management)
- **Data API** (REST, WebSocket) is **not protected** by authentication

For production deployments, use both:
1. Enable authentication to secure admin access
2. Use network-level security to restrict data API access

### Restrict Access

Don't expose the admin port publicly. Use:

- Firewall rules
- VPN
- SSH tunneling
- IP whitelisting in a reverse proxy

### nginx Example

```nginx
server {
    listen 8081;

    # Only allow internal IPs
    allow 10.0.0.0/8;
    allow 192.168.0.0/16;
    deny all;

    location / {
        proxy_pass http://squirreldb:8081;
    }
}
```

### SSH Tunnel

Access the admin UI securely via SSH:

```bash
ssh -L 8081:localhost:8081 user@server
```

Then open http://localhost:8081 in your browser.

## Storage Browser

When storage is enabled, the Admin UI includes a file browser for managing S3-compatible objects.

### Accessing Storage

1. Enable storage in your configuration:
   ```yaml
   features:
     storage: true
   ```

2. Navigate to **Storage** in the sidebar

3. Create a bucket or select an existing one

4. Click **View** to open the file browser

### Browser Interface

The file browser displays:

| Column | Description |
|--------|-------------|
| Checkbox | Select files for bulk operations |
| Icon | Folder or file type icon |
| Name | Object key/filename |
| Size | File size (folders show "-") |
| Modified | Last modification date |
| Actions | Download, Delete buttons |

### Navigation

- **Click folders** to navigate into them
- **Breadcrumbs** at the top show current path
- **Click breadcrumb segments** to navigate up

### Uploading Files

1. Click **Upload** button in the toolbar
2. Either:
   - **Drag and drop** files into the drop zone
   - Click **Choose Files** to open file picker
3. Review selected files in the list
4. Click **Upload** to start
5. Progress bar shows upload status

Features:
- Multiple file upload
- Drag-and-drop support
- Upload progress tracking
- Remove files before uploading

### Downloading Files

Click the **download icon** on any file row. The file downloads directly to your browser.

### Deleting Files

**Single file:**
- Click the **delete icon** on the file row
- Confirm deletion in the modal

**Multiple files:**
1. Check the boxes next to files to delete
2. Click **Delete Selected** in the toolbar
3. Confirm deletion

### File Preview

Click on a file name to preview:

| File Type | Preview |
|-----------|---------|
| Images (PNG, JPG, GIF, WebP, SVG) | Inline image display |
| Text files (.txt, .md, .log) | Syntax-highlighted text |
| JSON files | Formatted JSON viewer |
| Other files | Download prompt |

### Keyboard Shortcuts

| Key | Action |
|-----|--------|
| Enter | Open selected folder/preview file |
| Backspace | Navigate to parent folder |
| Delete | Delete selected files |

## Customization

The Admin UI is built with vanilla HTML, CSS, and JavaScript. The styles use CSS variables for theming:

```css
:root {
  --bg-primary: #1e1e2e;
  --bg-secondary: #2a2a3e;
  --text-primary: #e0e0e0;
  --text-secondary: #888;
  --accent: #ff6b6b;
  --accent-secondary: #4ecdc4;
  --border: #3a3a4e;
}
```

## Troubleshooting

### Admin UI Not Loading

- Check the admin port is accessible
- Verify no firewall blocking port 8081
- Check server logs for errors

### Queries Not Running

- Check browser console for JavaScript errors
- Verify WebSocket connection to server
- Try refreshing the page

### Slow Performance

- Large collections may take time to load
- Use filters and limits in queries
- Check database performance
