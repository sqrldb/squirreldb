//! Sidebar navigation component

use super::{Icon, Modal};
use crate::admin::apiclient;
use crate::admin::state::{AppState, AuthStatus, ProjectInfo, Theme, ToastLevel};
use leptos::*;
use leptos_router::*;
use wasm_bindgen::JsCast;

#[component]
pub fn Sidebar() -> impl IntoView {
  let state = use_context::<AppState>().expect("AppState not found");
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
      <ProjectSelector/>
      <div class="server-status">
        <span class="status-indicator"></span>
        "Connected"
      </div>
      <div class="nav-section">
        <div class="nav-section-title">"Main"</div>
        <ul class="nav-links">
          <li><NavLink href="/dashboard" label="Dashboard" icon="layout-dashboard"/></li>
          <li><NavLink href="/tables" label="Tables" icon="table"/></li>
          <Show when=move || storage_enabled.get()>
            <li><NavLink href="/buckets" label="Buckets" icon="bucket"/></li>
          </Show>
          <li><NavLink href="/explorer" label="Explorer" icon="search"/></li>
          <li><NavLink href="/console" label="Console" icon="terminal"/></li>
        </ul>
      </div>
      <div class="nav-section">
        <div class="nav-section-title">"Realtime"</div>
        <ul class="nav-links">
          <li><NavLink href="/live" label="Live" icon="zap"/></li>
          <li><NavLink href="/logs" label="Logs" icon="scroll-text"/></li>
        </ul>
      </div>
      <div class="nav-section">
        <div class="nav-section-title">"System"</div>
        <ul class="nav-links">
          <li><NavLink href="/projects" label="Projects" icon="folder"/></li>
          <li><NavLink href="/settings" label="Settings" icon="settings"/></li>
        </ul>
      </div>
      <div class="sidebar-footer">
        <Show when=move || auth_status.get().user.is_some()>
          <UserMenu/>
        </Show>
        <div class="sidebar-footer-info">"SquirrelDB v0.2"</div>
      </div>
    </nav>
  }
}

#[component]
fn ProjectSelector() -> impl IntoView {
  let state = use_context::<AppState>().expect("AppState not found");
  let projects = state.projects;
  let current_project = state.current_project;

  // Fetch projects on mount
  create_effect(move |_| {
    spawn_local(async move {
      if let Ok(fetched) = apiclient::fetch_projects().await {
        state.projects.set(fetched.clone());
        // If no project selected, select the first one (or default)
        if state.current_project.get().is_none() && !fetched.is_empty() {
          state.current_project.set(Some(fetched[0].id.clone()));
        }
      }
    });
  });

  let on_change = move |ev: web_sys::Event| {
    let target = ev.target().unwrap();
    let select: web_sys::HtmlSelectElement = target.dyn_into().unwrap();
    let value = select.value();
    current_project.set(Some(value));
  };

  view! {
    <div class="project-selector">
      <select on:change=on_change>
        <For
          each=move || projects.get()
          key=|p| p.id.clone()
          children=move |project: ProjectInfo| {
            let project_id = project.id.clone();
            let is_selected = move || current_project.get() == Some(project_id.clone());
            view! {
              <option value=project.id.clone() selected=is_selected>
                {project.name}
              </option>
            }
          }
        />
      </select>
      <A href="/projects" class="btn-icon" attr:title="Manage Projects">
        <Icon name="settings" size=14/>
      </A>
    </div>
  }
}

#[component]
fn NavLink(
  href: &'static str,
  label: &'static str,
  icon: &'static str,
) -> impl IntoView {
  view! {
    <A href=href class="nav-link" active_class="active">
      <Icon name=icon size=18/>
      <span>{label}</span>
    </A>
  }
}

