//! Toast notification component

use leptos::*;
use gloo_timers::callback::Timeout;
use crate::admin::state::{AppState, ToastLevel};
use super::Icon;

#[component]
pub fn ToastContainer() -> impl IntoView {
  let state = use_context::<AppState>().expect("AppState not found");
  let toasts = state.toasts;

  view! {
    <div id="toast-container" class="toast-container">
      <For
        each=move || toasts.get()
        key=|t| t.id
        children=move |toast| {
          let state = use_context::<AppState>().expect("AppState not found");
          let id = toast.id;
          let level_class = toast_level_class(&toast.level);
          let icon_name = toast_level_icon(&toast.level);

          // Auto-remove toast after 5 seconds
          let state_timeout = state.clone();
          let timeout = Timeout::new(5000, move || {
            state_timeout.remove_toast(id);
          });
          timeout.forget(); // Don't cancel on drop

          view! {
            <div class=format!("toast show {}", level_class)>
              <Icon name=icon_name size=18/>
              <span class="toast-message">{toast.message.clone()}</span>
              <button class="toast-close btn-ghost" on:click=move |_| state.remove_toast(id)>
                <Icon name="x" size=16/>
              </button>
            </div>
          }
        }
      />
    </div>
  }
}

fn toast_level_class(level: &ToastLevel) -> &'static str {
  match level {
    ToastLevel::Info => "info",
    ToastLevel::Success => "success",
    ToastLevel::Warning => "warning",
    ToastLevel::Error => "error",
  }
}

fn toast_level_icon(level: &ToastLevel) -> &'static str {
  match level {
    ToastLevel::Info => "info",
    ToastLevel::Success => "check-circle",
    ToastLevel::Warning => "alert-triangle",
    ToastLevel::Error => "alert-circle",
  }
}
