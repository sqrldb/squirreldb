//! Extended protocol tests - message serialization, parsing, and edge cases

use chrono::Utc;
use serde_json::json;
use squirreldb::types::{
  Change, ChangeEvent, ChangeOperation, ChangesOptions, ClientMessage, Document, FilterSpec,
  OrderBySpec, OrderDirection, QuerySpec, ServerMessage,
};
use uuid::Uuid;

// =============================================================================
// ClientMessage Tests
// =============================================================================

#[test]
fn test_client_message_query_serialization() {
  let msg = ClientMessage::Query {
    id: "query-1".into(),
    query: "db.table(\"users\").run()".into(),
  };

  let json = serde_json::to_string(&msg).unwrap();
  assert!(json.contains("\"type\":\"query\""));
  assert!(json.contains("\"id\":\"query-1\""));
  assert!(json.contains("\"query\":"));
}

#[test]
fn test_client_message_subscribe_serialization() {
  let msg = ClientMessage::Subscribe {
    id: "sub-1".into(),
    query: "db.table(\"users\").changes()".into(),
  };

  let json = serde_json::to_string(&msg).unwrap();
  assert!(json.contains("\"type\":\"subscribe\""));
  assert!(json.contains("\"id\":\"sub-1\""));
}

#[test]
fn test_client_message_unsubscribe_serialization() {
  let msg = ClientMessage::Unsubscribe { id: "sub-1".into() };

  let json = serde_json::to_string(&msg).unwrap();
  assert!(json.contains("\"type\":\"unsubscribe\""));
  assert!(json.contains("\"id\":\"sub-1\""));
}

#[test]
fn test_client_message_insert_serialization() {
  let msg = ClientMessage::Insert {
    id: "ins-1".into(),
    collection: "users".into(),
    data: json!({"name": "Alice"}),
  };

  let json = serde_json::to_string(&msg).unwrap();
  assert!(json.contains("\"type\":\"insert\""));
  assert!(json.contains("\"collection\":\"users\""));
  assert!(json.contains("\"name\":\"Alice\""));
}

#[test]
fn test_client_message_update_serialization() {
  let doc_id = Uuid::new_v4();
  let msg = ClientMessage::Update {
    id: "upd-1".into(),
    collection: "users".into(),
    document_id: doc_id,
    data: json!({"name": "Alice Updated"}),
  };

  let json = serde_json::to_string(&msg).unwrap();
  assert!(json.contains("\"type\":\"update\""));
  assert!(json.contains(&doc_id.to_string()));
}

#[test]
fn test_client_message_delete_serialization() {
  let doc_id = Uuid::new_v4();
  let msg = ClientMessage::Delete {
    id: "del-1".into(),
    collection: "users".into(),
    document_id: doc_id,
  };

  let json = serde_json::to_string(&msg).unwrap();
  assert!(json.contains("\"type\":\"delete\""));
  assert!(json.contains("\"collection\":\"users\""));
}

#[test]
fn test_client_message_list_collections_serialization() {
  let msg = ClientMessage::ListCollections {
    id: "list-1".into(),
  };

  let json = serde_json::to_string(&msg).unwrap();
  assert!(json.contains("\"type\":\"listcollections\""));
}

#[test]
fn test_client_message_ping_serialization() {
  let msg = ClientMessage::Ping {
    id: "ping-1".into(),
  };

  let json = serde_json::to_string(&msg).unwrap();
  assert!(json.contains("\"type\":\"ping\""));
}

#[test]
fn test_client_message_id_accessor() {
  let messages: Vec<ClientMessage> = vec![
    ClientMessage::Query {
      id: "q1".into(),
      query: "".into(),
    },
    ClientMessage::Subscribe {
      id: "s1".into(),
      query: "".into(),
    },
    ClientMessage::Unsubscribe { id: "u1".into() },
    ClientMessage::Insert {
      id: "i1".into(),
      collection: "".into(),
      data: json!({}),
    },
    ClientMessage::ListCollections { id: "l1".into() },
    ClientMessage::Ping { id: "p1".into() },
  ];

  let expected_ids = ["q1", "s1", "u1", "i1", "l1", "p1"];

  for (msg, expected) in messages.iter().zip(expected_ids.iter()) {
    assert_eq!(msg.id(), *expected);
  }
}

