//! Live changes component - real-time changefeed viewer

use leptos::*;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{js_sys, MessageEvent, WebSocket};
use crate::admin::state::AppState;
use super::Icon;

#[derive(Clone)]
struct ChangeEntry {
  id: u32,
  timestamp: String,
  table: String,
  operation: String,
  document: String,
}

#[component]
pub fn Live() -> impl IntoView {
  let state = use_context::<AppState>().expect("AppState not found");
  let tables = state.tables;

  let (changes, set_changes) = create_signal(Vec::<ChangeEntry>::new());
  let (connected, set_connected) = create_signal(false);
  let (watching, set_watching) = create_signal(false);
  let (selected_table, set_selected_table) = create_signal(String::from("*"));
  let next_id = create_rw_signal(0u32);
  let ws = create_rw_signal::<Option<WebSocket>>(None);

  let start_watching = move |_| {
    let window = web_sys::window().unwrap();
    let location = window.location();
    let protocol = if location.protocol().unwrap() == "https:" { "wss:" } else { "ws:" };
    let host = location.host().unwrap();
    let table = selected_table.get();
    let url = format!("{}//{}/ws", protocol, host);

    match WebSocket::new(&url) {
      Ok(socket) => {
        let onopen = Closure::wrap(Box::new(move || {
          set_connected.set(true);
          set_watching.set(true);
        }) as Box<dyn Fn()>);
        socket.set_onopen(Some(onopen.as_ref().unchecked_ref()));
        onopen.forget();

        let onclose = Closure::wrap(Box::new(move || {
          set_connected.set(false);
          set_watching.set(false);
        }) as Box<dyn Fn()>);
        socket.set_onclose(Some(onclose.as_ref().unchecked_ref()));
        onclose.forget();

        let onmessage = Closure::wrap(Box::new(move |e: MessageEvent| {
          if let Ok(txt) = e.data().dyn_into::<js_sys::JsString>() {
            let msg: String = txt.into();
            // Parse change message (JSON)
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&msg) {
              let table = val.get("table")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
              let operation = val.get("type")
                .or_else(|| val.get("operation"))
                .and_then(|v| v.as_str())
                .unwrap_or("change")
                .to_string();
              let document = val.get("new_val")
                .or_else(|| val.get("old_val"))
                .or_else(|| val.get("document"))
                .map(|v| serde_json::to_string_pretty(v).unwrap_or_default())
                .unwrap_or_else(|| msg.clone());

              let id = next_id.get();
              next_id.set(id + 1);

              let now = js_sys::Date::new_0();
              let timestamp = format!(
                "{:02}:{:02}:{:02}",
                now.get_hours(),
                now.get_minutes(),
                now.get_seconds()
              );

              set_changes.update(|c| {
                c.insert(0, ChangeEntry { id, timestamp, table, operation, document });
                // Keep only last 100 changes
                if c.len() > 100 {
                  c.pop();
                }
              });
            }
          }
        }) as Box<dyn Fn(MessageEvent)>);
        socket.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
        onmessage.forget();

        // Send subscribe message
        let subscribe_msg = if table == "*" {
          r#"{"type":"subscribe","query":"db.changes()"}"#.to_string()
        } else {
          format!(r#"{{"type":"subscribe","query":"db.table('{}').changes()"}}"#, table)
        };
        let _ = socket.send_with_str(&subscribe_msg);

        ws.set(Some(socket));
      }
      Err(_) => {
        set_connected.set(false);
      }
    }
  };

  let stop_watching = move |_| {
    if let Some(socket) = ws.get() {
      let _ = socket.close();
    }
    ws.set(None);
    set_watching.set(false);
    set_connected.set(false);
  };

  let clear_changes = move |_| {
    set_changes.set(Vec::new());
  };

  // Cleanup on unmount
  on_cleanup(move || {
    if let Some(socket) = ws.get() {
      let _ = socket.close();
    }
  });

  view! {
    <section id="live" class="page active">
      <div class="page-header">
        <h2>"Live Changes"</h2>
        <div class="live-status">
          <span class=move || if connected.get() { "status-indicator connected" } else { "status-indicator" }></span>
          {move || if watching.get() { "Watching" } else { "Ready" }}
        </div>
      </div>
      <div class="live-controls">
        <div class="live-control-row">
          <div class="live-control-group">
            <label>"Table"</label>
            <select
              class="table-selector"
              prop:value=selected_table
              on:change=move |ev| set_selected_table.set(event_target_value(&ev))
              disabled=watching
            >
              <option value="*">"All tables"</option>
              <For
                each=move || tables.get()
                key=|t| t.name.clone()
                children=move |table| {
                  view! {
                    <option value=table.name.clone()>{table.name}</option>
                  }
                }
              />
            </select>
          </div>
          <div class="live-control-buttons">
            <Show
              when=move || !watching.get()
              fallback=move || view! {
                <button class="btn btn-danger" on:click=stop_watching>
                  <Icon name="stop-circle" size=14/>
                  " Stop"
                </button>
              }
            >
              <button class="btn btn-primary" on:click=start_watching>
                <Icon name="play" size=14/>
                " Start"
              </button>
            </Show>
            <button class="btn btn-secondary" on:click=clear_changes>
              <Icon name="trash-2" size=14/>
              " Clear"
            </button>
          </div>
        </div>
      </div>
      <div class="live-feed-container">
        <div class="live-feed">
          <Show
            when=move || !changes.get().is_empty()
            fallback=|| view! {
              <div class="empty-state">
                <Icon name="zap" size=32/>
                <p>"No changes yet"</p>
                <p class="text-muted">"Start watching to see realtime changes"</p>
              </div>
            }
          >
            <For
              each=move || changes.get()
              key=|c| c.id
              children=move |change| {
                let op_class = match change.operation.to_lowercase().as_str() {
                  "insert" | "add" => "change-op insert",
                  "delete" | "remove" => "change-op delete",
                  "update" | "replace" => "change-op update",
                  _ => "change-op",
                };
                view! {
                  <div class="change-entry">
                    <div class="change-header">
                      <span class="change-timestamp">{change.timestamp.clone()}</span>
                      <span class=op_class>{change.operation.clone()}</span>
                      <span class="change-table">{change.table.clone()}</span>
                    </div>
                    <pre class="change-document">{change.document.clone()}</pre>
                  </div>
                }
              }
            />
          </Show>
        </div>
      </div>
    </section>
  }
}
