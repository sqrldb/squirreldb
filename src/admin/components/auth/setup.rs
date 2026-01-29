//! Setup page for initial admin user creation

use crate::admin::apiclient;
use crate::admin::state::{AppState, ToastLevel};
use leptos::*;

#[component]
pub fn SetupPage(on_complete: Callback<()>) -> impl IntoView {
  let state = use_context::<AppState>().expect("AppState not found");

  let (username, set_username) = create_signal(String::new());
  let (email, set_email) = create_signal(String::new());
  let (password, set_password) = create_signal(String::new());
  let (confirm_password, set_confirm_password) = create_signal(String::new());
  let (submitting, set_submitting) = create_signal(false);
  let (error, set_error) = create_signal(Option::<String>::None);

  let on_submit = move |ev: web_sys::SubmitEvent| {
    ev.prevent_default();
    set_error.set(None);

    let username_val = username.get().trim().to_string();
    let email_val = email.get().trim().to_string();
    let password_val = password.get();
    let confirm_val = confirm_password.get();

    // Validation
    if username_val.is_empty() {
      set_error.set(Some("Username is required".to_string()));
      return;
    }
    if username_val.len() < 3 {
      set_error.set(Some("Username must be at least 3 characters".to_string()));
      return;
    }
    if password_val.len() < 8 {
      set_error.set(Some("Password must be at least 8 characters".to_string()));
      return;
    }
    if password_val != confirm_val {
      set_error.set(Some("Passwords do not match".to_string()));
      return;
    }

    set_submitting.set(true);
    let state = state.clone();
    let email_opt = if email_val.is_empty() {
      None
    } else {
      Some(email_val)
    };

    spawn_local(async move {
      match apiclient::setup_admin(&username_val, email_opt.as_deref(), &password_val).await {
        Ok(_) => {
          // Update auth status
          if let Ok(status) = apiclient::fetch_auth_status().await {
            state.auth_status.set(status);
          }
          state.show_toast("Welcome to SquirrelDB!", ToastLevel::Success);
          on_complete.call(());
        }
        Err(e) => {
          set_error.set(Some(e));
        }
      }
      set_submitting.set(false);
    });
  };

  view! {
    <div class="auth-page">
      <div class="auth-card">
        <div class="auth-header">
          <h1>"Welcome to SquirrelDB"</h1>
          <p class="auth-subtitle">"Create your admin account to get started"</p>
        </div>

        <form class="auth-form" on:submit=on_submit>
          <Show when=move || error.get().is_some()>
            <div class="auth-error">
              {move || error.get().unwrap_or_default()}
            </div>
          </Show>

          <div class="form-group">
            <label for="username">"Username"</label>
            <input
              type="text"
              id="username"
              class="input"
              placeholder="admin"
              autocomplete="username"
              prop:value=username
              on:input=move |ev| set_username.set(event_target_value(&ev))
              disabled=move || submitting.get()
            />
          </div>

          <div class="form-group">
            <label for="email">"Email " <span class="text-muted">"(optional)"</span></label>
            <input
              type="email"
              id="email"
              class="input"
              placeholder="admin@example.com"
              autocomplete="email"
              prop:value=email
              on:input=move |ev| set_email.set(event_target_value(&ev))
              disabled=move || submitting.get()
            />
          </div>

          <div class="form-group">
            <label for="password">"Password"</label>
            <input
              type="password"
              id="password"
              class="input"
              placeholder="At least 8 characters"
              autocomplete="new-password"
              prop:value=password
              on:input=move |ev| set_password.set(event_target_value(&ev))
              disabled=move || submitting.get()
            />
          </div>

          <div class="form-group">
            <label for="confirm-password">"Confirm Password"</label>
            <input
              type="password"
              id="confirm-password"
              class="input"
              placeholder="Confirm your password"
              autocomplete="new-password"
              prop:value=confirm_password
              on:input=move |ev| set_confirm_password.set(event_target_value(&ev))
              disabled=move || submitting.get()
            />
          </div>

          <button
            type="submit"
            class="btn btn-primary btn-block"
            disabled=move || submitting.get()
          >
            {move || if submitting.get() { "Creating account..." } else { "Create Account" }}
          </button>
        </form>

        <div class="auth-footer">
          <p class="text-muted text-sm">
            "This account will have owner privileges and can manage other admins."
          </p>
        </div>
      </div>
    </div>
  }
}