// =============================================================================
// ClientMessage Deserialization Tests
// =============================================================================

#[test]
fn test_client_message_query_deserialization() {
  let json = r#"{"type":"query","id":"q1","query":"db.table(\"users\").run()"}"#;
  let msg: ClientMessage = serde_json::from_str(json).unwrap();

  match msg {
    ClientMessage::Query { id, query } => {
      assert_eq!(id, "q1");
      assert!(query.contains("users"));
    }
    _ => panic!("Expected Query message"),
  }
}

#[test]
fn test_client_message_subscribe_deserialization() {
  let json = r#"{"type":"subscribe","id":"s1","query":"db.table(\"users\").changes()"}"#;
  let msg: ClientMessage = serde_json::from_str(json).unwrap();

  match msg {
    ClientMessage::Subscribe { id, query } => {
      assert_eq!(id, "s1");
      assert!(query.contains("changes"));
    }
    _ => panic!("Expected Subscribe message"),
  }
}

#[test]
fn test_client_message_insert_deserialization() {
  let json = r#"{"type":"insert","id":"i1","collection":"users","data":{"name":"Alice","age":30}}"#;
  let msg: ClientMessage = serde_json::from_str(json).unwrap();

  match msg {
    ClientMessage::Insert {
      id,
      collection,
      data,
    } => {
      assert_eq!(id, "i1");
      assert_eq!(collection, "users");
      assert_eq!(data["name"], "Alice");
      assert_eq!(data["age"], 30);
    }
    _ => panic!("Expected Insert message"),
  }
}

// =============================================================================
// ServerMessage Tests
// =============================================================================

#[test]
fn test_server_message_result_constructor() {
  let data = json!([{"id": "123", "name": "Alice"}]);
  let msg = ServerMessage::result("req-1", data.clone());

  match msg {
    ServerMessage::Result { id, data: d } => {
      assert_eq!(id, "req-1");
      assert_eq!(d, data);
    }
    _ => panic!("Expected Result message"),
  }
}

#[test]
fn test_server_message_error_constructor() {
  let msg = ServerMessage::error("req-1", "Something went wrong");

  match msg {
    ServerMessage::Error { id, error } => {
      assert_eq!(id, "req-1");
      assert_eq!(error, "Something went wrong");
    }
    _ => panic!("Expected Error message"),
  }
}

#[test]
fn test_server_message_result_serialization() {
  let msg = ServerMessage::result("r1", json!({"foo": "bar"}));
  let json = serde_json::to_string(&msg).unwrap();

  assert!(json.contains("\"type\":\"result\""));
  assert!(json.contains("\"id\":\"r1\""));
  assert!(json.contains("\"data\":"));
}

#[test]
fn test_server_message_error_serialization() {
  let msg = ServerMessage::error("e1", "Test error");
  let json = serde_json::to_string(&msg).unwrap();

  assert!(json.contains("\"type\":\"error\""));
  assert!(json.contains("\"id\":\"e1\""));
  assert!(json.contains("\"error\":\"Test error\""));
}

#[test]
fn test_server_message_change_serialization() {
  let doc = Document {
    id: Uuid::new_v4(),
    collection: "users".into(),
    data: json!({"name": "Alice"}),
    created_at: Utc::now(),
    updated_at: Utc::now(),
  };

  let change_event = ChangeEvent::Insert { new: doc };

  let msg = ServerMessage::Change {
    id: "sub-1".into(),
    change: change_event,
  };

  let json = serde_json::to_string(&msg).unwrap();
  assert!(json.contains("\"type\":\"change\""));
}

#[test]
fn test_server_message_pong_serialization() {
  let msg = ServerMessage::Pong {
    id: "ping-1".into(),
  };
  let json = serde_json::to_string(&msg).unwrap();

  assert!(json.contains("\"type\":\"pong\""));
  assert!(json.contains("\"id\":\"ping-1\""));
}

#[test]
fn test_server_message_subscribed_serialization() {
  let msg = ServerMessage::Subscribed { id: "sub-1".into() };
  let json = serde_json::to_string(&msg).unwrap();

  assert!(json.contains("\"type\":\"subscribed\""));
}

