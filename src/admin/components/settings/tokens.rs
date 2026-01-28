//! API Tokens settings tab

use crate::admin::state::{AppState, ToastLevel};
use leptos::*;

#[component]
pub fn TokensSettings() -> impl IntoView {
  let state = use_context::<AppState>().expect("AppState not found");

  view! {
    <div class="settings-grid">
      <div class="settings-card settings-card-full">
        <div class="settings-card-header">
          <h3>"API Tokens"</h3>
          <span class="settings-card-description">"Manage API access tokens"</span>
        </div>
        <div class="settings-card-body">
          <div class="token-actions">
            <button class="btn btn-primary" on:click=move |_| {
              state.show_toast("Token generation coming soon", ToastLevel::Info);
            }>
              "Generate Token"
            </button>
          </div>
          <div class="empty-state tokens-empty">
            <p>"No API tokens yet"</p>
            <p class="text-muted">"Generate a token to enable authenticated API access"</p>
          </div>
        </div>
      </div>
    </div>
  }
}
