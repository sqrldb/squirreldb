//! User management settings component

use crate::admin::apiclient;
use crate::admin::components::Icon;
use crate::admin::state::{AdminUserInfo, AppState, ToastLevel};
use leptos::*;

#[component]
pub fn UsersSettings() -> impl IntoView {
  let state = use_context::<AppState>().expect("AppState not found");
  let auth_status = state.auth_status;

  let (users, set_users) = create_signal(Vec::<AdminUserInfo>::new());
  let (loading, set_loading) = create_signal(true);
  let (show_create, set_show_create) = create_signal(false);

  // Check if current user is owner
  let is_owner = move || {
    auth_status
      .get()
      .user
      .as_ref()
      .map(|u| u.role == "owner")
      .unwrap_or(false)
  };

  // Load users on mount
  {
    let state = state.clone();
    create_effect(move |_| {
      let state = state.clone();
      spawn_local(async move {
        match apiclient::fetch_admin_users().await {
          Ok(list) => set_users.set(list),
          Err(e) => {
            if !e.contains("403") {
              state.show_toast(&format!("Failed to load users: {}", e), ToastLevel::Error);
            }
          }
        }
        set_loading.set(false);
      });
    });
  }

  view! {
    <div class="settings-grid">
      <div class="settings-card settings-card-wide">
        <div class="settings-card-header">
          <h3>"Admin Users"</h3>
          <span class="settings-card-description">"Manage who can access the admin panel"</span>
        </div>
        <div class="settings-card-body">
          <Show when=move || loading.get()>
            <div class="loading-spinner"></div>
            " Loading..."
          </Show>

          <Show when=move || !loading.get() && !is_owner()>
            <p class="text-muted">"Only owners can manage admin users."</p>
          </Show>

          <Show when=move || !loading.get() && is_owner()>
            // Create User Button
            <Show when=move || !show_create.get()>
              <button class="btn btn-primary btn-sm" on:click=move |_| set_show_create.set(true)>
                <Icon name="plus" size=14/>
                " Add User"
              </button>
            </Show>

            // Create User Form
            <Show when=move || show_create.get()>
              <CreateUserForm
                set_show_create=set_show_create
                set_users=set_users
              />
            </Show>

            // Users Table
            <Show when=move || !users.get().is_empty()>
              <table class="data-table" style="margin-top: 16px">
                <thead>
                  <tr>
                    <th>"Username"</th>
                    <th>"Email"</th>
                    <th>"Role"</th>
                    <th>"Created"</th>
                    <th>"Actions"</th>
                  </tr>
                </thead>
                <tbody>
                  <For
                    each=move || users.get()
                    key=|u| u.id.clone()
                    children=move |user| {
                      let user_id = user.id.clone();
                      let is_self = auth_status
                        .get()
                        .user
                        .as_ref()
                        .map(|u| u.id == user_id)
                        .unwrap_or(false);
                      view! {
                        <tr>
                          <td>
                            <strong>{user.username.clone()}</strong>
                            {if is_self { " (you)" } else { "" }}
                          </td>
                          <td>{user.email.clone().unwrap_or_else(|| "-".to_string())}</td>
                          <td>
                            <span class=format!("role-badge role-{}", user.role)>
                              {user.role.clone()}
                            </span>
                          </td>
                          <td>{format_date(&user.created_at)}</td>
                          <td>
                            <Show when=move || !is_self>
                              <UserActions
                                user_id=user_id.clone()
                                current_role=user.role.clone()
                                set_users=set_users
                              />
                            </Show>
                          </td>
                        </tr>
                      }
                    }
                  />
                </tbody>
              </table>
            </Show>

            <Show when=move || users.get().is_empty() && !loading.get()>
              <p class="text-muted" style="margin-top: 12px">"No other admin users"</p>
            </Show>
          </Show>
        </div>
      </div>
    </div>
  }
}

