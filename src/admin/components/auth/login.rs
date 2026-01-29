//! Login page component

use crate::admin::apiclient;
use crate::admin::state::{AppState, ToastLevel};
use leptos::*;

#[component]
pub fn LoginPage(on_login: Callback<()>) -> impl IntoView {
  let state = use_context::<AppState>().expect("AppState not found");

  let (username, set_username) = create_signal(String::new());
  let (password, set_password) = create_signal(String::new());
  let (submitting, set_submitting) = create_signal(false);
  let (error, set_error) = create_signal(Option::<String>::None);

  let on_submit = move |ev: web_sys::SubmitEvent| {
    ev.prevent_default();
    set_error.set(None);

    let username_val = username.get().trim().to_string();
    let password_val = password.get();

    if username_val.is_empty() {
      set_error.set(Some("Username is required".to_string()));
      return;
    }
    if password_val.is_empty() {
      set_error.set(Some("Password is required".to_string()));
      return;
    }

    set_submitting.set(true);
    let state = state.clone();

    spawn_local(async move {
      match apiclient::login(&username_val, &password_val).await {
        Ok(_) => {
          // Update auth status
          if let Ok(status) = apiclient::fetch_auth_status().await {
            state.auth_status.set(status);
          }
          state.show_toast("Welcome back!", ToastLevel::Success);
          on_login.call(());
        }
        Err(e) => {
          let msg = if e.contains("401") || e.contains("Invalid") {
            "Invalid username or password".to_string()
          } else {
            e
          };
          set_error.set(Some(msg));
        }
      }
      set_submitting.set(false);
    });
  };

  view! {
    <div class="auth-page">
      <div class="auth-card">
        <div class="auth-header">
          <h1>"SquirrelDB"</h1>
          <p class="auth-subtitle">"Sign in to continue"</p>
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
              placeholder="Enter your username"
              autocomplete="username"
              prop:value=username
              on:input=move |ev| set_username.set(event_target_value(&ev))
              disabled=move || submitting.get()
            />
          </div>

          <div class="form-group">
            <label for="password">"Password"</label>
            <input
              type="password"
              id="password"
              class="input"
              placeholder="Enter your password"
              autocomplete="current-password"
              prop:value=password
              on:input=move |ev| set_password.set(event_target_value(&ev))
              disabled=move || submitting.get()
            />
          </div>

          <button
            type="submit"
            class="btn btn-primary btn-block"
            disabled=move || submitting.get()
          >
            {move || if submitting.get() { "Signing in..." } else { "Sign In" }}
          </button>
        </form>
      </div>
    </div>
  }
}
