//! Admin UI Components

use crate::admin::apiclient;
use crate::admin::state::{AppState, Page};
use leptos::*;

mod auth;
mod buckets;
mod console;
mod dashboard;
mod explorer;
mod icons;
mod live;
mod logs;
mod modal;
mod settings;
mod sidebar;
mod tables;
mod toast;

pub use auth::{LoginPage, SetupPage, UsersSettings};
pub use buckets::Buckets;
pub use console::Console;
pub use dashboard::Dashboard;
pub use explorer::Explorer;
pub use icons::Icon;
pub use live::Live;
pub use logs::Logs;
pub use modal::ModalContainer;
pub use settings::Settings;
pub use sidebar::Sidebar;
pub use tables::Tables;
pub use toast::ToastContainer;

/// Main App component
#[component]
pub fn App() -> impl IntoView {
  // Create global state
  let state = AppState::new();
  provide_context(state.clone());

  let (auth_loading, set_auth_loading) = create_signal(true);

  // Check auth status on startup
  let state_auth = state.clone();
  create_effect(move |_| {
    let state = state_auth.clone();
    spawn_local(async move {
      match apiclient::fetch_auth_status().await {
        Ok(status) => {
          state.auth_status.set(status);
        }
        Err(_) => {
          // If auth check fails, assume we need setup
          state.auth_status.update(|s| s.needs_setup = true);
        }
      }
      set_auth_loading.set(false);
    });
  });

  // Fetch initial data when authenticated
  let state_init = state.clone();
  let auth_status = state.auth_status;
  create_effect(move |_| {
    let status = auth_status.get();
    if status.logged_in {
      let state = state_init.clone();
      spawn_local(async move {
        // Fetch S3 settings to determine if storage is enabled
        if let Ok(settings) = apiclient::fetch_storage_settings().await {
          state.storage_settings.set(settings.clone());
          state.storage_enabled.set(settings.enabled);
        }
        // Fetch tables
        if let Ok(tables) = apiclient::fetch_tables().await {
          state.tables.set(tables);
        }
        // Fetch status
        if let Ok(stats) = apiclient::fetch_status().await {
          state.stats.set(stats);
        }
      });
    }
  });

  let current_page = state.current_page;
  let auth_status = state.auth_status;

  let on_setup_complete = Callback::new(move |_| {
    // Refresh auth status after setup
    let state = state.clone();
    spawn_local(async move {
      if let Ok(status) = apiclient::fetch_auth_status().await {
        state.auth_status.set(status);
      }
    });
  });

  let on_login = on_setup_complete.clone();

  view! {
    <Show when=move || auth_loading.get()>
      <div class="auth-loading">
        <div class="loading-spinner"></div>
        " Loading..."
      </div>
    </Show>

    <Show when=move || !auth_loading.get() && auth_status.get().needs_setup>
      <SetupPage on_complete=on_setup_complete/>
    </Show>

    <Show when=move || !auth_loading.get() && !auth_status.get().needs_setup && !auth_status.get().logged_in>
      <LoginPage on_login=on_login/>
    </Show>

    <Show when=move || !auth_loading.get() && auth_status.get().logged_in>
      <div class="app-container">
        <Sidebar/>
        <main class="content">
          {move || match current_page.get() {
            Page::Dashboard => view! { <Dashboard/> }.into_view(),
            Page::Buckets => view! { <Buckets/> }.into_view(),
            Page::Tables => view! { <Tables/> }.into_view(),
            Page::Explorer => view! { <Explorer/> }.into_view(),
            Page::Console => view! { <Console/> }.into_view(),
            Page::Live => view! { <Live/> }.into_view(),
            Page::Logs => view! { <Logs/> }.into_view(),
            Page::Settings(_) => view! { <Settings/> }.into_view(),
          }}
        </main>
        <ToastContainer/>
        <ModalContainer/>
      </div>
    </Show>
  }
}
