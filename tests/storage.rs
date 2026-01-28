use chrono::Utc;
use squirreldb::storage::config::StorageConfig;
use squirreldb::storage::error::{StorageError, StorageErrorCode};
use squirreldb::storage::types::*;
use squirreldb::storage::xml;
use uuid::Uuid;

// =============================================================================
// Storage Config Tests
// =============================================================================

#[test]
fn test_storage_config_default() {
  let config = StorageConfig::default();
  assert_eq!(config.port, 9000);
  assert_eq!(config.storage_path, "./data/storage");
  assert_eq!(config.max_object_size, 5 * 1024 * 1024 * 1024);
  assert_eq!(config.max_part_size, 5 * 1024 * 1024 * 1024);
  assert_eq!(config.min_part_size, 5 * 1024 * 1024);
  assert_eq!(config.region, "us-east-1");
}

// =============================================================================
// Storage Types Tests
// =============================================================================

#[test]
fn test_copy_source_parse_simple() {
  let source = CopySource::parse("/my-bucket/my-key.txt").unwrap();
  assert_eq!(source.bucket, "my-bucket");
  assert_eq!(source.key, "my-key.txt");
  assert!(source.version_id.is_none());
}

#[test]
fn test_copy_source_parse_with_version() {
  let source = CopySource::parse("/my-bucket/path/to/file.txt?versionId=abc123").unwrap();
  assert_eq!(source.bucket, "my-bucket");
  assert_eq!(source.key, "path/to/file.txt");
  assert_eq!(source.version_id, Some("abc123".to_string()));
}

#[test]
fn test_copy_source_parse_encoded() {
  let source = CopySource::parse("/my-bucket/path%2Fto%2Ffile.txt").unwrap();
  assert_eq!(source.bucket, "my-bucket");
  assert_eq!(source.key, "path/to/file.txt");
}

#[test]
fn test_copy_source_parse_without_leading_slash() {
  let source = CopySource::parse("my-bucket/my-key.txt").unwrap();
  assert_eq!(source.bucket, "my-bucket");
  assert_eq!(source.key, "my-key.txt");
}

#[test]
fn test_copy_source_parse_invalid() {
  assert!(CopySource::parse("").is_none());
  assert!(CopySource::parse("nobucket").is_none());
}

#[test]
fn test_bucket_acl_default() {
  let acl = BucketAcl::default();
  assert!(acl.grants.is_empty());
}

#[test]
fn test_object_acl_default() {
  let acl = ObjectAcl::default();
  assert!(acl.grants.is_empty());
}

#[test]
fn test_access_key_permissions_default() {
  let perms = AccessKeyPermissions::default();
  assert_eq!(perms.buckets, "*");
  assert_eq!(perms.actions, "*");
}

#[test]
fn test_versioning_status_serialize() {
  let config = VersioningConfiguration {
    status: VersioningStatus::Enabled,
  };
  let json = serde_json::to_string(&config).unwrap();
  assert!(json.contains("\"status\":\"Enabled\""));

  let config = VersioningConfiguration {
    status: VersioningStatus::Suspended,
  };
  let json = serde_json::to_string(&config).unwrap();
  assert!(json.contains("\"status\":\"Suspended\""));
}

#[test]
fn test_permission_equality() {
  assert_eq!(Permission::FullControl, Permission::FullControl);
  assert_ne!(Permission::Read, Permission::Write);
}

// =============================================================================
// Storage Error Tests
// =============================================================================

#[test]
fn test_storage_error_new() {
  let error = StorageError::new(StorageErrorCode::NoSuchBucket, "Bucket not found");
  assert_eq!(error.code, StorageErrorCode::NoSuchBucket);
  assert_eq!(error.message, "Bucket not found");
  assert!(error.resource.is_none());
  assert!(error.request_id.is_none());
}

#[test]
fn test_storage_error_with_resource() {
  let error = StorageError::new(StorageErrorCode::NoSuchKey, "Key not found").with_resource("my-key");
  assert_eq!(error.resource, Some("my-key".to_string()));
}

#[test]
fn test_storage_error_with_request_id() {
  let error = StorageError::new(StorageErrorCode::InternalError, "Server error").with_request_id("req-123");
  assert_eq!(error.request_id, Some("req-123".to_string()));
}

#[test]
fn test_storage_error_no_such_bucket() {
  let error = StorageError::no_such_bucket("test-bucket");
  assert_eq!(error.code, StorageErrorCode::NoSuchBucket);
  assert!(error.message.contains("test-bucket"));
  assert_eq!(error.resource, Some("test-bucket".to_string()));
}

