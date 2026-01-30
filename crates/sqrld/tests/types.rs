use squirreldb::types::*;

#[test]
fn test_change_operation_parse() {
  assert_eq!(
    "INSERT".parse::<ChangeOperation>().unwrap(),
    ChangeOperation::Insert
  );
  assert_eq!(
    "UPDATE".parse::<ChangeOperation>().unwrap(),
    ChangeOperation::Update
  );
  assert_eq!(
    "DELETE".parse::<ChangeOperation>().unwrap(),
    ChangeOperation::Delete
  );
  assert_eq!(
    "insert".parse::<ChangeOperation>().unwrap(),
    ChangeOperation::Insert
  );
  assert!("INVALID".parse::<ChangeOperation>().is_err());
}

#[test]
fn test_client_message_serialize() {
  let msg = ClientMessage::Query {
    id: "1".into(),
    query: "db.table(\"users\").run()".into(),
  };
  let json = serde_json::to_string(&msg).unwrap();
  assert!(json.contains("\"type\":\"query\""));
  assert!(json.contains("\"id\":\"1\""));
}

#[test]
fn test_server_message_constructors() {
  let result = ServerMessage::result("1", serde_json::json!({"foo": "bar"}));
  assert!(matches!(result, ServerMessage::Result { id, .. } if id == "1"));

  let error = ServerMessage::error("2", "something went wrong");
  assert!(
    matches!(error, ServerMessage::Error { id, error } if id == "2" && error == "something went wrong")
  );
}

#[test]
fn test_query_spec_defaults() {
  let spec = QuerySpec {
    project_id: None,
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
}

#[test]
fn test_order_direction_default() {
  assert_eq!(OrderDirection::default(), OrderDirection::Asc);
}

#[test]
fn test_changes_options_default() {
  let opts = ChangesOptions::default();
  assert!(!opts.include_initial);
}
