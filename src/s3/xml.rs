use chrono::{DateTime, Utc};

use super::types::*;

/// Build XML for ListAllMyBucketsResult
pub fn list_buckets_xml(response: &ListBucketsResponse) -> String {
  let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
  xml.push_str("<ListAllMyBucketsResult xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\">\n");

  if let Some(ref owner) = response.owner {
    xml.push_str("  <Owner>\n");
    xml.push_str(&format!("    <ID>{}</ID>\n", escape_xml(&owner.id)));
    if let Some(ref name) = owner.display_name {
      xml.push_str(&format!(
        "    <DisplayName>{}</DisplayName>\n",
        escape_xml(name)
      ));
    }
    xml.push_str("  </Owner>\n");
  }

  xml.push_str("  <Buckets>\n");
  for bucket in &response.buckets {
    xml.push_str("    <Bucket>\n");
    xml.push_str(&format!(
      "      <Name>{}</Name>\n",
      escape_xml(&bucket.name)
    ));
    xml.push_str(&format!(
      "      <CreationDate>{}</CreationDate>\n",
      bucket.creation_date.to_rfc3339()
    ));
    xml.push_str("    </Bucket>\n");
  }
  xml.push_str("  </Buckets>\n");
  xml.push_str("</ListAllMyBucketsResult>");
  xml
}

/// Build XML for ListBucketResult (ListObjectsV2)
pub fn list_objects_v2_xml(response: &ListObjectsResponse) -> String {
  let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
  xml.push_str("<ListBucketResult xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\">\n");

  xml.push_str(&format!("  <Name>{}</Name>\n", escape_xml(&response.name)));

  if let Some(ref prefix) = response.prefix {
    xml.push_str(&format!("  <Prefix>{}</Prefix>\n", escape_xml(prefix)));
  } else {
    xml.push_str("  <Prefix></Prefix>\n");
  }

  xml.push_str(&format!("  <MaxKeys>{}</MaxKeys>\n", response.max_keys));
  xml.push_str(&format!("  <KeyCount>{}</KeyCount>\n", response.key_count));
  xml.push_str(&format!(
    "  <IsTruncated>{}</IsTruncated>\n",
    response.is_truncated
  ));

  if let Some(ref delimiter) = response.delimiter {
    xml.push_str(&format!(
      "  <Delimiter>{}</Delimiter>\n",
      escape_xml(delimiter)
    ));
  }

  if let Some(ref token) = response.continuation_token {
    xml.push_str(&format!(
      "  <ContinuationToken>{}</ContinuationToken>\n",
      escape_xml(token)
    ));
  }

  if let Some(ref token) = response.next_continuation_token {
    xml.push_str(&format!(
      "  <NextContinuationToken>{}</NextContinuationToken>\n",
      escape_xml(token)
    ));
  }

  for obj in &response.contents {
    xml.push_str("  <Contents>\n");
    xml.push_str(&format!("    <Key>{}</Key>\n", escape_xml(&obj.key)));
    xml.push_str(&format!(
      "    <LastModified>{}</LastModified>\n",
      obj.last_modified.to_rfc3339()
    ));
    xml.push_str(&format!("    <ETag>\"{}\"</ETag>\n", escape_xml(&obj.etag)));
    xml.push_str(&format!("    <Size>{}</Size>\n", obj.size));
    xml.push_str(&format!(
      "    <StorageClass>{}</StorageClass>\n",
      escape_xml(&obj.storage_class)
    ));
    if let Some(ref owner) = obj.owner {
      xml.push_str("    <Owner>\n");
      xml.push_str(&format!("      <ID>{}</ID>\n", escape_xml(&owner.id)));
      if let Some(ref name) = owner.display_name {
        xml.push_str(&format!(
          "      <DisplayName>{}</DisplayName>\n",
          escape_xml(name)
        ));
      }
      xml.push_str("    </Owner>\n");
    }
    xml.push_str("  </Contents>\n");
  }

  for prefix in &response.common_prefixes {
    xml.push_str("  <CommonPrefixes>\n");
    xml.push_str(&format!(
      "    <Prefix>{}</Prefix>\n",
      escape_xml(&prefix.prefix)
    ));
    xml.push_str("  </CommonPrefixes>\n");
  }

  xml.push_str("</ListBucketResult>");
  xml
}

/// Build XML for InitiateMultipartUploadResult
pub fn initiate_multipart_upload_xml(response: &InitiateMultipartUploadResponse) -> String {
  let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
  xml.push_str(
    "<InitiateMultipartUploadResult xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\">\n",
  );
  xml.push_str(&format!(
    "  <Bucket>{}</Bucket>\n",
    escape_xml(&response.bucket)
  ));
  xml.push_str(&format!("  <Key>{}</Key>\n", escape_xml(&response.key)));
  xml.push_str(&format!(
    "  <UploadId>{}</UploadId>\n",
    escape_xml(&response.upload_id)
  ));
  xml.push_str("</InitiateMultipartUploadResult>");
  xml
}