#[test]
fn test_storage_error_no_such_key() {
  let error = StorageError::no_such_key("my-file.txt");
  assert_eq!(error.code, StorageErrorCode::NoSuchKey);
  assert_eq!(error.resource, Some("my-file.txt".to_string()));
}

#[test]
fn test_storage_error_bucket_already_exists() {
  let error = StorageError::bucket_already_exists("existing-bucket");
  assert_eq!(error.code, StorageErrorCode::BucketAlreadyExists);
  assert_eq!(error.resource, Some("existing-bucket".to_string()));
}

#[test]
fn test_storage_error_bucket_not_empty() {
  let error = StorageError::bucket_not_empty("my-bucket");
  assert_eq!(error.code, StorageErrorCode::BucketNotEmpty);
  assert_eq!(error.resource, Some("my-bucket".to_string()));
}

#[test]
fn test_storage_error_invalid_argument() {
  let error = StorageError::invalid_argument("Invalid parameter");
  assert_eq!(error.code, StorageErrorCode::InvalidArgument);
  assert_eq!(error.message, "Invalid parameter");
}

#[test]
fn test_storage_error_invalid_bucket_name() {
  let error = StorageError::invalid_bucket_name("INVALID_NAME");
  assert_eq!(error.code, StorageErrorCode::InvalidBucketName);
  assert!(error.message.contains("INVALID_NAME"));
}

#[test]
fn test_storage_error_no_such_upload() {
  let error = StorageError::no_such_upload("upload-123");
  assert_eq!(error.code, StorageErrorCode::NoSuchUpload);
  assert_eq!(error.resource, Some("upload-123".to_string()));
}

#[test]
fn test_storage_error_to_xml() {
  let error = StorageError::new(StorageErrorCode::NoSuchBucket, "The bucket does not exist")
    .with_resource("test-bucket")
    .with_request_id("req-abc123");

  let xml_output = error.to_xml();
  assert!(xml_output.contains("<?xml version"));
  assert!(xml_output.contains("<Code>NoSuchBucket</Code>"));
  assert!(xml_output.contains("<Message>The bucket does not exist</Message>"));
  assert!(xml_output.contains("<Resource>test-bucket</Resource>"));
  assert!(xml_output.contains("<RequestId>req-abc123</RequestId>"));
}

#[test]
fn test_storage_error_xml_escaping() {
  let error = StorageError::new(
    StorageErrorCode::InvalidArgument,
    "Value contains <special> & \"chars\"",
  );
  let xml_output = error.to_xml();
  assert!(xml_output.contains("&lt;special&gt;"));
  assert!(xml_output.contains("&amp;"));
  assert!(xml_output.contains("&quot;chars&quot;"));
}

#[test]
fn test_storage_error_code_as_str() {
  assert_eq!(StorageErrorCode::AccessDenied.as_str(), "AccessDenied");
  assert_eq!(StorageErrorCode::NoSuchBucket.as_str(), "NoSuchBucket");
  assert_eq!(StorageErrorCode::NoSuchKey.as_str(), "NoSuchKey");
  assert_eq!(StorageErrorCode::InternalError.as_str(), "InternalError");
  assert_eq!(StorageErrorCode::InvalidBucketName.as_str(), "InvalidBucketName");
}

#[test]
fn test_storage_error_code_http_status() {
  use axum::http::StatusCode;
  assert_eq!(
    StorageErrorCode::AccessDenied.http_status(),
    StatusCode::FORBIDDEN
  );
  assert_eq!(
    StorageErrorCode::NoSuchBucket.http_status(),
    StatusCode::NOT_FOUND
  );
  assert_eq!(StorageErrorCode::NoSuchKey.http_status(), StatusCode::NOT_FOUND);
  assert_eq!(
    StorageErrorCode::InternalError.http_status(),
    StatusCode::INTERNAL_SERVER_ERROR
  );
  assert_eq!(
    StorageErrorCode::BucketNotEmpty.http_status(),
    StatusCode::CONFLICT
  );
  assert_eq!(
    StorageErrorCode::EntityTooLarge.http_status(),
    StatusCode::BAD_REQUEST
  );
}

#[test]
fn test_storage_error_display() {
  let error = StorageError::new(StorageErrorCode::NoSuchBucket, "Bucket not found");
  let display = format!("{}", error);
  assert!(display.contains("NoSuchBucket"));
  assert!(display.contains("Bucket not found"));
}

// =============================================================================
// Storage XML Tests
// =============================================================================

