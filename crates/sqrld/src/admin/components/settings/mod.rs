//! Settings page components

use super::UsersSettings;
use crate::admin::state::AppState;
use leptos::*;
use leptos_router::*;

mod caching;
mod general;
mod storage;
mod tokens;

pub use caching::CachingSettings;
pub use general::GeneralSettings;
pub use storage::StorageSettings;
pub use tokens::TokensSettings;

#[component]
pub fn Settings() -> impl IntoView {
  let state = use_context::<AppState>().expect("AppState not found");
  let auth_status = state.auth_status;
  let params = use_params_map();

  let current_tab = move || {
    params.with(|p| {
      p.get("tab")
        .cloned()
        .unwrap_or_else(|| "general".to_string())
    })
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
        <TabLink tab="general" label="General" current_tab=current_tab/>
        <TabLink tab="api" label="API Access" current_tab=current_tab/>
        <TabLink tab="storage" label="Storage" current_tab=current_tab/>
        <TabLink tab="caching" label="Caching" current_tab=current_tab/>
        <Show when=move || is_owner()>
          <TabLink tab="users" label="Users" current_tab=current_tab/>
        </Show>
      </div>
      {move || match current_tab().as_str() {
        "general" => view! { <GeneralSettings/> }.into_view(),
        "api" => view! { <TokensSettings/> }.into_view(),
        "storage" => view! { <StorageSettings/> }.into_view(),
        "caching" => view! { <CachingSettings/> }.into_view(),
        "users" => view! { <UsersSettings/> }.into_view(),
        _ => view! { <GeneralSettings/> }.into_view(),
      }}
    </section>
  }
}

#[component]
fn TabLink<F>(tab: &'static str, label: &'static str, current_tab: F) -> impl IntoView
where
  F: Fn() -> String + 'static + Copy,
{
  let href = format!("/settings/{}", tab);

  view! {
    <A
      href=href
      class=move || format!("settings-tab{}", if current_tab() == tab { " active" } else { "" })
    >
      {label}
    </A>
  }
}
