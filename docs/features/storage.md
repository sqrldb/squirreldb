# Object Storage

SquirrelDB includes an S3-compatible object storage feature for storing files and binary data.

## Configuration

Enable storage in your `squirreldb.yaml`:

```yaml
features:
  storage: true

storage:
  mode: builtin          # builtin or proxy
  port: 9000             # S3-compatible API port
  data_path: "./storage" # Local storage directory (builtin mode)
```

Or via environment variable:

```bash
SQRL_STORAGE_ENABLED=true sqrld
```

## Storage Modes

SquirrelDB supports two storage modes:

### Built-in Mode (Default)

Uses local filesystem storage. Files are stored in the configured `data_path` directory.

```yaml
storage:
  mode: builtin
  data_path: "./storage"
```

### Proxy Mode

Connect to an external S3 provider (AWS S3, MinIO, DigitalOcean Spaces, etc.):

```yaml
storage:
  mode: proxy
  proxy:
    endpoint: "https://s3.amazonaws.com"
    region: "us-west-2"
    access_key_id: "AKIAIOSFODNN7EXAMPLE"
    secret_access_key: "your-secret-key"
    bucket_prefix: "myapp-"      # Optional prefix for all buckets
    force_path_style: false      # Set true for MinIO/self-hosted
```

#### Provider Examples

**AWS S3:**
```yaml
storage:
  mode: proxy
  proxy:
    endpoint: "https://s3.amazonaws.com"
    region: "us-west-2"
    access_key_id: "AKIAIOSFODNN7EXAMPLE"
    secret_access_key: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"
```

**MinIO (Self-hosted):**
```yaml
storage:
  mode: proxy
  proxy:
    endpoint: "http://minio.local:9000"
    region: "us-east-1"
    access_key_id: "minioadmin"
    secret_access_key: "minioadmin"
    force_path_style: true
```

**DigitalOcean Spaces:**
```yaml
storage:
  mode: proxy
  proxy:
    endpoint: "https://nyc3.digitaloceanspaces.com"
    region: "nyc3"
    access_key_id: "your-spaces-key"
    secret_access_key: "your-spaces-secret"
```

## Admin UI Settings

Configure storage mode through the Admin UI:

1. Open Admin UI at `http://localhost:8081`
2. Navigate to **Settings** > **Storage**
3. Toggle between **Built-in** and **Proxy** modes
4. For Proxy mode, enter your S3 credentials
5. Click **Test Connection** to verify connectivity
6. Click **Save** to apply changes

## File Browser

The Admin UI includes a file browser for managing objects:

### Accessing the Browser

1. Navigate to **Storage** in the Admin sidebar
2. Create a bucket or select an existing one
3. Click **View** to open the file browser

### Features

- **Folder Navigation**: Click folders to navigate, use breadcrumbs to go back
- **Upload Files**: Drag-and-drop or click "Upload" to add files
- **Download**: Click the download icon on any file
- **Delete**: Select files and click delete, or use the delete icon
- **Preview**: Click files to preview images, text, and JSON

### Uploading Files

1. Click **Upload** in the browser toolbar
2. Drag files into the drop zone, or click **Choose Files**
3. Review selected files in the list
4. Click **Upload** to start the upload

Supported features:
- Multiple file upload
- Drag-and-drop support
- Upload progress indicator
- Automatic content-type detection

### File Preview

Click on a file to preview:

| Type | Preview |
|------|---------|
| Images (PNG, JPG, GIF, WebP) | Inline image display |
| Text files | Syntax-highlighted text |
| JSON | Formatted JSON viewer |
| Other | Download prompt |

## S3-Compatible API

The storage API is S3-compatible. Use any S3 client library:

### AWS CLI

```bash
# Configure endpoint
aws configure set default.s3.endpoint_url http://localhost:9000

# List buckets
aws s3 ls

# Create bucket
aws s3 mb s3://my-bucket

# Upload file
aws s3 cp myfile.txt s3://my-bucket/

# List objects
aws s3 ls s3://my-bucket/

# Download file
aws s3 cp s3://my-bucket/myfile.txt ./

# Delete object
aws s3 rm s3://my-bucket/myfile.txt
```

### Node.js (AWS SDK)

```javascript
const { S3Client, PutObjectCommand, GetObjectCommand } = require("@aws-sdk/client-s3")

const client = new S3Client({
  endpoint: "http://localhost:9000",
  region: "us-east-1",
  credentials: {
    accessKeyId: "your-access-key",
    secretAccessKey: "your-secret-key"
  },
  forcePathStyle: true
})

// Upload
await client.send(new PutObjectCommand({
  Bucket: "my-bucket",
  Key: "myfile.txt",
  Body: "Hello, World!"
}))

// Download
const response = await client.send(new GetObjectCommand({
  Bucket: "my-bucket",
  Key: "myfile.txt"
}))
const body = await response.Body.transformToString()
```

### Python (boto3)

```python
import boto3

s3 = boto3.client(
    's3',
    endpoint_url='http://localhost:9000',
    aws_access_key_id='your-access-key',
    aws_secret_access_key='your-secret-key'
)

# Upload
s3.put_object(Bucket='my-bucket', Key='myfile.txt', Body=b'Hello, World!')

# Download
response = s3.get_object(Bucket='my-bucket', Key='myfile.txt')
body = response['Body'].read()

# List objects
response = s3.list_objects_v2(Bucket='my-bucket', Prefix='uploads/')
for obj in response.get('Contents', []):
    print(obj['Key'])
```

## REST API

The Admin API provides endpoints for object management:

### List Objects

```bash
GET /api/s3/buckets/{bucket}/objects?prefix=uploads/&delimiter=/
Authorization: Bearer YOUR_TOKEN
```

Response:
```json
{
  "objects": [
    {
      "key": "uploads/image.png",
      "size": 24576,
      "last_modified": "2024-01-15T10:30:00Z",
      "etag": "\"d41d8cd98f00b204e9800998ecf8427e\""
    }
  ],
  "prefixes": ["uploads/images/", "uploads/documents/"]
}
```

### Upload Object

```bash
POST /api/s3/buckets/{bucket}/upload
Authorization: Bearer YOUR_TOKEN
Content-Type: multipart/form-data

[file data]
```

### Download Object

```bash
GET /api/s3/buckets/{bucket}/download/{key}?token=YOUR_TOKEN
```

### Delete Object

```bash
DELETE /api/s3/buckets/{bucket}/objects/{key}
Authorization: Bearer YOUR_TOKEN
```

## Security

### Authentication

Storage operations require authentication when enabled:

- Admin API endpoints require a valid bearer token
- S3-compatible API uses SigV4 authentication

### Bucket Isolation

In proxy mode with `bucket_prefix`, all bucket names are automatically prefixed:

```yaml
storage:
  proxy:
    bucket_prefix: "prod-"
```

Creating bucket "uploads" actually creates "prod-uploads" on the remote provider.

### Credentials Storage

Proxy credentials are stored encrypted in the database. The secret key is never exposed through the API after initial configuration.

## Troubleshooting

### Connection Failed

1. Verify endpoint URL is correct
2. Check network connectivity to the S3 provider
3. Verify credentials are valid
4. For self-hosted, ensure `force_path_style: true`

### Permission Denied

1. Verify IAM permissions for the access key
2. Check bucket policies allow the required operations
3. Ensure the bucket exists (for proxy mode)

### Upload Failed

1. Check file size limits on your S3 provider
2. Verify bucket write permissions
3. Check available storage space (builtin mode)

### Files Not Appearing

1. Refresh the browser view
2. Check the correct prefix/folder
3. Verify upload completed successfully
4. Check server logs for errors
