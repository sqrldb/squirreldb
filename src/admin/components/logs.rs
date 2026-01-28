//! Logs component - real-time server log streaming

use leptos::*;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{js_sys, MessageEvent, WebSocket};
use super::Icon;

#[derive(Clone)]
struct LogEntry {
  id: u32,
  timestamp: String,
  level: String,
  message: String,
}

#[component]
pub fn Logs() -> impl IntoView {
  let (logs, set_logs) = create_signal(Vec::<LogEntry>::new());
  let (connected, set_connected) = create_signal(false);
  let (paused, set_paused) = create_signal(false);
  let next_id = create_rw_signal(0u32);
  let ws = create_rw_signal::<Option<WebSocket>>(None);

  // Connect to WebSocket
  let connect = move || {
    let window = web_sys::window().unwrap();
    let location = window.location();
    let protocol = if location.protocol().unwrap() == "https:" { "wss:" } else { "ws:" };
    let host = location.host().unwrap();
    let url = format!("{}//{}/ws/logs", protocol, host);

    match WebSocket::new(&url) {
      Ok(socket) => {
        let onopen = Closure::wrap(Box::new(move || {
          set_connected.set(true);
        }) as Box<dyn Fn()>);
        socket.set_onopen(Some(onopen.as_ref().unchecked_ref()));
        onopen.forget();

        let onclose = Closure::wrap(Box::new(move || {
          set_connected.set(false);
        }) as Box<dyn Fn()>);
        socket.set_onclose(Some(onclose.as_ref().unchecked_ref()));
        onclose.forget();

        let onmessage = Closure::wrap(Box::new(move |e: MessageEvent| {
          if paused.get() {
            return;
          }
          if let Ok(txt) = e.data().dyn_into::<js_sys::JsString>() {
            let msg: String = txt.into();
            // Parse log message (format: [TIMESTAMP] LEVEL message)
            let (timestamp, level, message) = parse_log_line(&msg);
            let id = next_id.get();
            next_id.set(id + 1);

            set_logs.update(|l| {
              l.push(LogEntry { id, timestamp, level, message });
              // Keep only last 500 logs
              if l.len() > 500 {
                l.remove(0);
              }
            });
          }
        }) as Box<dyn Fn(MessageEvent)>);
        socket.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
        onmessage.forget();

        ws.set(Some(socket));
      }
      Err(_) => {
        set_connected.set(false);
      }
    }
  };

  // Connect on mount
  create_effect(move |_| {
    connect();
  });

  // Cleanup on unmount
  on_cleanup(move || {
    if let Some(socket) = ws.get() {
      let _ = socket.close();
    }
  });

  let clear_logs = move |_| {
    set_logs.set(Vec::new());
  };

  let toggle_pause = move |_| {
    set_paused.update(|p| *p = !*p);
  };

  view! {
    <section id="logs" class="page active">
      <div class="page-header">
        <h2>"Server Logs"</h2>
      </div>
      <div class="log-status-bar">
        <div class="log-connection-status">
          <span class=move || if connected.get() { "status-indicator connected" } else { "status-indicator" }></span>
          {move || if connected.get() { "Connected" } else { "Disconnected" }}
        </div>
        <div class="log-actions">
          <button class="btn btn-secondary btn-sm" on:click=clear_logs>
            <Icon name="trash-2" size=14/>
            " Clear"
          </button>
          <button class="btn btn-secondary btn-sm" on:click=toggle_pause>
            {move || if paused.get() {
              view! { <><Icon name="play" size=14/>" Resume"</> }.into_view()
            } else {
              view! { <><Icon name="pause" size=14/>" Pause"</> }.into_view()
            }}
          </button>
        </div>
      </div>
      <div class="logs-container">
        <Show
          when=move || !logs.get().is_empty()
          fallback=|| view! {
            <div class="empty-state">
              <Icon name="scroll-text" size=32/>
              <p class="text-muted">"Waiting for logs..."</p>
            </div>
          }
        >
          <div class="log-entries">
            <For
              each=move || logs.get()
              key=|e| e.id
              children=move |entry| {
                let level_class = match entry.level.as_str() {
                  "ERROR" => "log-level error",
                  "WARN" => "log-level warn",
                  "INFO" => "log-level info",
                  "DEBUG" => "log-level debug",
                  _ => "log-level",
                };
                view! {
                  <div class="log-entry">
                    <span class="log-timestamp">{entry.timestamp.clone()}</span>
                    <span class=level_class>{entry.level.clone()}</span>
                    <span class="log-message">{entry.message.clone()}</span>
                  </div>
                }
              }
            />
          </div>
        </Show>
      </div>
    </section>
  }
}

fn parse_log_line(line: &str) -> (String, String, String) {
  // Try to parse format: [2024-01-15T10:30:00Z] INFO message
  // or: 2024-01-15T10:30:00Z INFO message
  let parts: Vec<&str> = line.splitn(3, ' ').collect();
  if parts.len() >= 3 {
    let timestamp = parts[0].trim_matches(|c| c == '[' || c == ']').to_string();
    let level = parts[1].to_string();
    let message = parts[2..].join(" ");
    (timestamp, level, message)
  } else {
    (String::new(), "INFO".to_string(), line.to_string())
  }
}
