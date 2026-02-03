//! API Access settings tab

use crate::admin::apiclient;
use crate::admin::state::{AppState, ToastLevel, TokenInfo};
use leptos::*;

#[component]
pub fn TokensSettings() -> impl IntoView {
  let state = use_context::<AppState>().expect("AppState not found");
  let projects = state.projects;
  let current_project = state.current_project;
  let tokens = state.tokens;
  let api_auth_required = state.api_auth_required;

  let loading = create_rw_signal(false);
  let auth_loading = create_rw_signal(false);
  let show_create_modal = create_rw_signal(false);
  let new_token_name = create_rw_signal(String::new());
  let generated_token = create_rw_signal::<Option<String>>(None);
  let copied = create_rw_signal(false);

  let state_stored = store_value(state.clone());

  // Load auth settings on mount
  create_effect(move |_| {
    spawn_local(async move {
      if let Ok(auth_required) = apiclient::fetch_auth_settings().await {
        api_auth_required.set(auth_required);
      }
    });
  });

  let load_tokens = move || {
    if let Some(project_id) = current_project.get() {
      loading.set(true);
      spawn_local(async move {
        match apiclient::fetch_tokens(&project_id).await {
          Ok(fetched_tokens) => {
            tokens.set(fetched_tokens);
          }
          Err(e) => {
            let st = state_stored.get_value();
            st.show_toast(&format!("Failed to load tokens: {}", e), ToastLevel::Error);
          }
        }
        loading.set(false);
      });
    }
  };

  create_effect(move |_| {
    let _ = current_project.get();
    load_tokens();
  });

  let on_toggle_auth = move |ev: web_sys::Event| {
    let checked = event_target_checked(&ev);
    auth_loading.set(true);
    spawn_local(async move {
      match apiclient::update_auth_settings(checked).await {
        Ok(new_value) => {
          api_auth_required.set(new_value);
          let st = state_stored.get_value();
          st.show_toast(
            if new_value {
              "Token authentication enabled"
            } else {
              "Token authentication disabled"
            },
            ToastLevel::Success,
          );
        }
        Err(e) => {
          let st = state_stored.get_value();
          st.show_toast(
            &format!("Failed to update auth settings: {}", e),
            ToastLevel::Error,
          );
        }
      }
      auth_loading.set(false);
    });
  };

  let on_create_token = move |_| {
    let name = new_token_name.get();
    if name.trim().is_empty() {
      let st = state_stored.get_value();
      st.show_toast("Token name is required", ToastLevel::Warning);
      return;
    }

    if let Some(project_id) = current_project.get() {
      let name_clone = name.clone();
      spawn_local(async move {
        match apiclient::create_token(&project_id, &name_clone).await {
          Ok(resp) => {
            if let Some(token) = resp.get("token").and_then(|v| v.as_str()) {
              generated_token.set(Some(token.to_string()));
            }
            let st = state_stored.get_value();
            st.show_toast("Token created successfully", ToastLevel::Success);
            load_tokens();
          }
          Err(e) => {
            let st = state_stored.get_value();
            st.show_toast(&format!("Failed to create token: {}", e), ToastLevel::Error);
          }
        }
      });
    }
  };

  let on_delete_token = move |token_id: String| {
    if let Some(project_id) = current_project.get() {
      spawn_local(async move {
        match apiclient::delete_token(&project_id, &token_id).await {
          Ok(_) => {
            let st = state_stored.get_value();
            st.show_toast("Token deleted", ToastLevel::Success);
            load_tokens();
          }
          Err(e) => {
            let st = state_stored.get_value();
            st.show_toast(&format!("Failed to delete token: {}", e), ToastLevel::Error);
          }
        }
      });
    }
  };

  let copy_token = move |_| {
    if let Some(token) = generated_token.get() {
      #[cfg(feature = "csr")]
      {
        if let Some(window) = web_sys::window() {
          let clipboard = window.navigator().clipboard();
          let _ = clipboard.write_text(&token);
          copied.set(true);
          let st = state_stored.get_value();
          st.show_toast("Token copied to clipboard", ToastLevel::Success);
        }
      }
    }
  };

  let close_modal = move |_| {
    show_create_modal.set(false);
    new_token_name.set(String::new());
    generated_token.set(None);
    copied.set(false);
  };

  view! {
    <div class="settings-grid">
      // Require Token Authentication Card
      <div class="settings-card settings-card-full">
        <div class="settings-card-header">
          <h3>"Authentication"</h3>
          <span class="settings-card-description">"Configure API authentication requirements"</span>
        </div>
        <div class="settings-card-body">
          <div class="setting-row">
            <div class="setting-info">
              <span class="setting-label">"Require Token Authentication"</span>
              <span class="setting-description">"When enabled, all API requests must include a valid token"</span>
            </div>
            <label class="toggle">
              <input
                type="checkbox"
                prop:checked=move || api_auth_required.get()
                prop:disabled=move || auth_loading.get()
                on:change=on_toggle_auth
              />
              <span class="toggle-slider"></span>
            </label>
          </div>
          <Show when=move || !api_auth_required.get()>
            <div class="setting-warning">
              <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
                <path d="M8.982 1.566a1.13 1.13 0 00-1.96 0L.165 13.233c-.457.778.091 1.767.98 1.767h13.713c.889 0 1.438-.99.98-1.767L8.982 1.566zM8 5c.535 0 .954.462.9.995l-.35 3.507a.552.552 0 01-1.1 0L7.1 5.995A.905.905 0 018 5zm.002 6a1 1 0 110 2 1 1 0 010-2z"/>
              </svg>
              <span>"Authentication is disabled. Your API is publicly accessible."</span>
            </div>
          </Show>
        </div>
      </div>

      // API Tokens Card
      <div class="settings-card settings-card-full">
        <div class="settings-card-header">
          <h3>"API Tokens"</h3>
          <span class="settings-card-description">"Manage API access tokens for your projects"</span>
        </div>
        <div class="settings-card-body">
          // Project selector
          <div class="token-project-selector">
            <label class="form-label">"Project"</label>
            <select
              class="form-select"
              on:change=move |ev| {
                let value = event_target_value(&ev);
                current_project.set(Some(value));
              }
            >
              <For
                each=move || projects.get()
                key=|p| p.id.clone()
                children=move |project| {
                  let project_id = project.id.clone();
                  let project_id_for_value = project_id.clone();
                  let project_name = project.name.clone();
                  let is_selected = move || current_project.get() == Some(project_id.clone());
                  view! {
                    <option value=project_id_for_value selected=is_selected>
                      {project_name}
                    </option>
                  }
                }
              />
            </select>
          </div>

          // Token actions
          <div class="token-actions">
            <button
              class="btn btn-primary"
              on:click=move |_| show_create_modal.set(true)
              disabled=move || current_project.get().is_none()
            >
              <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor" style="margin-right: 6px;">
                <path d="M8 4a.5.5 0 01.5.5v3h3a.5.5 0 010 1h-3v3a.5.5 0 01-1 0v-3h-3a.5.5 0 010-1h3v-3A.5.5 0 018 4z"/>
              </svg>
              "Generate Token"
            </button>
          </div>

          // Tokens list
          <Show
            when=move || loading.get()
            fallback=move || {
              let token_list = tokens.get();
              if token_list.is_empty() {
                view! {
                  <div class="empty-state tokens-empty">
                    <svg width="48" height="48" viewBox="0 0 16 16" fill="currentColor" class="empty-icon">
                      <path d="M0 8a4 4 0 017.465-2H14a.5.5 0 01.354.146l1.5 1.5a.5.5 0 010 .708l-1.5 1.5a.5.5 0 01-.708 0L13 9.207l-.646.647a.5.5 0 01-.708 0L11 9.207l-.646.647a.5.5 0 01-.708 0L9 9.207l-.646.647A.5.5 0 018 10h-.535A4 4 0 010 8zm4-3a3 3 0 100 6 3 3 0 000-6zM4 6a2 2 0 110 4 2 2 0 010-4z"/>
                    </svg>
                    <p>"No API tokens"</p>
                    <p class="text-muted">"Generate a token to enable authenticated API access"</p>
                  </div>
                }.into_view()
              } else {
                view! {
                  <div class="tokens-list">
                    <For
                      each=move || tokens.get()
                      key=|t| t.id.clone()
                      children=move |token: TokenInfo| {
                        let token_id = token.id.clone();
                        let token_id_for_delete = token.id.clone();
                        view! {
                          <div class="token-item">
                            <div class="token-info">
                              <span class="token-name">{token.name}</span>
                              <span class="token-id">{format!("ID: {}...", &token_id[..8.min(token_id.len())])}</span>
                              <span class="token-created">{format!("Created: {}", &token.created_at[..10.min(token.created_at.len())])}</span>
                            </div>
                            <button
                              class="btn btn-danger btn-sm"
                              on:click=move |_| {
                                on_delete_token(token_id_for_delete.clone());
                              }
                            >
                              "Delete"
                            </button>
                          </div>
                        }
                      }
                    />
                  </div>
                }.into_view()
              }
            }
          >
            <div class="loading-state">
              <span class="spinner"></span>
              <span>"Loading tokens..."</span>
            </div>
          </Show>
        </div>
      </div>
    </div>

    // Create Token Modal
    <Show when=move || show_create_modal.get()>
      <div class="modal-overlay" on:click=close_modal>
        <div class="modal" on:click=|e| e.stop_propagation()>
          <div class="modal-header">
            <h3>"Generate API Token"</h3>
            <button class="modal-close" on:click=close_modal>"×"</button>
          </div>
          <div class="modal-body">
            <Show
              when=move || generated_token.get().is_some()
              fallback=move || view! {
                <div class="form-group">
                  <label class="form-label">"Token Name"</label>
                  <input
                    type="text"
                    class="form-input"
                    placeholder="e.g., Production API Key"
                    prop:value=move || new_token_name.get()
                    on:input=move |ev| new_token_name.set(event_target_value(&ev))
                  />
                  <span class="form-hint">"Give your token a descriptive name"</span>
                </div>
              }
            >
              <div class="generated-token-section">
                <div class="token-warning">
                  <svg width="20" height="20" viewBox="0 0 16 16" fill="currentColor">
                    <path d="M8.982 1.566a1.13 1.13 0 00-1.96 0L.165 13.233c-.457.778.091 1.767.98 1.767h13.713c.889 0 1.438-.99.98-1.767L8.982 1.566zM8 5c.535 0 .954.462.9.995l-.35 3.507a.552.552 0 01-1.1 0L7.1 5.995A.905.905 0 018 5zm.002 6a1 1 0 110 2 1 1 0 010-2z"/>
                  </svg>
                  <span>"Copy this token now. You won't be able to see it again!"</span>
                </div>
                <div class="token-display">
                  <code class="token-value">{move || generated_token.get().unwrap_or_default()}</code>
                  <button class="btn btn-secondary btn-sm" on:click=copy_token>
                    {move || if copied.get() { "Copied!" } else { "Copy" }}
                  </button>
                </div>
              </div>
            </Show>
          </div>
          <div class="modal-footer">
            <Show
              when=move || generated_token.get().is_none()
              fallback=move || view! {
                <button class="btn btn-primary" on:click=close_modal>"Done"</button>
              }
            >
              <button class="btn btn-secondary" on:click=close_modal>"Cancel"</button>
              <button class="btn btn-primary" on:click=on_create_token>"Generate"</button>
            </Show>
          </div>
        </div>
      </div>
    </Show>
  }
}
