//! General settings tab

use crate::admin::apiclient;
use crate::admin::state::{AppState, ToastLevel};
use leptos::*;

#[component]
pub fn GeneralSettings() -> impl IntoView {
  let state = use_context::<AppState>().expect("AppState not found");
  let protocol_settings = state.protocol_settings;
  let cors_settings = state.cors_settings;
  let backup_settings = state.backup_settings;
  let loading = create_rw_signal(false);
  let cors_loading = create_rw_signal(false);
  let backup_loading = create_rw_signal(false);
  let restarting = create_rw_signal(false);
  let restart_logs = create_rw_signal::<Vec<(String, String)>>(Vec::new());
  let new_origin = create_rw_signal(String::new());

  let state_stored = store_value(state.clone());

  // Load settings on mount
  create_effect(move |_| {
    spawn_local(async move {
      if let Ok(settings) = apiclient::fetch_protocol_settings().await {
        protocol_settings.set(settings);
      }
      if let Ok(settings) = apiclient::fetch_cors_settings().await {
        cors_settings.set(settings);
      }
      if let Ok(settings) = apiclient::fetch_backup_settings().await {
        backup_settings.set(settings);
      }
    });
  });

  let add_log = move |msg: &str| {
    let now = js_sys::Date::new_0();
    let timestamp = format!(
      "{:02}:{:02}:{:02}",
      now.get_hours(),
      now.get_minutes(),
      now.get_seconds()
    );
    restart_logs.update(|logs| {
      logs.push((timestamp, msg.to_string()));
    });
  };

  let toggle_protocol = move |protocol: &'static str, checked: bool| {
    loading.set(true);
    spawn_local(async move {
      let result = match protocol {
        "rest" => apiclient::update_protocol_settings(Some(checked), None, None, None, None).await,
        "websocket" => {
          apiclient::update_protocol_settings(None, Some(checked), None, None, None).await
        }
        "sse" => apiclient::update_protocol_settings(None, None, Some(checked), None, None).await,
        "tcp" => apiclient::update_protocol_settings(None, None, None, Some(checked), None).await,
        "mcp" => apiclient::update_protocol_settings(None, None, None, None, Some(checked)).await,
        _ => Err("Unknown protocol".to_string()),
      };

      match result {
        Ok(settings) => {
          protocol_settings.set(settings);
          let st = state_stored.get_value();
          st.show_toast(
            &format!(
              "{} {} (restart required)",
              match protocol {
                "rest" => "REST API",
                "websocket" => "WebSocket",
                "sse" => "Server-Sent Events",
                "tcp" => "TCP Protocol",
                "mcp" => "MCP",
                _ => protocol,
              },
              if checked { "enabled" } else { "disabled" }
            ),
            ToastLevel::Success,
          );
        }
        Err(e) => {
          let st = state_stored.get_value();
          st.show_toast(&format!("Failed to update: {}", e), ToastLevel::Error);
        }
      }
      loading.set(false);
    });
  };

  let do_add_origin = move || {
    let origin = new_origin.get().trim().to_string();
    if origin.is_empty() {
      return;
    }

    cors_loading.set(true);
    let mut origins = cors_settings.get().origins;

    // Remove "*" if adding a specific origin
    if origin != "*" {
      origins.retain(|o| o != "*");
    }

    // Add the new origin if not already present
    if !origins.contains(&origin) {
      origins.push(origin);
    }

    spawn_local(async move {
      match apiclient::update_cors_settings(origins).await {
        Ok(settings) => {
          cors_settings.set(settings);
          new_origin.set(String::new());
          let st = state_stored.get_value();
          st.show_toast("CORS origin added (restart required)", ToastLevel::Success);
        }
        Err(e) => {
          let st = state_stored.get_value();
          st.show_toast(&format!("Failed to add origin: {}", e), ToastLevel::Error);
        }
      }
      cors_loading.set(false);
    });
  };

  let remove_origin = move |origin: String| {
    cors_loading.set(true);
    let mut origins = cors_settings.get().origins;
    origins.retain(|o| o != &origin);

    // If empty, default to permissive
    if origins.is_empty() {
      origins.push("*".to_string());
    }

    spawn_local(async move {
      match apiclient::update_cors_settings(origins).await {
        Ok(settings) => {
          cors_settings.set(settings);
          let st = state_stored.get_value();
          st.show_toast(
            "CORS origin removed (restart required)",
            ToastLevel::Success,
          );
        }
        Err(e) => {
          let st = state_stored.get_value();
          st.show_toast(
            &format!("Failed to remove origin: {}", e),
            ToastLevel::Error,
          );
        }
      }
      cors_loading.set(false);
    });
  };

  let set_permissive = move |_| {
    cors_loading.set(true);
    spawn_local(async move {
      match apiclient::update_cors_settings(vec!["*".to_string()]).await {
        Ok(settings) => {
          cors_settings.set(settings);
          let st = state_stored.get_value();
          st.show_toast(
            "CORS set to permissive (restart required)",
            ToastLevel::Success,
          );
        }
        Err(e) => {
          let st = state_stored.get_value();
          st.show_toast(&format!("Failed to update: {}", e), ToastLevel::Error);
        }
      }
      cors_loading.set(false);
    });
  };

  let set_restricted = move |_| {
    cors_loading.set(true);
    // Start with empty list - user will add specific origins
    spawn_local(async move {
      match apiclient::update_cors_settings(vec![]).await {
        Ok(settings) => {
          cors_settings.set(settings);
          let st = state_stored.get_value();
          st.show_toast(
            "CORS set to restricted mode - add allowed origins (restart required)",
            ToastLevel::Success,
          );
        }
        Err(e) => {
          let st = state_stored.get_value();
          st.show_toast(&format!("Failed to update: {}", e), ToastLevel::Error);
        }
      }
      cors_loading.set(false);
    });
  };

  let on_restart_click = move |_| {
    restarting.set(true);
    restart_logs.set(Vec::new());
    add_log("Starting server restart...");

    spawn_local(async move {
      add_log("Sending restart command to server...");

      match apiclient::restart_server().await {
        Ok(_) => {
          add_log("✓ Restart command accepted");
          add_log("Server shutting down...");

          gloo_timers::future::TimeoutFuture::new(1500).await;

          add_log("Waiting for server to restart...");

          let mut attempts = 0;
          let max_attempts = 30;

          loop {
            attempts += 1;
            if attempts > max_attempts {
              add_log("✗ Timeout waiting for server");
              add_log("Please refresh the page manually");
              restarting.set(false);
              break;
            }

            gloo_timers::future::TimeoutFuture::new(1000).await;

            add_log(&format!("Polling server... (attempt {})", attempts));

            if apiclient::health_check().await.is_ok() {
              add_log("✓ Server is back online!");
              add_log("Reloading page...");

              gloo_timers::future::TimeoutFuture::new(500).await;

              let st = state_stored.get_value();
              st.show_toast("Server restarted successfully", ToastLevel::Success);

              if let Some(window) = web_sys::window() {
                let _ = window.location().reload();
              }
              break;
            }
          }
        }
        Err(e) => {
          add_log(&format!("✗ Failed: {}", e));
          restarting.set(false);
          let st = state_stored.get_value();
          st.show_toast(&format!("Failed to restart: {}", e), ToastLevel::Error);
        }
      }
    });
  };

  let is_permissive = move || {
    let origins = cors_settings.get().origins;
    origins.len() == 1 && origins.first().map(|s| s.as_str()) == Some("*")
  };

  let toggle_backup = move |checked: bool| {
    backup_loading.set(true);
    spawn_local(async move {
      match apiclient::toggle_feature("backup", checked).await {
        Ok(_) => {
          let st = state_stored.get_value();
          st.show_toast(
            &format!(
              "Automatic backups {} (restart required)",
              if checked { "enabled" } else { "disabled" }
            ),
            ToastLevel::Success,
          );
          // Refresh backup settings
          if let Ok(settings) = apiclient::fetch_backup_settings().await {
            backup_settings.set(settings);
          }
        }
        Err(e) => {
          let st = state_stored.get_value();
          st.show_toast(&format!("Failed to update: {}", e), ToastLevel::Error);
        }
      }
      backup_loading.set(false);
    });
  };

  view! {
    <div class="settings-grid">
      // Protocols Card
      <div class="settings-card">
          <div class="settings-card-header">
            <h3>"Protocols"</h3>
            <span class="settings-card-description">"Enable or disable server protocols"</span>
          </div>
          <div class="settings-card-body">
            <div class="setting-row">
              <div class="setting-info">
                <span class="setting-label">"REST API"</span>
                <span class="setting-description">"HTTP REST endpoints for CRUD operations"</span>
              </div>
              <label class="toggle">
                <input
                  type="checkbox"
                  prop:checked=move || protocol_settings.get().rest
                  prop:disabled=move || loading.get()
                  on:change=move |ev| {
                    let checked = event_target_checked(&ev);
                    toggle_protocol("rest", checked);
                  }
                />
                <span class="toggle-slider"></span>
              </label>
            </div>
            <div class="setting-row">
              <div class="setting-info">
                <span class="setting-label">"WebSocket"</span>
                <span class="setting-description">"Real-time subscriptions and queries"</span>
              </div>
              <label class="toggle">
                <input
                  type="checkbox"
                  prop:checked=move || protocol_settings.get().websocket
                  prop:disabled=move || loading.get()
                  on:change=move |ev| {
                    let checked = event_target_checked(&ev);
                    toggle_protocol("websocket", checked);
                  }
                />
                <span class="toggle-slider"></span>
              </label>
            </div>
            <div class="setting-row">
              <div class="setting-info">
                <span class="setting-label">"TCP Protocol"</span>
                <span class="setting-description">"Binary protocol for high-performance clients"</span>
              </div>
              <label class="toggle">
                <input
                  type="checkbox"
                  prop:checked=move || protocol_settings.get().tcp
                  prop:disabled=move || loading.get()
                  on:change=move |ev| {
                    let checked = event_target_checked(&ev);
                    toggle_protocol("tcp", checked);
                  }
                />
                <span class="toggle-slider"></span>
              </label>
            </div>
            <div class="setting-row">
              <div class="setting-info">
                <span class="setting-label">"Server-Sent Events"</span>
                <span class="setting-description">"One-way real-time streaming"</span>
              </div>
              <label class="toggle">
                <input
                  type="checkbox"
                  prop:checked=move || protocol_settings.get().sse
                  prop:disabled=move || loading.get()
                  on:change=move |ev| {
                    let checked = event_target_checked(&ev);
                    toggle_protocol("sse", checked);
                  }
                />
                <span class="toggle-slider"></span>
              </label>
            </div>
            <div class="setting-row">
              <div class="setting-info">
                <span class="setting-label">"MCP (Model Context Protocol)"</span>
                <span class="setting-description">"AI/LLM integration protocol"</span>
              </div>
              <label class="toggle">
                <input
                  type="checkbox"
                  prop:checked=move || protocol_settings.get().mcp
                  prop:disabled=move || loading.get()
                  on:change=move |ev| {
                    let checked = event_target_checked(&ev);
                    toggle_protocol("mcp", checked);
                  }
                />
                <span class="toggle-slider"></span>
              </label>
            </div>
          </div>
          <div class="settings-card-footer">
            <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor" style="margin-right: 6px; opacity: 0.7;">
              <path d="M8 16A8 8 0 108 0a8 8 0 000 16zm.93-9.412l-1 4.705c-.07.34.029.533.304.533.194 0 .487-.07.686-.246l-.088.416c-.287.346-.92.598-1.465.598-.703 0-1.002-.422-.808-1.319l.738-3.468c.064-.293.006-.399-.287-.47l-.451-.081.082-.381 2.29-.287h.001zM8 5.5a1 1 0 110-2 1 1 0 010 2z"/>
            </svg>
            <span class="text-muted">"Protocol changes require server restart"</span>
          </div>
        </div>

      // CORS Settings Card
      <div class="settings-card">
        <div class="settings-card-header">
          <h3>"CORS (Cross-Origin Resource Sharing)"</h3>
          <span class="settings-card-description">"Control which web origins can access your API"</span>
        </div>
        <div class="settings-card-body">
          // Current mode indicator
          <div class="cors-mode-indicator">
            <Show
              when=move || is_permissive()
              fallback=move || view! {
                <div class="cors-mode cors-mode-restricted">
                  <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
                    <path d="M8 1a2 2 0 012 2v4H6V3a2 2 0 012-2zm3 6V3a3 3 0 00-6 0v4a2 2 0 00-2 2v5a2 2 0 002 2h6a2 2 0 002-2V9a2 2 0 00-2-2z"/>
                  </svg>
                  <span>"Restricted Mode"</span>
                  <span class="cors-mode-desc">" — Only listed origins can access the API"</span>
                </div>
              }
            >
              <div class="cors-mode cors-mode-permissive">
                <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
                  <path d="M11 1a2 2 0 00-2 2v4a2 2 0 012 2v5a2 2 0 01-2 2H3a2 2 0 01-2-2V9a2 2 0 012-2h1V3a3 3 0 116 0v4a.5.5 0 01-1 0V3a2 2 0 00-2-2H4a3 3 0 00-3 3v4h-.5a.5.5 0 000 1H4v5a1 1 0 001 1h6a1 1 0 001-1V9a1 1 0 00-1-1z"/>
                </svg>
                <span>"Permissive Mode"</span>
                <span class="cors-mode-desc">" — Any origin can access the API"</span>
              </div>
            </Show>
          </div>

          // Origins list
          <Show when=move || !is_permissive()>
            <div class="cors-origins-list">
              <label class="form-label">"Allowed Origins"</label>
              <For
                each=move || cors_settings.get().origins
                key=|o| o.clone()
                children=move |origin: String| {
                  let origin_for_remove = origin.clone();
                  view! {
                    <div class="cors-origin-item">
                      <code class="cors-origin-url">{origin}</code>
                      <button
                        class="cors-origin-remove"
                        on:click=move |_| remove_origin(origin_for_remove.clone())
                        disabled=move || cors_loading.get()
                        title="Remove origin"
                      >
                        <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
                          <path d="M4.646 4.646a.5.5 0 01.708 0L8 7.293l2.646-2.647a.5.5 0 01.708.708L8.707 8l2.647 2.646a.5.5 0 01-.708.708L8 8.707l-2.646 2.647a.5.5 0 01-.708-.708L7.293 8 4.646 5.354a.5.5 0 010-.708z"/>
                        </svg>
                      </button>
                    </div>
                  }
                }
              />
            </div>
          </Show>

          // Add origin form
          <div class="cors-add-origin">
            <Show
              when=move || is_permissive()
              fallback=move || view! {
                <div class="cors-add-form">
                  <input
                    type="text"
                    class="form-input"
                    placeholder="https://example.com"
                    prop:value=move || new_origin.get()
                    on:input=move |ev| new_origin.set(event_target_value(&ev))
                    on:keypress=move |ev| {
                      if ev.key() == "Enter" {
                        do_add_origin();
                      }
                    }
                    disabled=move || cors_loading.get()
                  />
                  <button
                    class="btn btn-primary"
                    on:click=move |_| do_add_origin()
                    disabled=move || cors_loading.get() || new_origin.get().trim().is_empty()
                  >
                    "Add Origin"
                  </button>
                </div>
                <button
                  class="btn btn-secondary btn-sm"
                  on:click=set_permissive
                  disabled=move || cors_loading.get()
                  style="margin-top: 8px;"
                >
                  "Switch to Permissive Mode"
                </button>
              }
            >
              <p class="cors-permissive-note">
                "All origins are currently allowed."
              </p>
              <button
                class="btn btn-secondary btn-sm"
                on:click=set_restricted
                disabled=move || cors_loading.get()
                style="margin-top: 8px;"
              >
                "Switch to Restricted Mode"
              </button>
            </Show>
          </div>
        </div>
        <div class="settings-card-footer">
          <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor" style="margin-right: 6px; opacity: 0.7;">
            <path d="M8 16A8 8 0 108 0a8 8 0 000 16zm.93-9.412l-1 4.705c-.07.34.029.533.304.533.194 0 .487-.07.686-.246l-.088.416c-.287.346-.92.598-1.465.598-.703 0-1.002-.422-.808-1.319l.738-3.468c.064-.293.006-.399-.287-.47l-.451-.081.082-.381 2.29-.287h.001zM8 5.5a1 1 0 110-2 1 1 0 010 2z"/>
          </svg>
          <span class="text-muted">"CORS changes require server restart"</span>
        </div>
      </div>

      // Backup Settings Card
      <div class="settings-card">
        <div class="settings-card-header">
          <h3>"Database Backups"</h3>
          <span class="settings-card-description">"Automatic database backup configuration"</span>
        </div>
        <div class="settings-card-body">
          <div class="setting-row">
            <div class="setting-info">
              <span class="setting-label">"Enable Automatic Backups"</span>
              <span class="setting-description">
                {move || {
                  if backup_settings.get().storage_enabled {
                    "Backups will be stored in S3 Storage (/backups)"
                  } else {
                    "Backups will be stored locally in ./backup/"
                  }
                }}
              </span>
            </div>
            <label class="toggle">
              <input
                type="checkbox"
                prop:checked=move || backup_settings.get().enabled
                prop:disabled=move || backup_loading.get()
                on:change=move |ev| {
                  let checked = event_target_checked(&ev);
                  toggle_backup(checked);
                }
              />
              <span class="toggle-slider"></span>
            </label>
          </div>

          <Show when=move || backup_settings.get().enabled>
            <div class="backup-info">
              <div class="backup-info-row">
                <span class="backup-info-label">"Backup Interval:"</span>
                <span class="backup-info-value">
                  {move || {
                    let interval = backup_settings.get().interval;
                    if interval >= 3600 {
                      format!("{} hour(s)", interval / 3600)
                    } else if interval >= 60 {
                      format!("{} minute(s)", interval / 60)
                    } else {
                      format!("{} seconds", interval)
                    }
                  }}
                </span>
              </div>
              <div class="backup-info-row">
                <span class="backup-info-label">"Retention:"</span>
                <span class="backup-info-value">
                  {move || format!("{} backups", backup_settings.get().retention)}
                </span>
              </div>
              <div class="backup-info-row">
                <span class="backup-info-label">"Storage:"</span>
                <span class="backup-info-value">
                  {move || {
                    if backup_settings.get().storage_enabled {
                      format!("S3: /{}", backup_settings.get().storage_path)
                    } else {
                      backup_settings.get().local_path.clone()
                    }
                  }}
                </span>
              </div>
              <Show when=move || backup_settings.get().last_backup.is_some()>
                <div class="backup-info-row">
                  <span class="backup-info-label">"Last Backup:"</span>
                  <span class="backup-info-value">
                    {move || backup_settings.get().last_backup.unwrap_or_else(|| "Never".to_string())}
                  </span>
                </div>
              </Show>
              <Show when=move || backup_settings.get().next_backup.is_some()>
                <div class="backup-info-row">
                  <span class="backup-info-label">"Next Backup:"</span>
                  <span class="backup-info-value">
                    {move || backup_settings.get().next_backup.unwrap_or_else(|| "Pending".to_string())}
                  </span>
                </div>
              </Show>
            </div>
          </Show>
        </div>
        <div class="settings-card-footer">
          <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor" style="margin-right: 6px; opacity: 0.7;">
            <path d="M8 16A8 8 0 108 0a8 8 0 000 16zm.93-9.412l-1 4.705c-.07.34.029.533.304.533.194 0 .487-.07.686-.246l-.088.416c-.287.346-.92.598-1.465.598-.703 0-1.002-.422-.808-1.319l.738-3.468c.064-.293.006-.399-.287-.47l-.451-.081.082-.381 2.29-.287h.001zM8 5.5a1 1 0 110-2 1 1 0 010 2z"/>
          </svg>
          <span class="text-muted">"Backup changes require server restart"</span>
        </div>
      </div>

      // Server Control Card
      <div class="settings-card">
        <div class="settings-card-header">
          <h3>"Server Control"</h3>
          <span class="settings-card-description">"Manage server lifecycle"</span>
        </div>
        <div class="settings-card-body">
          // Restart log view
          <Show when=move || !restart_logs.get().is_empty()>
            <div class="restart-log">
              <div class="restart-log-header">
                <span class="restart-log-title">"Restart Log"</span>
                <Show when=move || restarting.get()>
                  <span class="restart-log-status">"Running..."</span>
                </Show>
              </div>
              <div class="restart-log-content">
                <For
                  each=move || restart_logs.get()
                  key=|(ts, msg)| format!("{}-{}", ts, msg)
                  children=move |(timestamp, message)| {
                    view! {
                      <div class="restart-log-line">
                        <span class="restart-log-time">{timestamp}</span>
                        <span class="restart-log-msg">{message}</span>
                      </div>
                    }
                  }
                />
              </div>
            </div>
          </Show>

          <div class="setting-row">
            <div class="setting-info">
              <span class="setting-label">"Restart Server"</span>
              <span class="setting-description">"Apply pending configuration changes by restarting the server"</span>
            </div>
            <button
              class="btn btn-warning"
              on:click=on_restart_click
              disabled=move || restarting.get()
            >
              <Show
                when=move || restarting.get()
                fallback=move || view! {
                  <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor" style="margin-right: 6px;">
                    <path d="M11.534 7h3.932a.25.25 0 01.192.41l-1.966 2.36a.25.25 0 01-.384 0l-1.966-2.36a.25.25 0 01.192-.41zm-11 2h3.932a.25.25 0 00.192-.41L2.692 6.23a.25.25 0 00-.384 0L.342 8.59A.25.25 0 00.534 9z"/>
                    <path fill-rule="evenodd" d="M8 3c-1.552 0-2.94.707-3.857 1.818a.5.5 0 11-.771-.636A6.002 6.002 0 0113.917 7H12.9A5.002 5.002 0 008 3zM3.1 9a5.002 5.002 0 008.757 2.182.5.5 0 11.771.636A6.002 6.002 0 012.083 9H3.1z"/>
                  </svg>
                  "Restart Server"
                }
              >
                <span class="btn-spinner"></span>
                "Restarting..."
              </Show>
            </button>
          </div>
        </div>
      </div>
    </div>
  }
}