#[test]
fn test_list_buckets_xml() {
  let response = ListBucketsResponse {
    buckets: vec![
      BucketInfo {
        name: "bucket-1".to_string(),
        creation_date: Utc::now(),
      },
      BucketInfo {
        name: "bucket-2".to_string(),
        creation_date: Utc::now(),
      },
    ],
    owner: Some(Owner {
      id: "owner-123".to_string(),
      display_name: Some("Test Owner".to_string()),
    }),
  };

  let xml_output = xml::list_buckets_xml(&response);
  assert!(xml_output.contains("<?xml version"));
  assert!(xml_output.contains("<ListAllMyBucketsResult"));
  assert!(xml_output.contains("<Name>bucket-1</Name>"));
  assert!(xml_output.contains("<Name>bucket-2</Name>"));
  assert!(xml_output.contains("<ID>owner-123</ID>"));
  assert!(xml_output.contains("<DisplayName>Test Owner</DisplayName>"));
}

#[test]
fn test_list_buckets_xml_no_owner() {
  let response = ListBucketsResponse {
    buckets: vec![],
    owner: None,
  };

  let xml_output = xml::list_buckets_xml(&response);
  assert!(xml_output.contains("<Buckets>"));
  assert!(xml_output.contains("</Buckets>"));
  assert!(!xml_output.contains("<Owner>"));
}

#[test]
fn test_list_objects_v2_xml() {
  let response = ListObjectsResponse {
    name: "test-bucket".to_string(),
    prefix: Some("folder/".to_string()),
    delimiter: Some("/".to_string()),
    max_keys: 1000,
    is_truncated: false,
    contents: vec![ObjectInfo {
      key: "folder/file.txt".to_string(),
      last_modified: Utc::now(),
      etag: "abc123".to_string(),
      size: 1024,
      storage_class: "STANDARD".to_string(),
      owner: None,
    }],
    common_prefixes: vec![CommonPrefix {
      prefix: "folder/subfolder/".to_string(),
    }],
    continuation_token: None,
    next_continuation_token: None,
    key_count: 1,
    encoding_type: None,
  };

  let xml_output = xml::list_objects_v2_xml(&response);
  assert!(xml_output.contains("<ListBucketResult"));
  assert!(xml_output.contains("<Name>test-bucket</Name>"));
  assert!(xml_output.contains("<Prefix>folder/</Prefix>"));
  assert!(xml_output.contains("<Delimiter>/</Delimiter>"));
  assert!(xml_output.contains("<Key>folder/file.txt</Key>"));
  assert!(xml_output.contains("<ETag>\"abc123\"</ETag>"));
  assert!(xml_output.contains("<Size>1024</Size>"));
  assert!(xml_output.contains("<CommonPrefixes>"));
  assert!(xml_output.contains("<Prefix>folder/subfolder/</Prefix>"));
}

#[test]
fn test_initiate_multipart_upload_xml() {
  let response = InitiateMultipartUploadResponse {
    bucket: "my-bucket".to_string(),
    key: "large-file.zip".to_string(),
    upload_id: "upload-abc123".to_string(),
  };

  let xml_output = xml::initiate_multipart_upload_xml(&response);
  assert!(xml_output.contains("<InitiateMultipartUploadResult"));
  assert!(xml_output.contains("<Bucket>my-bucket</Bucket>"));
  assert!(xml_output.contains("<Key>large-file.zip</Key>"));
  assert!(xml_output.contains("<UploadId>upload-abc123</UploadId>"));
}

#[test]
fn test_complete_multipart_upload_xml() {
  let response = CompleteMultipartUploadResponse {
    location: "/my-bucket/large-file.zip".to_string(),
    bucket: "my-bucket".to_string(),
    key: "large-file.zip".to_string(),
    etag: "final-etag-123".to_string(),
  };

  let xml_output = xml::complete_multipart_upload_xml(&response);
  assert!(xml_output.contains("<CompleteMultipartUploadResult"));
  assert!(xml_output.contains("<Location>/my-bucket/large-file.zip</Location>"));
  assert!(xml_output.contains("<Bucket>my-bucket</Bucket>"));
  assert!(xml_output.contains("<Key>large-file.zip</Key>"));
  assert!(xml_output.contains("<ETag>\"final-etag-123\"</ETag>"));
}

