//! Storage (S3) settings tab

use leptos::*;
use crate::admin::state::{AppState, ToastLevel};
use crate::admin::apiclient;

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
            if checked { "Storage enabled" } else { "Storage disabled" },
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
      match apiclient::update_storage_settings(port_val, Some(path_val.clone()), Some(region_val.clone())).await {
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
    </div>
  }
}
