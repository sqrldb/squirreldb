//! Storage (S3) settings tab

use super::super::Icon;
use crate::admin::apiclient;
use crate::admin::state::{AppState, S3AccessKey, ToastLevel};
use leptos::*;

#[component]
pub fn StorageSettings() -> impl IntoView {
  let state = use_context::<AppState>().expect("AppState not found");
  let storage_settings = state.storage_settings;

  let (enabled, set_enabled) = create_signal(false);
  let (port, set_port) = create_signal(String::from("9000"));
  let (storage_path, set_storage_path) = create_signal(String::from("./data/s3"));
  let (region, set_region) = create_signal(String::from("us-east-1"));
  let (saving, set_saving) = create_signal(false);
  let (toggling, set_toggling) = create_signal(false);

  // Sync with state on load
  create_effect(move |_| {
    let settings = storage_settings.get();
    set_enabled.set(settings.enabled);
    state.storage_enabled.set(settings.enabled);
    set_port.set(settings.port.to_string());
    set_storage_path.set(settings.storage_path.clone());
    set_region.set(settings.region.clone());
  });

  let state_toggle = state.clone();
  let on_toggle = move |ev: web_sys::Event| {
    let checked = event_target_checked(&ev);
    set_enabled.set(checked);
    set_toggling.set(true);
    let state = state_toggle.clone();
    spawn_local(async move {
      match apiclient::toggle_feature("storage", checked).await {
        Ok(_) => {
          state.storage_enabled.set(checked);
          state.show_toast(
            if checked {
              "Storage enabled"
            } else {
              "Storage disabled"
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
  let on_save = move |_| {
    set_saving.set(true);
    let port_val: Option<u16> = port.get().parse().ok();
    let path_val = storage_path.get();
    let region_val = region.get();
    let state = state_save.clone();
    let is_running = enabled.get();

    spawn_local(async move {
      match apiclient::update_storage_settings(
        port_val,
        Some(path_val.clone()),
        Some(region_val.clone()),
      )
      .await
      {
        Ok(_) => {
          // Update local state with new settings
          state.storage_settings.update(|s| {
            if let Some(p) = port_val {
              s.port = p;
            }
            s.storage_path = path_val;
            s.region = region_val;
          });
          if is_running {
            state.show_toast("Settings saved and S3 restarted", ToastLevel::Success);
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
  };

  view! {
    <div class="settings-grid">
      // Enable/Disable Card
      <div class="settings-card">
        <div class="settings-card-header">
          <h3>"Object Storage"</h3>
          <span class="settings-card-description">"S3-compatible storage service"</span>
        </div>
        <div class="settings-card-body">
          <div class="setting-row">
            <div class="setting-info">
              <span class="setting-label">"Enable Storage"</span>
              <span class="setting-description">"Run S3-compatible object storage server"</span>
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

      // Configuration Card
      <div class="settings-card">
        <div class="settings-card-header">
          <h3>"Configuration"</h3>
          <span class="settings-card-description">"S3 server settings (saved to database)"</span>
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
              <p class="form-hint">"Port for the S3 API endpoint"</p>
            </div>
            <div class="form-group">
              <label>"Storage Path"</label>
              <input
                type="text"
                class="input"
                prop:value=storage_path
                on:input=move |ev| set_storage_path.set(event_target_value(&ev))
                disabled=move || saving.get()
              />
              <p class="form-hint">"Directory where objects will be stored"</p>
            </div>
            <div class="form-group">
              <label>"Region"</label>
              <input
                type="text"
                class="input"
                prop:value=region
                on:input=move |ev| set_region.set(event_target_value(&ev))
                disabled=move || saving.get()
              />
              <p class="form-hint">"AWS region name for S3 compatibility"</p>
            </div>
            <div class="form-actions">
              <button
                class="btn btn-primary"
                disabled=move || saving.get() || toggling.get()
                on:click=on_save
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
        <Show when=move || enabled.get()>
          <div class="settings-card-footer">
            <p class="form-hint">"Saving will restart the S3 server with new settings"</p>
          </div>
        </Show>
      </div>

      // Access Keys Card
      <AccessKeysCard/>
    </div>
  }
}

/// Access keys management component
#[component]
fn AccessKeysCard() -> impl IntoView {
  let state = use_context::<AppState>().expect("AppState not found");

  let (keys, set_keys) = create_signal(Vec::<S3AccessKey>::new());
  let (loading, set_loading) = create_signal(true);
  let (show_create, set_show_create) = create_signal(false);
  let (new_key_name, set_new_key_name) = create_signal(String::new());
  let (creating, set_creating) = create_signal(false);
  let (new_key_result, set_new_key_result) = create_signal(None::<(String, String)>);

  // Load keys on mount
  {
    let state = state.clone();
    create_effect(move |_| {
      let state = state.clone();
      let set_keys = set_keys;
      let set_loading = set_loading;
      spawn_local(async move {
        match apiclient::fetch_s3_keys().await {
          Ok(list) => set_keys.set(list),
          Err(e) => state.show_toast(&format!("Failed to load keys: {}", e), ToastLevel::Error),
        }
        set_loading.set(false);
      });
    });
  }

  view! {
    <div class="settings-card settings-card-wide">
      <div class="settings-card-header">
        <h3>"Access Keys"</h3>
        <span class="settings-card-description">"AWS Signature V4 credentials for S3 API access"</span>
      </div>
      <div class="settings-card-body">
        <Show when=move || loading.get()>
          <div class="loading-spinner"></div>
          " Loading..."
        </Show>

        <Show when=move || !loading.get()>
          // Create Key Button
          <Show when=move || !show_create.get() && new_key_result.get().is_none()>
            <button class="btn btn-primary btn-sm" on:click=move |_| set_show_create.set(true)>
              <Icon name="plus" size=14/>
              " Create Access Key"
            </button>
          </Show>

          // Create Key Form
          <Show when=move || show_create.get() && new_key_result.get().is_none()>
            <CreateKeyForm
              new_key_name=new_key_name
              set_new_key_name=set_new_key_name
              creating=creating
              set_creating=set_creating
              set_show_create=set_show_create
              set_new_key_result=set_new_key_result
              set_keys=set_keys
            />
          </Show>

          // New Key Result (show once after creation)
          <Show when=move || new_key_result.get().is_some()>
            {move || {
              let (access_key_id, secret_key) = new_key_result.get().unwrap();
              view! {
                <div class="key-created-alert">
                  <div class="alert-header">
                    <Icon name="check-circle" size=20/>
                    <strong>"Access key created successfully"</strong>
                  </div>
                  <p class="alert-warning">
                    "Save these credentials now. The secret key will not be shown again."
                  </p>
                  <div class="credential-row">
                    <span class="credential-label">"Access Key ID:"</span>
                    <code class="credential-value">{access_key_id.clone()}</code>
                  </div>
                  <div class="credential-row">
                    <span class="credential-label">"Secret Access Key:"</span>
                    <code class="credential-value">{secret_key}</code>
                  </div>
                  <button
                    class="btn btn-secondary btn-sm"
                    on:click=move |_| set_new_key_result.set(None)
                  >
                    "Done"
                  </button>
                </div>
              }
            }}
          </Show>

          // Keys Table
          <Show when=move || !keys.get().is_empty() && new_key_result.get().is_none()>
            <table class="data-table" style="margin-top: 16px">
              <thead>
                <tr>
                  <th>"Name"</th>
                  <th>"Access Key ID"</th>
                  <th>"Created"</th>
                  <th>"Actions"</th>
                </tr>
              </thead>
              <tbody>
                <For
                  each=move || keys.get()
                  key=|k| k.access_key_id.clone()
                  children=move |key| {
                    let key_id = key.access_key_id.clone();
                    view! {
                      <tr>
                        <td>{key.name.clone()}</td>
                        <td><code>{key.access_key_id.clone()}</code></td>
                        <td>{format_date(&key.created_at)}</td>
                        <td>
                          <DeleteKeyButton key_id=key_id set_keys=set_keys/>
                        </td>
                      </tr>
                    }
                  }
                />
              </tbody>
            </table>
          </Show>

          // Empty State
          <Show when=move || keys.get().is_empty() && !loading.get() && new_key_result.get().is_none()>
            <p class="text-muted" style="margin-top: 12px">"No access keys yet"</p>
          </Show>
        </Show>
      </div>
    </div>
  }
}

/// Create key form component
#[component]
fn CreateKeyForm(
  new_key_name: ReadSignal<String>,
  set_new_key_name: WriteSignal<String>,
  creating: ReadSignal<bool>,
  set_creating: WriteSignal<bool>,
  set_show_create: WriteSignal<bool>,
  set_new_key_result: WriteSignal<Option<(String, String)>>,
  set_keys: WriteSignal<Vec<S3AccessKey>>,
) -> impl IntoView {
  let state = use_context::<AppState>().expect("AppState not found");

  view! {
    <div class="inline-form">
      <input
        type="text"
        class="input"
        placeholder="Key name (e.g., app-uploads)"
        prop:value=new_key_name
        on:input=move |ev| set_new_key_name.set(event_target_value(&ev))
      />
      <button
        class="btn btn-primary"
        disabled=move || creating.get()
        on:click=move |_| {
          let name = new_key_name.get();
          if name.is_empty() {
            state.show_toast("Key name is required", ToastLevel::Warning);
            return;
          }
          set_creating.set(true);
          let state = state.clone();
          spawn_local(async move {
            match apiclient::create_s3_key(&name).await {
              Ok(resp) => {
                let access_key_id = resp
                  .get("access_key_id")
                  .and_then(|v| v.as_str())
                  .unwrap_or("")
                  .to_string();
                let secret_key = resp
                  .get("secret_access_key")
                  .and_then(|v| v.as_str())
                  .unwrap_or("")
                  .to_string();
                set_new_key_result.set(Some((access_key_id, secret_key)));
                set_new_key_name.set(String::new());
                if let Ok(list) = apiclient::fetch_s3_keys().await {
                  set_keys.set(list);
                }
              }
              Err(e) => {
                state.show_toast(&format!("Failed to create key: {}", e), ToastLevel::Error);
              }
            }
            set_creating.set(false);
          });
        }
      >
        {move || if creating.get() { "Creating..." } else { "Create" }}
      </button>
      <button class="btn btn-secondary" on:click=move |_| set_show_create.set(false)>
        "Cancel"
      </button>
    </div>
  }
}

/// Delete key button component
#[component]
fn DeleteKeyButton(key_id: String, set_keys: WriteSignal<Vec<S3AccessKey>>) -> impl IntoView {
  let state = use_context::<AppState>().expect("AppState not found");
  let (deleting, set_deleting) = create_signal(false);

  view! {
    <button
      class="btn btn-ghost btn-sm text-danger"
      disabled=move || deleting.get()
      on:click=move |_| {
        let key_id = key_id.clone();
        let state = state.clone();
        set_deleting.set(true);
        spawn_local(async move {
          match apiclient::delete_s3_key(&key_id).await {
            Ok(_) => {
              state.show_toast("Access key deleted", ToastLevel::Success);
              if let Ok(list) = apiclient::fetch_s3_keys().await {
                set_keys.set(list);
              }
            }
            Err(e) => {
              state.show_toast(&format!("Failed to delete: {}", e), ToastLevel::Error);
            }
          }
          set_deleting.set(false);
        });
      }
    >
      <Icon name="trash-2" size=14/>
      {move || if deleting.get() { " Deleting..." } else { " Delete" }}
    </button>
  }
}

fn format_date(date_str: &str) -> String {
  // Parse ISO date and return friendly format
  if let Some(date_part) = date_str.split('T').next() {
    date_part.to_string()
  } else {
    date_str.to_string()
  }
}