#[test]
fn test_list_parts_xml() {
  let parts = vec![
    MultipartPart {
      upload_id: Uuid::new_v4(),
      part_number: 1,
      etag: "etag-part-1".to_string(),
      size: 5 * 1024 * 1024,
      storage_path: "/tmp/part1".to_string(),
      created_at: Utc::now(),
    },
    MultipartPart {
      upload_id: Uuid::new_v4(),
      part_number: 2,
      etag: "etag-part-2".to_string(),
      size: 3 * 1024 * 1024,
      storage_path: "/tmp/part2".to_string(),
      created_at: Utc::now(),
    },
  ];

  let xml_output = xml::list_parts_xml(
    "my-bucket",
    "large-file.zip",
    "upload-123",
    &parts,
    1000,
    false,
  );
  assert!(xml_output.contains("<ListPartsResult"));
  assert!(xml_output.contains("<Bucket>my-bucket</Bucket>"));
  assert!(xml_output.contains("<Key>large-file.zip</Key>"));
  assert!(xml_output.contains("<UploadId>upload-123</UploadId>"));
  assert!(xml_output.contains("<PartNumber>1</PartNumber>"));
  assert!(xml_output.contains("<PartNumber>2</PartNumber>"));
  assert!(xml_output.contains("<ETag>\"etag-part-1\"</ETag>"));
  assert!(xml_output.contains("<IsTruncated>false</IsTruncated>"));
}

#[test]
fn test_list_multipart_uploads_xml() {
  let uploads = vec![MultipartUpload {
    upload_id: Uuid::new_v4(),
    bucket: "my-bucket".to_string(),
    key: "file1.zip".to_string(),
    content_type: Some("application/zip".to_string()),
    metadata: serde_json::Value::Object(serde_json::Map::new()),
    initiated_at: Utc::now(),
  }];

  let xml_output = xml::list_multipart_uploads_xml("my-bucket", &uploads, 1000, false);
  assert!(xml_output.contains("<ListMultipartUploadsResult"));
  assert!(xml_output.contains("<Bucket>my-bucket</Bucket>"));
  assert!(xml_output.contains("<Key>file1.zip</Key>"));
  assert!(xml_output.contains("<IsTruncated>false</IsTruncated>"));
}

#[test]
fn test_versioning_config_xml_enabled() {
  let xml_output = xml::versioning_config_xml(true);
  assert!(xml_output.contains("<VersioningConfiguration"));
  assert!(xml_output.contains("<Status>Enabled</Status>"));
}

#[test]
fn test_versioning_config_xml_suspended() {
  let xml_output = xml::versioning_config_xml(false);
  assert!(xml_output.contains("<Status>Suspended</Status>"));
}

#[test]
fn test_copy_object_result_xml() {
  let xml_output = xml::copy_object_result_xml("etag-copy-123", Utc::now());
  assert!(xml_output.contains("<CopyObjectResult>"));
  assert!(xml_output.contains("<ETag>\"etag-copy-123\"</ETag>"));
  assert!(xml_output.contains("<LastModified>"));
}

#[test]
fn test_acl_xml_with_canonical_user() {
  let grants = vec![AclGrant {
    grantee: Grantee::CanonicalUser {
      id: "user-123".to_string(),
      display_name: Some("Test User".to_string()),
    },
    permission: Permission::FullControl,
  }];

  let xml_output = xml::acl_xml("owner-id", Some("Owner Name"), &grants);
  assert!(xml_output.contains("<AccessControlPolicy"));
  assert!(xml_output.contains("<ID>owner-id</ID>"));
  assert!(xml_output.contains("<DisplayName>Owner Name</DisplayName>"));
  assert!(xml_output.contains("xsi:type=\"CanonicalUser\""));
  assert!(xml_output.contains("<ID>user-123</ID>"));
  assert!(xml_output.contains("<Permission>FULL_CONTROL</Permission>"));
}

#[test]
fn test_acl_xml_with_group() {
  let grants = vec![AclGrant {
    grantee: Grantee::Group {
      uri: "http://acs.amazonaws.com/groups/global/AllUsers".to_string(),
    },
    permission: Permission::Read,
  }];

  let xml_output = xml::acl_xml("owner-id", None, &grants);
  assert!(xml_output.contains("xsi:type=\"Group\""));
  assert!(xml_output.contains("<URI>http://acs.amazonaws.com/groups/global/AllUsers</URI>"));
  assert!(xml_output.contains("<Permission>READ</Permission>"));
}

