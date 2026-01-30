use squirreldb::types::{ClientMessage, ServerMessage};

#[test]
fn test_client_message_roundtrip() {
  let messages = vec![
    ClientMessage::Query {
      id: "1".into(),
      query: "db.table(\"test\").run()".into(),
    },
    ClientMessage::Subscribe {
      id: "2".into(),
      query: "db.table(\"test\").changes()".into(),
    },
    ClientMessage::Unsubscribe { id: "3".into() },
    ClientMessage::ListCollections { id: "4".into() },
    ClientMessage::Ping { id: "5".into() },
  ];

  for msg in messages {
    let json = serde_json::to_string(&msg).unwrap();
    let parsed: ClientMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(msg.id(), parsed.id());
  }
}

#[test]
fn test_server_message_roundtrip() {
  let result = ServerMessage::result("1", serde_json::json!({"data": [1, 2, 3]}));
  let json = serde_json::to_string(&result).unwrap();
  let parsed: ServerMessage = serde_json::from_str(&json).unwrap();
  assert!(matches!(parsed, ServerMessage::Result { id, .. } if id == "1"));
}

#[test]
fn test_client_message_insert_with_uuid() {
  let doc_id = uuid::Uuid::new_v4();
  let msg = ClientMessage::Update {
    id: "1".into(),
    collection: "users".into(),
    document_id: doc_id,
    data: serde_json::json!({"name": "Alice"}),
  };
  let json = serde_json::to_string(&msg).unwrap();
  assert!(json.contains(&doc_id.to_string()));
}

#[test]
fn test_server_error_message() {
  let err = ServerMessage::error("err-1", "Something went wrong");
  let json = serde_json::to_string(&err).unwrap();
  assert!(json.contains("error"));
  assert!(json.contains("Something went wrong"));
}

#[test]
fn test_message_type_tag() {
  let query = ClientMessage::Query {
    id: "1".into(),
    query: "test".into(),
  };
  let json = serde_json::to_string(&query).unwrap();
  assert!(json.contains(r#""type":"query""#));

  let ping = ClientMessage::Ping { id: "2".into() };
  let json = serde_json::to_string(&ping).unwrap();
  assert!(json.contains(r#""type":"ping""#));
}
