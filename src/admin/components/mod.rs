//! Admin UI Components

use leptos::*;
use crate::admin::state::{AppState, Page};
use crate::admin::apiclient;

mod icons;
mod sidebar;
mod dashboard;
mod buckets;
mod tables;
mod explorer;
mod console;
mod live;
mod logs;
mod settings;
mod toast;
mod modal;

pub use icons::Icon;
pub use sidebar::Sidebar;
pub use dashboard::Dashboard;
pub use buckets::Buckets;
pub use tables::Tables;
pub use explorer::Explorer;
pub use console::Console;
pub use live::Live;
pub use logs::Logs;
pub use settings::Settings;
pub use toast::ToastContainer;
pub use modal::ModalContainer;

/// Main App component
#[component]
pub fn App() -> impl IntoView {
  // Create global state
  let state = AppState::new();
  provide_context(state.clone());

  // Fetch initial data on startup
  let state_init = state.clone();
  create_effect(move |_| {
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
  });

  let current_page = state.current_page;

  view! {
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
  }
}
