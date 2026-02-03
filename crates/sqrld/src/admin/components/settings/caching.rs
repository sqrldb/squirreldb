//! Caching settings tab

use super::super::Icon;
use crate::admin::apiclient;
use crate::admin::state::{AppState, ToastLevel};
use leptos::*;

#[component]
pub fn CachingSettings() -> impl IntoView {
  let state = use_context::<AppState>().expect("AppState not found");
  let cache_settings = state.cache_settings;

  let (enabled, set_enabled) = create_signal(false);
  let (port, set_port) = create_signal(String::from("6379"));
  let (max_memory, set_max_memory) = create_signal(String::from("256"));
  let (memory_unit, set_memory_unit) = create_signal(String::from("mb"));
  let (eviction, set_eviction) = create_signal(String::from("lru"));
  let (default_ttl, set_default_ttl) = create_signal(String::from("0"));
  let (snapshot_enabled, set_snapshot_enabled) = create_signal(false);
  let (snapshot_path, set_snapshot_path) = create_signal(String::from("./data/cache.snapshot"));
  let (snapshot_interval, set_snapshot_interval) = create_signal(String::from("300"));
  let (saving, set_saving) = create_signal(false);
  let (toggling, set_toggling) = create_signal(false);

  // Proxy mode settings
  let (mode, set_mode) = create_signal(String::from("builtin"));
  let (proxy_host, set_proxy_host) = create_signal(String::from("localhost"));
  let (proxy_port, set_proxy_port) = create_signal(String::from("6379"));
  let (proxy_password, set_proxy_password) = create_signal(String::new());
  let (proxy_database, set_proxy_database) = create_signal(String::from("0"));
  let (proxy_tls_enabled, set_proxy_tls_enabled) = create_signal(false);
  let (testing_connection, set_testing_connection) = create_signal(false);
  let (connection_status, set_connection_status) =
    create_signal(Option::<Result<(), String>>::None);

  // Sync with state on load
  create_effect(move |_| {
    let settings = cache_settings.get();
    set_enabled.set(settings.enabled);
    state.cache_enabled.set(settings.enabled);
    set_port.set(settings.port.to_string());

    // Parse max_memory into value and unit
    let mem = settings.max_memory.to_lowercase();
    if mem.ends_with("gb") {
      set_max_memory.set(mem.trim_end_matches("gb").to_string());
      set_memory_unit.set("gb".to_string());
    } else if mem.ends_with("mb") {
      set_max_memory.set(mem.trim_end_matches("mb").to_string());
      set_memory_unit.set("mb".to_string());
    } else {
      set_max_memory.set(mem);
      set_memory_unit.set("mb".to_string());
    }

    set_eviction.set(settings.eviction.clone());
    set_default_ttl.set(settings.default_ttl.to_string());
    set_snapshot_enabled.set(settings.snapshot_enabled);
    set_snapshot_path.set(settings.snapshot_path.clone());
    set_snapshot_interval.set(settings.snapshot_interval.to_string());
    set_mode.set(settings.mode.clone());
    set_proxy_host.set(settings.proxy_host.clone());
    set_proxy_port.set(settings.proxy_port.to_string());
    set_proxy_database.set(settings.proxy_database.to_string());
    set_proxy_tls_enabled.set(settings.proxy_tls_enabled);
  });

  let state_toggle = state.clone();
  let on_toggle = move |ev: web_sys::Event| {
    let checked = event_target_checked(&ev);
    set_enabled.set(checked);
    set_toggling.set(true);
    let state = state_toggle.clone();
    spawn_local(async move {
      match apiclient::toggle_feature("caching", checked).await {
        Ok(_) => {
          state.cache_enabled.set(checked);
          state.show_toast(
            if checked {
              "Cache enabled"
            } else {
              "Cache disabled"
            },
            ToastLevel::Success,
          );
        }
        Err(e) => {
          set_enabled.set(!checked); // Revert
          state.show_toast(&format!("Failed: {}", e), ToastLevel::Error);
        }
      }
      set_toggling.set(false);
    });
  };

  let state_save = state.clone();
  let on_save = store_value(move |_| {
    set_saving.set(true);
    let port_val: Option<u16> = port.get().parse().ok();
    let max_mem = format!("{}{}", max_memory.get(), memory_unit.get());
    let eviction_val = eviction.get();
    let ttl_val: u64 = default_ttl.get().parse().unwrap_or(0);
    let snap_enabled = snapshot_enabled.get();
    let snap_path = snapshot_path.get();
    let snap_interval: u64 = snapshot_interval.get().parse().unwrap_or(300);
    let mode_val = mode.get();
    let proxy_host_val = proxy_host.get();
    let proxy_port_val: u16 = proxy_port.get().parse().unwrap_or(6379);
    let proxy_password_val = proxy_password.get();
    let proxy_database_val: u8 = proxy_database.get().parse().unwrap_or(0);
    let proxy_tls_val = proxy_tls_enabled.get();
    let state = state_save.clone();
    let is_running = enabled.get();

    spawn_local(async move {
      match apiclient::update_cache_settings_extended(
        port_val,
        Some(max_mem.clone()),
        Some(eviction_val.clone()),
        Some(ttl_val),
        Some(snap_enabled),
        Some(snap_path.clone()),
        Some(snap_interval),
        Some(mode_val.clone()),
        Some(proxy_host_val.clone()),
        Some(proxy_port_val),
        if proxy_password_val.is_empty() {
          None
        } else {
          Some(proxy_password_val)
        },
        Some(proxy_database_val),
        Some(proxy_tls_val),
      )
      .await
      {
        Ok(_) => {
          // Update local state with new settings
          state.cache_settings.update(|s| {
            if let Some(p) = port_val {
              s.port = p;
            }
            s.max_memory = max_mem;
            s.eviction = eviction_val;
            s.default_ttl = ttl_val;
            s.snapshot_enabled = snap_enabled;
            s.snapshot_path = snap_path;
            s.snapshot_interval = snap_interval;
            s.mode = mode_val;
            s.proxy_host = proxy_host_val;
            s.proxy_port = proxy_port_val;
            s.proxy_database = proxy_database_val;
            s.proxy_tls_enabled = proxy_tls_val;
          });
          if is_running {
            state.show_toast("Settings saved and cache restarted", ToastLevel::Success);
          } else {
            state.show_toast("Settings saved", ToastLevel::Success);
          }
        }
        Err(e) => {
          state.show_toast(&format!("Failed to save: {}", e), ToastLevel::Error);
        }
      }
      set_saving.set(false);
    });
  });

  // Test proxy connection
  let state_test = state.clone();
  let on_test_connection = store_value(move |_| {
    set_testing_connection.set(true);
    set_connection_status.set(None);
    let host = proxy_host.get();
    let port: u16 = proxy_port.get().parse().unwrap_or(6379);
    let password = proxy_password.get();
    let database: u8 = proxy_database.get().parse().unwrap_or(0);
    let tls_enabled = proxy_tls_enabled.get();
    let state = state_test.clone();

    spawn_local(async move {
      match apiclient::test_cache_connection(
        &host,
        port,
        if password.is_empty() {
          None
        } else {
          Some(&password)
        },
        database,
        tls_enabled,
      )
      .await
      {
        Ok(_) => {
          set_connection_status.set(Some(Ok(())));
          state.show_toast("Connection successful", ToastLevel::Success);
        }
        Err(e) => {
          set_connection_status.set(Some(Err(e.clone())));
          state.show_toast(&format!("Connection failed: {}", e), ToastLevel::Error);
        }
      }
      set_testing_connection.set(false);
    });
  });

  let (flushing, set_flushing) = create_signal(false);
  let flush_state = store_value(state.clone());

  // Load stats periodically when enabled
  let state_stats = state.clone();
  create_effect(move |_| {
    if enabled.get() {
      let state = state_stats.clone();
      spawn_local(async move {
        if let Ok(stats) = apiclient::fetch_cache_stats().await {
          state.cache_stats.set(stats);
        }
      });
    }
  });

  view! {
    <div class="settings-grid">
      // Enable/Disable Card
      <div class="settings-card">
        <div class="settings-card-header">
          <h3>"In-Memory Cache"</h3>
          <span class="settings-card-description">"Redis-compatible caching service"</span>
        </div>
        <div class="settings-card-body">
          <div class="setting-row">
            <div class="setting-info">
              <span class="setting-label">"Enable Caching"</span>
              <span class="setting-description">"Run Redis-compatible cache server"</span>
            </div>
            <label class="toggle">
              <input
                type="checkbox"
                prop:checked=enabled
                on:change=on_toggle
                disabled=move || toggling.get() || saving.get()
              />
              <span class="toggle-slider"></span>
            </label>
          </div>
        </div>
        <Show when=move || enabled.get()>
          <div class="settings-card-footer">
            <span class="status-badge success">
              <span class="status-dot"></span>
              {move || if toggling.get() { "Starting..." } else { "Running" }}
            </span>
          </div>
        </Show>
      </div>

      // Mode Selection Card
      <div class="settings-card">
        <div class="settings-card-header">
          <h3>"Cache Mode"</h3>
          <span class="settings-card-description">"Choose built-in or external Redis cache"</span>
        </div>
        <div class="settings-card-body">
          <div class="mode-toggle">
            <button
              class=move || if mode.get() == "builtin" { "mode-btn active" } else { "mode-btn" }
              on:click=move |_| set_mode.set("builtin".to_string())
              disabled=move || saving.get()
            >
              <Icon name="cpu" size=20/>
              <span>"Built-in"</span>
              <small>"In-memory cache"</small>
            </button>
            <button
              class=move || if mode.get() == "proxy" { "mode-btn active" } else { "mode-btn" }
              on:click=move |_| set_mode.set("proxy".to_string())
              disabled=move || saving.get()
            >
              <Icon name="server" size=20/>
              <span>"Proxy"</span>
              <small>"External Redis server"</small>
            </button>
          </div>
        </div>
      </div>

      // Built-in Configuration Card
      <Show when=move || mode.get() == "builtin">
        <div class="settings-card">
          <div class="settings-card-header">
            <h3>"Built-in Configuration"</h3>
            <span class="settings-card-description">"In-memory cache settings"</span>
          </div>
          <div class="settings-card-body">
            <div class="settings-form">
              <div class="form-group">
                <label>"Port"</label>
                <input
                  type="number"
                  class="input"
                  prop:value=port
                  on:input=move |ev| set_port.set(event_target_value(&ev))
                  disabled=move || saving.get()
                />
                <p class="form-hint">"Port for Redis protocol (default: 6379)"</p>
              </div>
              <div class="form-group">
                <label>"Max Memory"</label>
                <div class="input-group">
                  <input
                    type="number"
                    class="input"
                    style="width: 120px"
                    prop:value=max_memory
                    on:input=move |ev| set_max_memory.set(event_target_value(&ev))
                    disabled=move || saving.get()
                  />
                  <select
                    class="input"
                    style="width: 80px"
                    prop:value=memory_unit
                    on:change=move |ev| set_memory_unit.set(event_target_value(&ev))
                    disabled=move || saving.get()
                  >
                    <option value="mb">"MB"</option>
                    <option value="gb">"GB"</option>
                  </select>
                </div>
                <p class="form-hint">"Maximum memory for cached data"</p>
              </div>
              <div class="form-group">
                <label>"Eviction Policy"</label>
                <select
                  class="input"
                  prop:value=eviction
                  on:change=move |ev| set_eviction.set(event_target_value(&ev))
                  disabled=move || saving.get()
                >
                  <option value="lru">"LRU (Least Recently Used)"</option>
                  <option value="lfu">"LFU (Least Frequently Used)"</option>
                  <option value="random">"Random"</option>
                  <option value="noeviction">"No Eviction (Error on full)"</option>
                </select>
                <p class="form-hint">"Policy when memory limit is reached"</p>
              </div>
              <div class="form-group">
                <label>"Default TTL (seconds)"</label>
                <input
                  type="number"
                  class="input"
                  prop:value=default_ttl
                  on:input=move |ev| set_default_ttl.set(event_target_value(&ev))
                  disabled=move || saving.get()
                />
                <p class="form-hint">"Default expiry for keys (0 = no expiry)"</p>
              </div>
              <div class="form-actions">
                <button
                  class="btn btn-primary"
                  disabled=move || saving.get() || toggling.get()
                  on:click=move |e| on_save.get_value()(e)
                >
                  {move || {
                    if saving.get() {
                      if enabled.get() { "Saving & Restarting..." } else { "Saving..." }
                    } else if enabled.get() {
                      "Save & Restart"
                    } else {
                      "Save Changes"
                    }
                  }}
                </button>
              </div>
            </div>
          </div>
        </div>
      </Show>

      // Proxy Configuration Card
      <Show when=move || mode.get() == "proxy">
        <div class="settings-card settings-card-wide">
          <div class="settings-card-header">
            <h3>"Proxy Configuration"</h3>
            <span class="settings-card-description">"Connect to external Redis server"</span>
          </div>
          <div class="settings-card-body">
            <div class="settings-form">
              <div class="form-row">
                <div class="form-group">
                  <label>"Host"</label>
                  <input
                    type="text"
                    class="input"
                    placeholder="redis.example.com"
                    prop:value=proxy_host
                    on:input=move |ev| set_proxy_host.set(event_target_value(&ev))
                    disabled=move || saving.get()
                  />
                  <p class="form-hint">"Redis server hostname or IP"</p>
                </div>
                <div class="form-group">
                  <label>"Port"</label>
                  <input
                    type="number"
                    class="input"
                    placeholder="6379"
                    prop:value=proxy_port
                    on:input=move |ev| set_proxy_port.set(event_target_value(&ev))
                    disabled=move || saving.get()
                  />
                  <p class="form-hint">"Redis port (default: 6379)"</p>
                </div>
              </div>
              <div class="form-row">
                <div class="form-group">
                  <label>"Password"</label>
                  <input
                    type="password"
                    class="input"
                    placeholder="Enter password to change"
                    prop:value=proxy_password
                    on:input=move |ev| set_proxy_password.set(event_target_value(&ev))
                    disabled=move || saving.get()
                  />
                  <p class="form-hint">"Leave empty to keep existing or for no auth"</p>
                </div>
                <div class="form-group">
                  <label>"Database"</label>
                  <input
                    type="number"
                    class="input"
                    placeholder="0"
                    prop:value=proxy_database
                    on:input=move |ev| set_proxy_database.set(event_target_value(&ev))
                    disabled=move || saving.get()
                  />
                  <p class="form-hint">"Redis database number (0-15)"</p>
                </div>
              </div>
              <div class="form-group">
                <div class="setting-row">
                  <div class="setting-info">
                    <span class="setting-label">"Enable TLS"</span>
                    <span class="setting-description">"Use encrypted connection to Redis"</span>
                  </div>
                  <label class="toggle">
                    <input
                      type="checkbox"
                      prop:checked=proxy_tls_enabled
                      on:change=move |ev| set_proxy_tls_enabled.set(event_target_checked(&ev))
                      disabled=move || saving.get()
                    />
                    <span class="toggle-slider"></span>
                  </label>
                </div>
              </div>
              <div class="form-actions">
                <button
                  class="btn btn-secondary"
                  disabled=move || testing_connection.get() || proxy_host.get().is_empty()
                  on:click=move |e| on_test_connection.get_value()(e)
                >
                  {move || if testing_connection.get() { "Testing..." } else { "Test Connection" }}
                </button>
                <button
                  class="btn btn-primary"
                  disabled=move || saving.get() || toggling.get()
                  on:click=move |e| on_save.get_value()(e)
                >
                  {move || {
                    if saving.get() {
                      if enabled.get() { "Saving & Restarting..." } else { "Saving..." }
                    } else if enabled.get() {
                      "Save & Restart"
                    } else {
                      "Save Changes"
                    }
                  }}
                </button>
              </div>
              <Show when=move || connection_status.get().is_some()>
                {move || {
                  match connection_status.get() {
                    Some(Ok(())) => view! {
                      <div class="connection-status success">
                        <Icon name="check-circle" size=16/>
                        " Connected successfully"
                      </div>
                    }.into_view(),
                    Some(Err(e)) => view! {
                      <div class="connection-status error">
                        <Icon name="x-circle" size=16/>
                        " "{e}
                      </div>
                    }.into_view(),
                    None => view! { <span></span> }.into_view(),
                  }
                }}
              </Show>
            </div>
          </div>
        </div>
      </Show>

      // Snapshot Configuration Card (only for builtin mode)
      <Show when=move || mode.get() == "builtin">
      <div class="settings-card">
        <div class="settings-card-header">
          <h3>"Persistence"</h3>
          <span class="settings-card-description">"Snapshot persistence settings"</span>
        </div>
        <div class="settings-card-body">
          <div class="settings-form">
            <div class="setting-row">
              <div class="setting-info">
                <span class="setting-label">"Enable Snapshots"</span>
                <span class="setting-description">"Periodically save cache to disk"</span>
              </div>
              <label class="toggle">
                <input
                  type="checkbox"
                  prop:checked=snapshot_enabled
                  on:change=move |ev| set_snapshot_enabled.set(event_target_checked(&ev))
                  disabled=move || saving.get()
                />
                <span class="toggle-slider"></span>
              </label>
            </div>
            <Show when=move || snapshot_enabled.get()>
              <div class="form-group">
                <label>"Snapshot Path"</label>
                <input
                  type="text"
                  class="input"
                  prop:value=snapshot_path
                  on:input=move |ev| set_snapshot_path.set(event_target_value(&ev))
                  disabled=move || saving.get()
                />
              </div>
              <div class="form-group">
                <label>"Snapshot Interval (seconds)"</label>
                <input
                  type="number"
                  class="input"
                  prop:value=snapshot_interval
                  on:input=move |ev| set_snapshot_interval.set(event_target_value(&ev))
                  disabled=move || saving.get()
                />
              </div>
            </Show>
          </div>
        </div>
      </div>
      </Show>

      // Statistics Card (only when enabled and builtin mode)
      <Show when=move || enabled.get()>
        <div class="settings-card settings-card-wide">
          <div class="settings-card-header">
            <h3>"Statistics"</h3>
            <span class="settings-card-description">"Cache performance metrics"</span>
          </div>
          <div class="settings-card-body">
            <div class="stats-grid">
              <div class="stat-item">
                <span class="stat-label">"Keys"</span>
                <span class="stat-value">{move || state.cache_stats.get().keys.to_string()}</span>
              </div>
              <div class="stat-item">
                <span class="stat-label">"Memory Used"</span>
                <span class="stat-value">{move || format_memory(state.cache_stats.get().memory_used)}</span>
              </div>
              <div class="stat-item">
                <span class="stat-label">"Memory Limit"</span>
                <span class="stat-value">{move || format_memory(state.cache_stats.get().memory_limit)}</span>
              </div>
              <div class="stat-item">
                <span class="stat-label">"Hit Rate"</span>
                <span class="stat-value">{move || {
                  let stats = state.cache_stats.get();
                  let total = stats.hits + stats.misses;
                  if total == 0 {
                    "0%".to_string()
                  } else {
                    format!("{:.1}%", (stats.hits as f64 / total as f64) * 100.0)
                  }
                }}</span>
              </div>
              <div class="stat-item">
                <span class="stat-label">"Hits"</span>
                <span class="stat-value">{move || state.cache_stats.get().hits.to_string()}</span>
              </div>
              <div class="stat-item">
                <span class="stat-label">"Misses"</span>
                <span class="stat-value">{move || state.cache_stats.get().misses.to_string()}</span>
              </div>
              <div class="stat-item">
                <span class="stat-label">"Evictions"</span>
                <span class="stat-value">{move || state.cache_stats.get().evictions.to_string()}</span>
              </div>
              <div class="stat-item">
                <span class="stat-label">"Expired"</span>
                <span class="stat-value">{move || state.cache_stats.get().expired.to_string()}</span>
              </div>
            </div>
            <div class="form-actions" style="margin-top: 16px">
              <button
                class="btn btn-danger"
                disabled=move || flushing.get()
                on:click=move |_| {
                  set_flushing.set(true);
                  let state = flush_state.get_value();
                  spawn_local(async move {
                    match apiclient::flush_cache().await {
                      Ok(_) => {
                        state.show_toast("Cache flushed", ToastLevel::Success);
                        if let Ok(stats) = apiclient::fetch_cache_stats().await {
                          state.cache_stats.set(stats);
                        }
                      }
                      Err(e) => {
                        state.show_toast(&format!("Failed to flush: {}", e), ToastLevel::Error);
                      }
                    }
                    set_flushing.set(false);
                  });
                }
              >
                {move || if flushing.get() { "Flushing..." } else { "Flush Cache" }}
              </button>
            </div>
          </div>
        </div>
      </Show>
    </div>
  }
}

fn format_memory(bytes: usize) -> String {
  const GB: usize = 1024 * 1024 * 1024;
  const MB: usize = 1024 * 1024;
  const KB: usize = 1024;

  if bytes >= GB {
    format!("{:.1} GB", bytes as f64 / GB as f64)
  } else if bytes >= MB {
    format!("{:.1} MB", bytes as f64 / MB as f64)
  } else if bytes >= KB {
    format!("{:.1} KB", bytes as f64 / KB as f64)
  } else {
    format!("{} B", bytes)
  }
}