#[test]
fn test_server_message_unsubscribed_serialization() {
  let msg = ServerMessage::Unsubscribed { id: "sub-1".into() };
  let json = serde_json::to_string(&msg).unwrap();

  assert!(json.contains("\"type\":\"unsubscribed\""));
}

// =============================================================================
// Change and ChangeOperation Tests
// =============================================================================

#[test]
fn test_change_operation_parse_insert() {
  let op: ChangeOperation = "INSERT".parse().unwrap();
  assert_eq!(op, ChangeOperation::Insert);
}

#[test]
fn test_change_operation_parse_update() {
  let op: ChangeOperation = "UPDATE".parse().unwrap();
  assert_eq!(op, ChangeOperation::Update);
}

#[test]
fn test_change_operation_parse_delete() {
  let op: ChangeOperation = "DELETE".parse().unwrap();
  assert_eq!(op, ChangeOperation::Delete);
}

#[test]
fn test_change_operation_parse_lowercase() {
  let op: ChangeOperation = "insert".parse().unwrap();
  assert_eq!(op, ChangeOperation::Insert);
}

#[test]
fn test_change_operation_parse_mixed_case() {
  let op: ChangeOperation = "Update".parse().unwrap();
  assert_eq!(op, ChangeOperation::Update);
}

#[test]
fn test_change_operation_parse_invalid() {
  let result = "INVALID".parse::<ChangeOperation>();
  assert!(result.is_err());
}

#[test]
fn test_change_serialization_insert() {
  let change = Change {
    id: 1,
    operation: ChangeOperation::Insert,
    collection: "users".into(),
    document_id: Uuid::new_v4(),
    old_data: None,
    new_data: Some(json!({"name": "Alice"})),
    changed_at: Utc::now(),
  };

  let json = serde_json::to_string(&change).unwrap();
  assert!(json.contains("\"collection\":\"users\""));
  assert!(json.contains("\"new_data\":"));
}

#[test]
fn test_change_serialization_update() {
  let change = Change {
    id: 2,
    operation: ChangeOperation::Update,
    collection: "users".into(),
    document_id: Uuid::new_v4(),
    old_data: Some(json!({"name": "Alice", "age": 30})),
    new_data: Some(json!({"name": "Alice", "age": 31})),
    changed_at: Utc::now(),
  };

  let json = serde_json::to_string(&change).unwrap();
  assert!(json.contains("\"old_data\":"));
  assert!(json.contains("\"new_data\":"));
}

#[test]
fn test_change_serialization_delete() {
  let change = Change {
    id: 3,
    operation: ChangeOperation::Delete,
    collection: "users".into(),
    document_id: Uuid::new_v4(),
    old_data: Some(json!({"name": "Alice"})),
    new_data: None,
    changed_at: Utc::now(),
  };

  let json = serde_json::to_string(&change).unwrap();
  assert!(json.contains("\"old_data\":"));
}

// =============================================================================
// ChangeEvent Tests
// =============================================================================

#[test]
fn test_change_event_initial() {
  let doc = Document {
    id: Uuid::new_v4(),
    collection: "users".into(),
    data: json!({"name": "Alice"}),
    created_at: Utc::now(),
    updated_at: Utc::now(),
  };

  let event = ChangeEvent::Initial {
    document: doc.clone(),
  };

  let json = serde_json::to_string(&event).unwrap();
  assert!(json.contains("\"type\":\"initial\""));
  assert!(json.contains("\"document\":"));
}

#[test]
fn test_change_event_insert() {
  let doc = Document {
    id: Uuid::new_v4(),
    collection: "users".into(),
    data: json!({"name": "Bob"}),
    created_at: Utc::now(),
    updated_at: Utc::now(),
  };

  let event = ChangeEvent::Insert { new: doc };

  let json = serde_json::to_string(&event).unwrap();
  assert!(json.contains("\"type\":\"insert\""));
  assert!(json.contains("\"new\":"));
}

#[test]
fn test_change_event_update() {
  let doc = Document {
    id: Uuid::new_v4(),
    collection: "users".into(),
    data: json!({"name": "Charlie", "age": 31}),
    created_at: Utc::now(),
    updated_at: Utc::now(),
  };

  let event = ChangeEvent::Update {
    old: json!({"name": "Charlie", "age": 30}),
    new: doc,
  };

  let json = serde_json::to_string(&event).unwrap();
  assert!(json.contains("\"type\":\"update\""));
  assert!(json.contains("\"old\":"));
  assert!(json.contains("\"new\":"));
}

