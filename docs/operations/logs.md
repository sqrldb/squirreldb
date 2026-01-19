# Live Logs

The Live Logs feature provides real-time streaming of server logs directly in your browser.

## Accessing Live Logs

1. Open the Admin UI at `http://localhost:8081`
2. Click **Logs** in the sidebar

## Interface

### Status Bar

The status bar shows:

- **Connection Status**: Green dot = connected, red = disconnected
- **Connect/Disconnect Buttons**: Control the log stream
- **Log Count**: Total logs received

### Controls

- **Auto-scroll**: Toggle to follow new logs automatically
- **Level Filter**: Filter by log level (All, Error, Warn, Info, Debug, Trace)
- **Clear**: Remove all logs from display

### Log Display

Logs appear in a scrollable container with:

- Timestamp
- Log level (color-coded)
- Target (module/component)
- Message

## Connecting to Log Stream

### Manual Connection

1. Click the **Connect** button
2. Wait for "Connected" status
3. Logs will appear automatically

### Auto-Connect

The log viewer does not auto-connect to preserve resources. Click Connect when you want to view logs.

### Disconnecting

Click **Disconnect** to stop receiving logs. Existing logs remain visible.

## Log Levels

Logs are color-coded by level:

| Level | Color | Description |
|-------|-------|-------------|
| ERROR | Red | Critical errors requiring attention |
| WARN | Orange | Warnings about potential issues |
| INFO | Blue | General information messages |
| DEBUG | Gray | Detailed debugging information |
| TRACE | Light gray | Very verbose tracing info |

### Filtering by Level

Use the dropdown to filter:

- **All levels**: Show everything
- **Error**: Only errors
- **Warn**: Warnings and above
- **Info**: Info and above
- **Debug**: Debug and above
- **Trace**: All including trace

## Log Format

Each log entry contains:

```
[timestamp] [LEVEL] [target] message
```

Example:
```
10:30:45 INFO squirreldb::api Document inserted in 'users': abc-123
10:30:46 DEBUG squirreldb::query Executing query: db.table("users").run()
10:30:46 INFO squirreldb::query Query on 'users' returned 5 results
```

### Timestamp

Shows local time in `HH:MM:SS` format.

### Level

Uppercase log level (ERROR, WARN, INFO, DEBUG, TRACE).

### Target

The Rust module/component that generated the log:

| Target | Description |
|--------|-------------|
| `squirreldb::daemon` | Server startup/shutdown |
| `squirreldb::api` | REST API operations |
| `squirreldb::query` | Query execution |
| `squirreldb::websocket` | WebSocket connections |
| `squirreldb::admin` | Admin UI events |
| `squirreldb::db` | Database operations |

### Message

The actual log message content.

## Auto-Scroll

When enabled (default), the log view automatically scrolls to show new logs.

### Enabling/Disabling

- Check the "Auto-scroll" checkbox to enable
- Uncheck to disable and browse history

### Pause on Scroll

Manually scrolling up automatically pauses auto-scroll. Re-enable by checking the box.

## Log Storage

### In-Browser

Logs are stored in memory (up to 1000 entries). Older logs are removed when the limit is reached.

### Persistence

Logs are not persisted. Refreshing the page clears all logs. For persistent logs, use server-side logging configuration.

## WebSocket Protocol

Logs are streamed via WebSocket at `/ws/logs`.

### Connection URL

```
ws://localhost:8081/ws/logs
ws://localhost:8081/ws/logs?token=sqrl_xxx  # With auth
```

### Message Format

```json
{
  "timestamp": "2024-01-15T10:30:45Z",
  "level": "info",
  "target": "squirreldb::api",
  "message": "Document inserted in 'users': abc-123"
}
```

### Using from Code

```javascript
const ws = new WebSocket('ws://localhost:8081/ws/logs?token=sqrl_xxx');

ws.onmessage = (event) => {
  const log = JSON.parse(event.data);
  console.log(`[${log.level}] ${log.message}`);
};

ws.onopen = () => console.log('Connected to log stream');
ws.onclose = () => console.log('Disconnected from log stream');
```

## Logged Events

### Server Startup

```
INFO squirreldb::daemon Initializing database schema...
INFO squirreldb::daemon Database schema initialized
INFO squirreldb::daemon Starting change listener...
INFO squirreldb::daemon Change listener started
INFO squirreldb::admin Starting admin UI on 0.0.0.0:8081
INFO squirreldb::websocket Starting WebSocket server on 0.0.0.0:8080
```

### API Operations

```
INFO squirreldb::api Document inserted in 'users': 550e8400-...
INFO squirreldb::api Document deleted from 'users': 550e8400-...
DEBUG squirreldb::query Executing query: db.table("users").run()
INFO squirreldb::query Query on 'users' returned 5 results
```

### Connections

```
INFO squirreldb::admin Log stream connected
INFO squirreldb::websocket Client connected: abc-123
INFO squirreldb::websocket Client disconnected: abc-123
```

## Troubleshooting

### No Logs Appearing

1. Verify connection status (green dot)
2. Check level filter isn't too restrictive
3. Perform some action to generate logs
4. Check browser console for errors

### Connection Keeps Dropping

1. Check network stability
2. Verify server is running
3. Check for proxy timeout settings
4. Try reconnecting

### Missing Log Entries

1. Check level filter
2. Logs may have been truncated (1000 limit)
3. Server may not emit logs for all events

### High Memory Usage

Clear logs periodically if viewing for extended periods:

1. Click **Clear** button
2. Or disconnect when not actively monitoring

## Security

### Authentication

When auth is enabled, the log stream requires a valid token:

```
ws://localhost:8081/ws/logs?token=sqrl_your_token
```

### Sensitive Information

Logs may contain:
- User IDs
- Query content
- Error details

Restrict access to authorized personnel only.

### Network Security

Use WSS (WebSocket Secure) in production:

```
wss://your-domain.com/ws/logs?token=sqrl_xxx
```

## Best Practices

### 1. Use in Development

Live logs are most useful during development and debugging.

### 2. Filter Appropriately

Use level filters to focus on relevant logs:
- Production monitoring: ERROR + WARN
- Debugging: INFO + DEBUG
- Deep investigation: TRACE

### 3. Clear Periodically

Clear logs when starting a new debugging session for cleaner output.

### 4. Combine with Persistent Logging

For audit trails, configure server-side logging in addition to live viewing:

```bash
# Start server with file logging
RUST_LOG=info sqrld 2>&1 | tee squirreldb.log
```

### 5. Monitor in Separate Tab

Keep logs in a separate browser tab while using other Admin UI features.

## Comparison with Server Logs

| Feature | Live Logs (UI) | Server Logs (File) |
|---------|----------------|-------------------|
| Persistence | No | Yes |
| Real-time | Yes | Tail -f |
| Filtering | UI dropdown | grep |
| History | 1000 entries | Unlimited |
| Search | No | Yes (grep) |
| Remote access | Yes (browser) | SSH required |

## Future Enhancements

Planned improvements:

- Log search/filter by text
- Export logs to file
- Persistent log storage
- Log level configuration via UI
- Structured log fields display
- Time range selection