#[test]
fn test_lifecycle_config_xml() {
  let rules = vec![
    LifecycleRule {
      id: "rule-1".to_string(),
      enabled: true,
      prefix: Some("logs/".to_string()),
      expiration_days: Some(30),
      noncurrent_version_expiration_days: Some(7),
    },
    LifecycleRule {
      id: "rule-2".to_string(),
      enabled: false,
      prefix: None,
      expiration_days: Some(90),
      noncurrent_version_expiration_days: None,
    },
  ];

  let xml_output = xml::lifecycle_config_xml(&rules);
  assert!(xml_output.contains("<LifecycleConfiguration"));
  assert!(xml_output.contains("<ID>rule-1</ID>"));
  assert!(xml_output.contains("<Status>Enabled</Status>"));
  assert!(xml_output.contains("<Prefix>logs/</Prefix>"));
  assert!(xml_output.contains("<Days>30</Days>"));
  assert!(xml_output.contains("<NoncurrentDays>7</NoncurrentDays>"));
  assert!(xml_output.contains("<ID>rule-2</ID>"));
  assert!(xml_output.contains("<Status>Disabled</Status>"));
}

#[test]
fn test_xml_special_characters_escaping() {
  let response = ListBucketsResponse {
    buckets: vec![BucketInfo {
      name: "bucket-with-<special>&chars".to_string(),
      creation_date: Utc::now(),
    }],
    owner: None,
  };

  let xml_output = xml::list_buckets_xml(&response);
  assert!(xml_output.contains("&lt;special&gt;"));
  assert!(xml_output.contains("&amp;chars"));
}

// =============================================================================
// S3 Type Serialization Tests
// =============================================================================

#[test]
fn test_storage_bucket_serialize() {
  let bucket = StorageBucket {
    name: "test-bucket".to_string(),
    owner_id: Some(Uuid::new_v4()),
    versioning_enabled: true,
    acl: BucketAcl::default(),
    lifecycle_rules: vec![],
    quota_bytes: Some(1024 * 1024 * 1024),
    current_size: 1000,
    object_count: 5,
    created_at: Utc::now(),
  };

  let json = serde_json::to_string(&bucket).unwrap();
  assert!(json.contains("\"name\":\"test-bucket\""));
  assert!(json.contains("\"versioning_enabled\":true"));
  assert!(json.contains("\"current_size\":1000"));
  assert!(json.contains("\"object_count\":5"));
}

#[test]
fn test_storage_object_serialize() {
  let object = StorageObject {
    bucket: "my-bucket".to_string(),
    key: "path/to/file.txt".to_string(),
    version_id: Uuid::new_v4(),
    is_latest: true,
    etag: "abc123".to_string(),
    size: 2048,
    content_type: "text/plain".to_string(),
    storage_path: "/data/s3/my-bucket/abc/123.data".to_string(),
    metadata: serde_json::json!({"custom": "value"}),
    acl: ObjectAcl::default(),
    is_delete_marker: false,
    created_at: Utc::now(),
  };

  let json = serde_json::to_string(&object).unwrap();
  assert!(json.contains("\"key\":\"path/to/file.txt\""));
  assert!(json.contains("\"is_latest\":true"));
  assert!(json.contains("\"content_type\":\"text/plain\""));
  assert!(json.contains("\"custom\":\"value\""));
}

#[test]
fn test_completed_part_deserialize() {
  let json = r#"{"PartNumber": 1, "ETag": "\"abc123\""}"#;
  let part: CompletedPart = serde_json::from_str(json).unwrap();
  assert_eq!(part.part_number, 1);
  assert_eq!(part.etag, "\"abc123\"");
}

#[test]
fn test_delete_objects_request_deserialize() {
  let json = r#"{
    "Object": [
      {"Key": "file1.txt"},
      {"Key": "file2.txt", "VersionId": "v123"}
    ],
    "Quiet": true
  }"#;

  let request: DeleteObjectsRequest = serde_json::from_str(json).unwrap();
  assert_eq!(request.objects.len(), 2);
  assert_eq!(request.objects[0].key, "file1.txt");
  assert!(request.objects[0].version_id.is_none());
  assert_eq!(request.objects[1].key, "file2.txt");
  assert_eq!(request.objects[1].version_id, Some("v123".to_string()));
  assert!(request.quiet);
}

#[test]
fn test_grantee_serialize() {
  let canonical = Grantee::CanonicalUser {
    id: "user-123".to_string(),
    display_name: Some("Test User".to_string()),
  };
  let json = serde_json::to_string(&canonical).unwrap();
  assert!(json.contains("\"type\":\"CanonicalUser\""));
  assert!(json.contains("\"id\":\"user-123\""));

  let group = Grantee::Group {
    uri: "http://acs.amazonaws.com/groups/global/AllUsers".to_string(),
  };
  let json = serde_json::to_string(&group).unwrap();
  assert!(json.contains("\"type\":\"Group\""));
  assert!(json.contains("\"uri\":"));
}