#[test]
fn test_change_event_delete() {
  let doc = Document {
    id: Uuid::new_v4(),
    collection: "users".into(),
    data: json!({"name": "David"}),
    created_at: Utc::now(),
    updated_at: Utc::now(),
  };

  let event = ChangeEvent::Delete { old: doc };

  let json = serde_json::to_string(&event).unwrap();
  assert!(json.contains("\"type\":\"delete\""));
  assert!(json.contains("\"old\":"));
}

// =============================================================================
// Document Tests
// =============================================================================

#[test]
fn test_document_serialization() {
  let doc = Document {
    id: Uuid::new_v4(),
    collection: "users".into(),
    data: json!({"name": "Alice", "age": 30}),
    created_at: chrono::Utc::now(),
    updated_at: chrono::Utc::now(),
  };

  let json = serde_json::to_string(&doc).unwrap();
  assert!(json.contains("\"id\":"));
  assert!(json.contains("\"collection\":\"users\""));
  assert!(json.contains("\"data\":"));
  assert!(json.contains("\"created_at\":"));
  assert!(json.contains("\"updated_at\":"));
}

#[test]
fn test_document_deserialization() {
  let id = Uuid::new_v4();
  let json = format!(
    r#"{{"id":"{}","collection":"users","data":{{"name":"Alice"}},"created_at":"2024-01-15T10:30:00Z","updated_at":"2024-01-15T10:30:00Z"}}"#,
    id
  );

  let doc: Document = serde_json::from_str(&json).unwrap();
  assert_eq!(doc.id, id);
  assert_eq!(doc.collection, "users");
  assert_eq!(doc.data["name"], "Alice");
}

// =============================================================================
// QuerySpec Tests
// =============================================================================

#[test]
fn test_query_spec_defaults() {
  let spec = QuerySpec {
    table: "users".into(),
    filter: None,
    map: None,
    order_by: None,
    limit: None,
    offset: None,
    changes: None,
  };

  assert_eq!(spec.table, "users");
  assert!(spec.filter.is_none());
  assert!(spec.map.is_none());
  assert!(spec.order_by.is_none());
  assert!(spec.limit.is_none());
  assert!(spec.offset.is_none());
  assert!(spec.changes.is_none());
}

#[test]
fn test_query_spec_with_all_fields() {
  let spec = QuerySpec {
    table: "users".into(),
    filter: Some(FilterSpec {
      js_code: "u => u.active".into(),
      compiled_sql: Some("active = true".into()),
    }),
    map: Some("u => u.name".into()),
    order_by: Some(OrderBySpec {
      field: "name".into(),
      direction: OrderDirection::Asc,
    }),
    limit: Some(10),
    offset: Some(5),
    changes: Some(ChangesOptions {
      include_initial: true,
    }),
  };

  assert_eq!(spec.table, "users");
  assert!(spec.filter.is_some());
  assert!(spec.map.is_some());
  assert!(spec.order_by.is_some());
  assert_eq!(spec.limit, Some(10));
  assert!(spec.changes.is_some());
}

// =============================================================================
// OrderBySpec and OrderDirection Tests
// =============================================================================

#[test]
fn test_order_direction_default() {
  let dir = OrderDirection::default();
  assert_eq!(dir, OrderDirection::Asc);
}

#[test]
fn test_order_by_serialization() {
  let order = OrderBySpec {
    field: "created_at".into(),
    direction: OrderDirection::Desc,
  };

  let json = serde_json::to_string(&order).unwrap();
  assert!(json.contains("\"field\":\"created_at\""));
  assert!(json.contains("\"direction\":\"desc\"") || json.contains("\"direction\":\"Desc\""));
}

#[test]
fn test_order_direction_variants() {
  assert_eq!(OrderDirection::Asc, OrderDirection::Asc);
  assert_eq!(OrderDirection::Desc, OrderDirection::Desc);
  assert_ne!(OrderDirection::Asc, OrderDirection::Desc);
}