#[component]
fn CreateUserForm(
  set_show_create: WriteSignal<bool>,
  set_users: WriteSignal<Vec<AdminUserInfo>>,
) -> impl IntoView {
  let state = use_context::<AppState>().expect("AppState not found");

  let (username, set_username) = create_signal(String::new());
  let (email, set_email) = create_signal(String::new());
  let (password, set_password) = create_signal(String::new());
  let (role, set_role) = create_signal("admin".to_string());
  let (creating, set_creating) = create_signal(false);

  let on_create = move |_| {
    let username_val = username.get().trim().to_string();
    let password_val = password.get();

    if username_val.is_empty() {
      state.show_toast("Username is required", ToastLevel::Warning);
      return;
    }
    if password_val.len() < 8 {
      state.show_toast(
        "Password must be at least 8 characters",
        ToastLevel::Warning,
      );
      return;
    }

    set_creating.set(true);
    let email_val = email.get().trim().to_string();
    let email_opt = if email_val.is_empty() {
      None
    } else {
      Some(email_val)
    };
    let role_val = role.get();
    let state = state.clone();

    spawn_local(async move {
      match apiclient::create_admin_user(
        &username_val,
        email_opt.as_deref(),
        &password_val,
        &role_val,
      )
      .await
      {
        Ok(_) => {
          state.show_toast(
            &format!("User '{}' created", username_val),
            ToastLevel::Success,
          );
          set_show_create.set(false);
          set_username.set(String::new());
          set_email.set(String::new());
          set_password.set(String::new());
          // Refresh user list
          if let Ok(list) = apiclient::fetch_admin_users().await {
            set_users.set(list);
          }
        }
        Err(e) => {
          state.show_toast(&format!("Failed to create user: {}", e), ToastLevel::Error);
        }
      }
      set_creating.set(false);
    });
  };

  view! {
    <div class="create-user-form">
      <div class="form-row">
        <div class="form-group">
          <label>"Username"</label>
          <input
            type="text"
            class="input"
            placeholder="newadmin"
            prop:value=username
            on:input=move |ev| set_username.set(event_target_value(&ev))
          />
        </div>
        <div class="form-group">
          <label>"Email " <span class="text-muted">"(optional)"</span></label>
          <input
            type="email"
            class="input"
            placeholder="user@example.com"
            prop:value=email
            on:input=move |ev| set_email.set(event_target_value(&ev))
          />
        </div>
      </div>
      <div class="form-row">
        <div class="form-group">
          <label>"Password"</label>
          <input
            type="password"
            class="input"
            placeholder="At least 8 characters"
            prop:value=password
            on:input=move |ev| set_password.set(event_target_value(&ev))
          />
        </div>
        <div class="form-group">
          <label>"Role"</label>
          <select
            class="input"
            prop:value=role
            on:change=move |ev| set_role.set(event_target_value(&ev))
          >
            <option value="admin">"Admin"</option>
            <option value="owner">"Owner"</option>
          </select>
        </div>
      </div>
      <div class="form-actions">
        <button
          class="btn btn-primary"
          disabled=move || creating.get()
          on:click=on_create
        >
          {move || if creating.get() { "Creating..." } else { "Create User" }}
        </button>
        <button class="btn btn-secondary" on:click=move |_| set_show_create.set(false)>
          "Cancel"
        </button>
      </div>
    </div>
  }
}

#[component]
fn UserActions(
  user_id: String,
  current_role: String,
  set_users: WriteSignal<Vec<AdminUserInfo>>,
) -> impl IntoView {
  let state = use_context::<AppState>().expect("AppState not found");
  let (deleting, set_deleting) = create_signal(false);
  let (changing_role, set_changing_role) = create_signal(false);

  let user_id_delete = user_id.clone();
  let user_id_role = user_id.clone();

  let new_role = if current_role == "owner" {
    "admin"
  } else {
    "owner"
  };
  let new_role_label = new_role.to_string();
  let new_role_display = new_role_label.clone();

  let state_delete = state.clone();
  let on_delete = move |_| {
    let user_id = user_id_delete.clone();
    let state = state_delete.clone();
    set_deleting.set(true);

    spawn_local(async move {
      match apiclient::delete_admin_user(&user_id).await {
        Ok(_) => {
          state.show_toast("User deleted", ToastLevel::Success);
          if let Ok(list) = apiclient::fetch_admin_users().await {
            set_users.set(list);
          }
        }
        Err(e) => {
          state.show_toast(&format!("Failed to delete: {}", e), ToastLevel::Error);
        }
      }
      set_deleting.set(false);
    });
  };

  let on_change_role = move |_| {
    let user_id = user_id_role.clone();
    let state = state.clone();
    set_changing_role.set(true);

    spawn_local(async move {
      match apiclient::update_admin_user_role(&user_id, new_role).await {
        Ok(_) => {
          state.show_toast("Role updated", ToastLevel::Success);
          if let Ok(list) = apiclient::fetch_admin_users().await {
            set_users.set(list);
          }
        }
        Err(e) => {
          state.show_toast(&format!("Failed to update role: {}", e), ToastLevel::Error);
        }
      }
      set_changing_role.set(false);
    });
  };

  view! {
    <div class="user-actions">
      <button
        class="btn btn-ghost btn-sm"
        title=format!("Make {}", new_role_label)
        disabled=move || changing_role.get()
        on:click=on_change_role
      >
        {move || if changing_role.get() { "...".to_string() } else { new_role_display.clone() }}
      </button>
      <button
        class="btn btn-ghost btn-sm text-danger"
        title="Delete user"
        disabled=move || deleting.get()
        on:click=on_delete
      >
        <Icon name="trash-2" size=14/>
      </button>
    </div>
  }
}

fn format_date(date_str: &str) -> String {
  if let Some(date_part) = date_str.split('T').next() {
    date_part.to_string()
  } else {
    date_str.to_string()
  }
}