#[component]
fn UserMenu() -> impl IntoView {
  let state = use_context::<AppState>().expect("AppState not found");
  let auth_status = state.auth_status;
  let (menu_open, set_menu_open) = create_signal(false);
  let (show_password_modal, set_show_password_modal) = create_signal(false);
  let (logging_out, set_logging_out) = create_signal(false);

  // Close menu when clicking outside
  let menu_ref = create_node_ref::<html::Div>();

  let on_logout = {
    let state = state.clone();
    move |_| {
      set_menu_open.set(false);
      set_logging_out.set(true);
      let state = state.clone();
      spawn_local(async move {
        match apiclient::logout().await {
          Ok(_) => {
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
    }
  };

  view! {
    <div class="sidebar-user" node_ref=menu_ref>
      <button
        class="sidebar-user-btn"
        on:click=move |_| set_menu_open.update(|v| *v = !*v)
        disabled=move || logging_out.get()
      >
        <div class="sidebar-user-info">
          <span class="sidebar-username">
            {move || auth_status.get().user.as_ref().map(|u| u.username.clone()).unwrap_or_default()}
          </span>
          <span class="sidebar-role">
            {move || auth_status.get().user.as_ref().map(|u| u.role.clone()).unwrap_or_default()}
          </span>
        </div>
        <Icon name="chevron-up" size=16/>
      </button>

      <Show when=move || menu_open.get()>
        <div class="user-menu">
          <button
            class="user-menu-item"
            on:click=move |_| {
              set_menu_open.set(false);
              set_show_password_modal.set(true);
            }
          >
            <Icon name="key" size=16/>
            <span>"Change password"</span>
          </button>
          <button
            class="user-menu-item"
            on:click=on_logout.clone()
          >
            <Icon name="log-out" size=16/>
            <span>"Sign out"</span>
          </button>
        </div>
      </Show>
    </div>

    <ChangePasswordModal
      show=show_password_modal
      on_close=move || set_show_password_modal.set(false)
    />
  }
}

#[component]
fn ChangePasswordModal(
  show: ReadSignal<bool>,
  on_close: impl Fn() + 'static + Clone,
) -> impl IntoView {
  let state = use_context::<AppState>().expect("AppState not found");
  let (current_password, set_current_password) = create_signal(String::new());
  let (new_password, set_new_password) = create_signal(String::new());
  let (confirm_password, set_confirm_password) = create_signal(String::new());
  let (saving, set_saving) = create_signal(false);
  let (error, set_error) = create_signal(Option::<String>::None);

  let on_close_stored = store_value(on_close.clone());
  let state_stored = store_value(state.clone());

  // Reset form when modal opens
  create_effect(move |_| {
    if show.get() {
      set_current_password.set(String::new());
      set_new_password.set(String::new());
      set_confirm_password.set(String::new());
      set_error.set(None);
    }
  });

  view! {
    <Modal show=show on_close=on_close.clone() title="Change Password">
      <div class="modal-form">
        <Show when=move || error.get().is_some()>
          <div class="alert alert-error">
            {move || error.get().unwrap_or_default()}
          </div>
        </Show>

        <div class="form-group">
          <label>"Current Password"</label>
          <input
            type="password"
            class="input"
            prop:value=current_password
            on:input=move |ev| set_current_password.set(event_target_value(&ev))
            disabled=move || saving.get()
          />
        </div>

        <div class="form-group">
          <label>"New Password"</label>
          <input
            type="password"
            class="input"
            prop:value=new_password
            on:input=move |ev| set_new_password.set(event_target_value(&ev))
            disabled=move || saving.get()
          />
          <p class="form-hint">"Minimum 8 characters"</p>
        </div>

        <div class="form-group">
          <label>"Confirm New Password"</label>
          <input
            type="password"
            class="input"
            prop:value=confirm_password
            on:input=move |ev| set_confirm_password.set(event_target_value(&ev))
            disabled=move || saving.get()
          />
        </div>

        <div class="modal-actions">
          <button
            class="btn btn-ghost"
            on:click=move |_| (on_close_stored.get_value())()
            disabled=move || saving.get()
          >
            "Cancel"
          </button>
          <button
            class="btn btn-primary"
            on:click=move |_| {
              set_error.set(None);

              // Validate
              if new_password.get().len() < 8 {
                set_error.set(Some("New password must be at least 8 characters".to_string()));
                return;
              }
              if new_password.get() != confirm_password.get() {
                set_error.set(Some("Passwords do not match".to_string()));
                return;
              }

              set_saving.set(true);
              let state = state_stored.get_value();
              let on_close = on_close_stored.get_value();
              let current = current_password.get();
              let new_pwd = new_password.get();
              spawn_local(async move {
                match apiclient::change_password(&current, &new_pwd).await {
                  Ok(_) => {
                    state.show_toast("Password changed successfully", ToastLevel::Success);
                    on_close();
                  }
                  Err(e) => {
                    set_error.set(Some(e));
                  }
                }
                set_saving.set(false);
              });
            }
            disabled=move || saving.get() || current_password.get().is_empty() || new_password.get().is_empty() || confirm_password.get().is_empty()
          >
            {move || if saving.get() { "Changing..." } else { "Change Password" }}
          </button>
        </div>
      </div>
    </Modal>
  }
}