/// Build XML for CompleteMultipartUploadResult
pub fn complete_multipart_upload_xml(response: &CompleteMultipartUploadResponse) -> String {
  let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
  xml.push_str(
    "<CompleteMultipartUploadResult xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\">\n",
  );
  xml.push_str(&format!(
    "  <Location>{}</Location>\n",
    escape_xml(&response.location)
  ));
  xml.push_str(&format!(
    "  <Bucket>{}</Bucket>\n",
    escape_xml(&response.bucket)
  ));
  xml.push_str(&format!("  <Key>{}</Key>\n", escape_xml(&response.key)));
  xml.push_str(&format!(
    "  <ETag>\"{}\"</ETag>\n",
    escape_xml(&response.etag)
  ));
  xml.push_str("</CompleteMultipartUploadResult>");
  xml
}

/// Build XML for ListPartsResult
pub fn list_parts_xml(
  bucket: &str,
  key: &str,
  upload_id: &str,
  parts: &[S3Part],
  max_parts: i32,
  is_truncated: bool,
) -> String {
  let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
  xml.push_str("<ListPartsResult xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\">\n");
  xml.push_str(&format!("  <Bucket>{}</Bucket>\n", escape_xml(bucket)));
  xml.push_str(&format!("  <Key>{}</Key>\n", escape_xml(key)));
  xml.push_str(&format!(
    "  <UploadId>{}</UploadId>\n",
    escape_xml(upload_id)
  ));
  xml.push_str(&format!("  <MaxParts>{}</MaxParts>\n", max_parts));
  xml.push_str(&format!("  <IsTruncated>{}</IsTruncated>\n", is_truncated));

  for part in parts {
    xml.push_str("  <Part>\n");
    xml.push_str(&format!(
      "    <PartNumber>{}</PartNumber>\n",
      part.part_number
    ));
    xml.push_str(&format!(
      "    <LastModified>{}</LastModified>\n",
      part.created_at.to_rfc3339()
    ));
    xml.push_str(&format!(
      "    <ETag>\"{}\"</ETag>\n",
      escape_xml(&part.etag)
    ));
    xml.push_str(&format!("    <Size>{}</Size>\n", part.size));
    xml.push_str("  </Part>\n");
  }

  xml.push_str("</ListPartsResult>");
  xml
}

/// Build XML for ListMultipartUploadsResult
pub fn list_multipart_uploads_xml(
  bucket: &str,
  uploads: &[S3MultipartUpload],
  max_uploads: i32,
  is_truncated: bool,
) -> String {
  let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
  xml.push_str("<ListMultipartUploadsResult xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\">\n");
  xml.push_str(&format!("  <Bucket>{}</Bucket>\n", escape_xml(bucket)));
  xml.push_str(&format!("  <MaxUploads>{}</MaxUploads>\n", max_uploads));
  xml.push_str(&format!("  <IsTruncated>{}</IsTruncated>\n", is_truncated));

  for upload in uploads {
    xml.push_str("  <Upload>\n");
    xml.push_str(&format!("    <Key>{}</Key>\n", escape_xml(&upload.key)));
    xml.push_str(&format!("    <UploadId>{}</UploadId>\n", upload.upload_id));
    xml.push_str(&format!(
      "    <Initiated>{}</Initiated>\n",
      upload.initiated_at.to_rfc3339()
    ));
    xml.push_str("  </Upload>\n");
  }

  xml.push_str("</ListMultipartUploadsResult>");
  xml
}

/// Build XML for VersioningConfiguration
pub fn versioning_config_xml(enabled: bool) -> String {
  let status = if enabled { "Enabled" } else { "Suspended" };
  format!(
    "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<VersioningConfiguration xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\">\n  <Status>{}</Status>\n</VersioningConfiguration>",
    status
  )
}

/// Build XML for DeleteResult
pub fn delete_result_xml(result: &DeleteResult) -> String {
  let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
  xml.push_str("<DeleteResult xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\">\n");

  for deleted in &result.deleted {
    xml.push_str("  <Deleted>\n");
    xml.push_str(&format!("    <Key>{}</Key>\n", escape_xml(&deleted.key)));
    if let Some(ref vid) = deleted.version_id {
      xml.push_str(&format!("    <VersionId>{}</VersionId>\n", escape_xml(vid)));
    }
    if let Some(dm) = deleted.delete_marker {
      xml.push_str(&format!("    <DeleteMarker>{}</DeleteMarker>\n", dm));
    }
    if let Some(ref vid) = deleted.delete_marker_version_id {
      xml.push_str(&format!(
        "    <DeleteMarkerVersionId>{}</DeleteMarkerVersionId>\n",
        escape_xml(vid)
      ));
    }
    xml.push_str("  </Deleted>\n");
  }

  for error in &result.errors {
    xml.push_str("  <Error>\n");
    xml.push_str(&format!("    <Key>{}</Key>\n", escape_xml(&error.key)));
    if let Some(ref vid) = error.version_id {
      xml.push_str(&format!("    <VersionId>{}</VersionId>\n", escape_xml(vid)));
    }
    xml.push_str(&format!("    <Code>{}</Code>\n", escape_xml(&error.code)));
    xml.push_str(&format!(
      "    <Message>{}</Message>\n",
      escape_xml(&error.message)
    ));
    xml.push_str("  </Error>\n");
  }

  xml.push_str("</DeleteResult>");
  xml
}

