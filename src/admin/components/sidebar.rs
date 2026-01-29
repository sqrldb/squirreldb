//! Sidebar navigation component

use super::Icon;
use crate::admin::apiclient;
use crate::admin::state::{AppState, AuthStatus, Page, SettingsTab, Theme, ToastLevel};
use leptos::*;

#[component]
pub fn Sidebar() -> impl IntoView {
  let state = use_context::<AppState>().expect("AppState not found");
  let current_page = state.current_page;
  let theme = state.theme;
  let storage_enabled = state.storage_enabled;
  let auth_status = state.auth_status;

  // Apply theme on change
  create_effect(move |_| {
    let document = web_sys::window().unwrap().document().unwrap();
    let html = document.document_element().unwrap();
    let theme_value = match theme.get() {
      Theme::Light => "light",
      Theme::Dark => "dark",
      Theme::System => "system",
    };
    html.set_attribute("data-theme", theme_value).unwrap();
  });

  view! {
    <nav class="sidebar">
      <div class="logo">
        <h1>"SquirrelDB"</h1>
        <div class="theme-toggle">
          <button
            class="theme-btn"
            class:active=move || theme.get() == Theme::Light
            title="Light mode"
            on:click=move |_| theme.set(Theme::Light)
          >
            <Icon name="sun" size=14/>
          </button>
          <button
            class="theme-btn"
            class:active=move || theme.get() == Theme::System
            title="System preference"
            on:click=move |_| theme.set(Theme::System)
          >
            <Icon name="monitor" size=14/>
          </button>
          <button
            class="theme-btn"
            class:active=move || theme.get() == Theme::Dark
            title="Dark mode"
            on:click=move |_| theme.set(Theme::Dark)
          >
            <Icon name="moon" size=14/>
          </button>
        </div>
      </div>
      <div class="server-status">
        <span class="status-indicator"></span>
        "Connected"
      </div>
      <div class="nav-section">
        <div class="nav-section-title">"Main"</div>
        <ul class="nav-links">
          <li><NavItem page=Page::Dashboard label="Dashboard" icon="layout-dashboard" current_page=current_page/></li>
          <li><NavItem page=Page::Tables label="Tables" icon="table" current_page=current_page/></li>
          <Show when=move || storage_enabled.get()>
            <li><NavItem page=Page::Buckets label="Buckets" icon="bucket" current_page=current_page/></li>
          </Show>
          <li><NavItem page=Page::Explorer label="Explorer" icon="search" current_page=current_page/></li>
          <li><NavItem page=Page::Console label="Console" icon="terminal" current_page=current_page/></li>
        </ul>
      </div>
      <div class="nav-section">
        <div class="nav-section-title">"Realtime"</div>
        <ul class="nav-links">
          <li><NavItem page=Page::Live label="Live" icon="zap" current_page=current_page/></li>
          <li><NavItem page=Page::Logs label="Logs" icon="scroll-text" current_page=current_page/></li>
        </ul>
      </div>
      <div class="nav-section">
        <div class="nav-section-title">"System"</div>
        <ul class="nav-links">
          <li><NavItem page=Page::Settings(SettingsTab::General) label="Settings" icon="settings" current_page=current_page/></li>
        </ul>
      </div>
      <div class="sidebar-footer">
        <Show when=move || auth_status.get().user.is_some()>
          <div class="sidebar-user">
            <div class="sidebar-user-info">
              <span class="sidebar-username">
                {move || auth_status.get().user.as_ref().map(|u| u.username.clone()).unwrap_or_default()}
              </span>
              <span class="sidebar-role">
                {move || auth_status.get().user.as_ref().map(|u| u.role.clone()).unwrap_or_default()}
              </span>
            </div>
            <LogoutButton/>
          </div>
        </Show>
        <div class="sidebar-footer-info">"SquirrelDB v0.2"</div>
      </div>
    </nav>
  }
}

#[component]
fn NavItem(
  page: Page,
  label: &'static str,
  icon: &'static str,
  current_page: RwSignal<Page>,
) -> impl IntoView {
  let is_active = move || {
    let current = current_page.get();
    match (current, page) {
      (Page::Settings(_), Page::Settings(_)) => true,
      _ => std::mem::discriminant(&current) == std::mem::discriminant(&page),
    }
  };

  view! {
    <a
      href="#"
      class="nav-link"
      class:active=is_active
      on:click=move |e| {
        e.prevent_default();
        current_page.set(page);
      }
    >
      <Icon name=icon size=18/>
      <span>{label}</span>
    </a>
  }
}

#[component]
fn LogoutButton() -> impl IntoView {
  let state = use_context::<AppState>().expect("AppState not found");
  let (logging_out, set_logging_out) = create_signal(false);

  let on_logout = move |_| {
    let state = state.clone();
    set_logging_out.set(true);
    spawn_local(async move {
      match apiclient::logout().await {
        Ok(_) => {
          // Reset auth state
          state.auth_status.set(AuthStatus {
            needs_setup: false,
            logged_in: false,
            user: None,
          });
        }
        Err(e) => {
          state.show_toast(&format!("Logout failed: {}", e), ToastLevel::Error);
        }
      }
      set_logging_out.set(false);
    });
  };

  view! {
    <button
      class="btn btn-ghost btn-sm logout-btn"
      title="Sign out"
      disabled=move || logging_out.get()
      on:click=on_logout
    >
      <Icon name="log-out" size=16/>
    </button>
  }
}
