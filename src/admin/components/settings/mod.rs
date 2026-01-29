//! Settings page components

use super::UsersSettings;
use crate::admin::state::{AppState, Page, SettingsTab};
use leptos::*;

mod general;
mod storage;
mod tokens;

pub use general::GeneralSettings;
pub use storage::StorageSettings;
pub use tokens::TokensSettings;

#[component]
pub fn Settings() -> impl IntoView {
  let state = use_context::<AppState>().expect("AppState not found");
  let current_page = state.current_page;
  let auth_status = state.auth_status;

  let current_tab = move || {
    if let Page::Settings(tab) = current_page.get() {
      tab
    } else {
      SettingsTab::General
    }
  };

  let is_owner = move || {
    auth_status
      .get()
      .user
      .as_ref()
      .map(|u| u.role == "owner")
      .unwrap_or(false)
  };

  view! {
    <section id="settings" class="page active">
      <div class="page-header">
        <h2>"Settings"</h2>
      </div>
      <div class="settings-tabs">
        <TabButton tab=SettingsTab::General label="General" current_page=current_page/>
        <TabButton tab=SettingsTab::Tokens label="API Tokens" current_page=current_page/>
        <TabButton tab=SettingsTab::Storage label="Storage" current_page=current_page/>
        <Show when=move || is_owner()>
          <TabButton tab=SettingsTab::Users label="Users" current_page=current_page/>
        </Show>
      </div>
      {move || match current_tab() {
        SettingsTab::General => view! { <GeneralSettings/> }.into_view(),
        SettingsTab::Tokens => view! { <TokensSettings/> }.into_view(),
        SettingsTab::Storage => view! { <StorageSettings/> }.into_view(),
        SettingsTab::Users => view! { <UsersSettings/> }.into_view(),
      }}
    </section>
  }
}

#[component]
fn TabButton(tab: SettingsTab, label: &'static str, current_page: RwSignal<Page>) -> impl IntoView {
  let is_active = move || {
    if let Page::Settings(current_tab) = current_page.get() {
      current_tab == tab
    } else {
      false
    }
  };

  view! {
    <button
      class="settings-tab"
      class:active=is_active
      on:click=move |_| current_page.set(Page::Settings(tab))
    >
      {label}
    </button>
  }
}
