//! General settings tab

use leptos::*;

#[component]
pub fn GeneralSettings() -> impl IntoView {
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
                <input type="checkbox" checked disabled />
                <span class="toggle-slider"></span>
              </label>
            </div>
            <div class="setting-row">
              <div class="setting-info">
                <span class="setting-label">"WebSocket"</span>
                <span class="setting-description">"Real-time subscriptions and queries"</span>
              </div>
              <label class="toggle">
                <input type="checkbox" checked disabled />
                <span class="toggle-slider"></span>
              </label>
            </div>
            <div class="setting-row">
              <div class="setting-info">
                <span class="setting-label">"Server-Sent Events"</span>
                <span class="setting-description">"Coming soon"</span>
              </div>
              <label class="toggle">
                <input type="checkbox" disabled />
                <span class="toggle-slider"></span>
              </label>
            </div>
          </div>
          <div class="settings-card-footer">
            <span class="text-muted">"Protocol changes require server restart"</span>
          </div>
        </div>

        // Authentication Card
        <div class="settings-card">
          <div class="settings-card-header">
            <h3>"Authentication"</h3>
            <span class="settings-card-description">"Configure API authentication"</span>
          </div>
          <div class="settings-card-body">
            <div class="setting-row">
              <div class="setting-info">
                <span class="setting-label">"Enable Authentication"</span>
                <span class="setting-description">"Require API tokens for access"</span>
              </div>
              <label class="toggle">
                <input type="checkbox" disabled />
                <span class="toggle-slider"></span>
              </label>
            </div>
            <div class="setting-warning">
              <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
                <path d="M8.982 1.566a1.13 1.13 0 00-1.96 0L.165 13.233c-.457.778.091 1.767.98 1.767h13.713c.889 0 1.438-.99.98-1.767L8.982 1.566zM8 5c.535 0 .954.462.9.995l-.35 3.507a.552.552 0 01-1.1 0L7.1 5.995A.905.905 0 018 5zm.002 6a1 1 0 110 2 1 1 0 010-2z"/>
              </svg>
              <span>"Authentication is disabled. API is publicly accessible."</span>
            </div>
          </div>
          <div class="settings-card-footer">
            <span class="text-muted">"Auth changes require server restart"</span>
          </div>
        </div>
    </div>
  }
}