// =============================================================================
// ChangesOptions Tests
// =============================================================================

#[test]
fn test_changes_options_default() {
  let opts = ChangesOptions::default();
  assert!(!opts.include_initial);
}

#[test]
fn test_changes_options_with_include_initial() {
  let opts = ChangesOptions {
    include_initial: true,
  };
  assert!(opts.include_initial);
}

#[test]
fn test_changes_options_serialization() {
  let opts = ChangesOptions {
    include_initial: true,
  };

  let json = serde_json::to_string(&opts).unwrap();
  assert!(json.contains("\"include_initial\":true") || json.contains("\"includeInitial\":true"));
}

// =============================================================================
// Roundtrip Tests
// =============================================================================

#[test]
fn test_client_message_roundtrip_all_types() {
  let messages: Vec<ClientMessage> = vec![
    ClientMessage::Query {
      id: "1".into(),
      query: "test".into(),
    },
    ClientMessage::Subscribe {
      id: "2".into(),
      query: "changes".into(),
    },
    ClientMessage::Unsubscribe { id: "3".into() },
    ClientMessage::Insert {
      id: "4".into(),
      collection: "col".into(),
      data: json!({"x": 1}),
    },
    ClientMessage::Update {
      id: "5".into(),
      collection: "col".into(),
      document_id: Uuid::new_v4(),
      data: json!({"x": 2}),
    },
    ClientMessage::Delete {
      id: "6".into(),
      collection: "col".into(),
      document_id: Uuid::new_v4(),
    },
    ClientMessage::ListCollections { id: "7".into() },
    ClientMessage::Ping { id: "8".into() },
  ];

  for msg in messages {
    let json = serde_json::to_string(&msg).unwrap();
    let parsed: ClientMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(msg.id(), parsed.id());
  }
}

#[test]
fn test_server_message_roundtrip() {
  let msg = ServerMessage::result("r1", json!({"data": [1, 2, 3]}));
  let json = serde_json::to_string(&msg).unwrap();
  let parsed: ServerMessage = serde_json::from_str(&json).unwrap();

  match parsed {
    ServerMessage::Result { id, data } => {
      assert_eq!(id, "r1");
      assert_eq!(data["data"], json!([1, 2, 3]));
    }
    _ => panic!("Expected Result"),
  }
}

// =============================================================================
// Edge Cases
// =============================================================================

#[test]
fn test_message_with_empty_id() {
  let msg = ClientMessage::Query {
    id: "".into(),
    query: "test".into(),
  };

  let json = serde_json::to_string(&msg).unwrap();
  let parsed: ClientMessage = serde_json::from_str(&json).unwrap();
  assert_eq!(parsed.id(), "");
}

#[test]
fn test_message_with_special_characters_in_id() {
  let msg = ClientMessage::Query {
    id: "id-with-special-chars_123.456".into(),
    query: "test".into(),
  };

  let json = serde_json::to_string(&msg).unwrap();
  let parsed: ClientMessage = serde_json::from_str(&json).unwrap();
  assert_eq!(parsed.id(), "id-with-special-chars_123.456");
}

#[test]
fn test_message_with_unicode_in_query() {
  let msg = ClientMessage::Query {
    id: "1".into(),
    query: "db.table(\"日本語\").run()".into(),
  };

  let json = serde_json::to_string(&msg).unwrap();
  let parsed: ClientMessage = serde_json::from_str(&json).unwrap();

  match parsed {
    ClientMessage::Query { query, .. } => {
      assert!(query.contains("日本語"));
    }
    _ => panic!("Expected Query"),
  }
}

#[test]
fn test_document_with_large_data() {
  let mut obj = serde_json::Map::new();
  for i in 0..1000 {
    obj.insert(format!("field_{}", i), json!(format!("value_{}", i)));
  }

  let doc = Document {
    id: Uuid::new_v4(),
    collection: "large".into(),
    data: serde_json::Value::Object(obj),
    created_at: chrono::Utc::now(),
    updated_at: chrono::Utc::now(),
  };

  let json = serde_json::to_string(&doc).unwrap();
  let parsed: Document = serde_json::from_str(&json).unwrap();

  assert_eq!(parsed.data["field_0"], "value_0");
  assert_eq!(parsed.data["field_999"], "value_999");
}