/// Build XML for CopyObjectResult
pub fn copy_object_result_xml(etag: &str, last_modified: DateTime<Utc>) -> String {
  format!(
    "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<CopyObjectResult>\n  <ETag>\"{}\"</ETag>\n  <LastModified>{}</LastModified>\n</CopyObjectResult>",
    escape_xml(etag),
    last_modified.to_rfc3339()
  )
}

/// Build XML for AccessControlPolicy (bucket/object ACL)
pub fn acl_xml(owner_id: &str, owner_name: Option<&str>, grants: &[AclGrant]) -> String {
  let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
  xml.push_str("<AccessControlPolicy xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\">\n");
  xml.push_str("  <Owner>\n");
  xml.push_str(&format!("    <ID>{}</ID>\n", escape_xml(owner_id)));
  if let Some(name) = owner_name {
    xml.push_str(&format!(
      "    <DisplayName>{}</DisplayName>\n",
      escape_xml(name)
    ));
  }
  xml.push_str("  </Owner>\n");
  xml.push_str("  <AccessControlList>\n");

  for grant in grants {
    xml.push_str("    <Grant>\n");
    match &grant.grantee {
      Grantee::CanonicalUser { id, display_name } => {
        xml.push_str("      <Grantee xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xsi:type=\"CanonicalUser\">\n");
        xml.push_str(&format!("        <ID>{}</ID>\n", escape_xml(id)));
        if let Some(name) = display_name {
          xml.push_str(&format!(
            "        <DisplayName>{}</DisplayName>\n",
            escape_xml(name)
          ));
        }
        xml.push_str("      </Grantee>\n");
      }
      Grantee::Group { uri } => {
        xml.push_str("      <Grantee xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xsi:type=\"Group\">\n");
        xml.push_str(&format!("        <URI>{}</URI>\n", escape_xml(uri)));
        xml.push_str("      </Grantee>\n");
      }
    }
    xml.push_str(&format!(
      "      <Permission>{}</Permission>\n",
      permission_to_str(grant.permission)
    ));
    xml.push_str("    </Grant>\n");
  }

  xml.push_str("  </AccessControlList>\n");
  xml.push_str("</AccessControlPolicy>");
  xml
}

/// Build XML for LifecycleConfiguration
pub fn lifecycle_config_xml(rules: &[LifecycleRule]) -> String {
  let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
  xml.push_str("<LifecycleConfiguration xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\">\n");

  for rule in rules {
    xml.push_str("  <Rule>\n");
    xml.push_str(&format!("    <ID>{}</ID>\n", escape_xml(&rule.id)));
    xml.push_str(&format!(
      "    <Status>{}</Status>\n",
      if rule.enabled { "Enabled" } else { "Disabled" }
    ));
    if let Some(ref prefix) = rule.prefix {
      xml.push_str(&format!("    <Prefix>{}</Prefix>\n", escape_xml(prefix)));
    }
    if let Some(days) = rule.expiration_days {
      xml.push_str("    <Expiration>\n");
      xml.push_str(&format!("      <Days>{}</Days>\n", days));
      xml.push_str("    </Expiration>\n");
    }
    if let Some(days) = rule.noncurrent_version_expiration_days {
      xml.push_str("    <NoncurrentVersionExpiration>\n");
      xml.push_str(&format!(
        "      <NoncurrentDays>{}</NoncurrentDays>\n",
        days
      ));
      xml.push_str("    </NoncurrentVersionExpiration>\n");
    }
    xml.push_str("  </Rule>\n");
  }

  xml.push_str("</LifecycleConfiguration>");
  xml
}

fn permission_to_str(p: Permission) -> &'static str {
  match p {
    Permission::FullControl => "FULL_CONTROL",
    Permission::Write => "WRITE",
    Permission::WriteAcp => "WRITE_ACP",
    Permission::Read => "READ",
    Permission::ReadAcp => "READ_ACP",
  }
}

fn escape_xml(s: &str) -> String {
  s.replace('&', "&amp;")
    .replace('<', "&lt;")
    .replace('>', "&gt;")
    .replace('\'', "&apos;")
    .replace('"', "&quot;")
}
