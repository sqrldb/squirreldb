//! Modal components

use super::Icon;
use leptos::*;

/// Reusable Modal component
#[component]
pub fn Modal(
  show: ReadSignal<bool>,
  on_close: impl Fn() + 'static + Clone,
  title: &'static str,
  children: ChildrenFn,
) -> impl IntoView {
  let on_close_stored = store_value(on_close);

  view! {
    <Show when=move || show.get()>
      <div
        class="modal-overlay active"
        on:click=move |_| (on_close_stored.get_value())()
      >
        <div
          class="modal"
          on:click=|e| e.stop_propagation()
        >
          <div class="modal-header">
            <h3>{title}</h3>
            <button
              class="modal-close"
              on:click=move |_| (on_close_stored.get_value())()
            >
              <Icon name="x" size=18/>
            </button>
          </div>
          <div class="modal-body">
            {children()}
          </div>
        </div>
      </div>
    </Show>
  }
}

/// Modal container (placeholder for portal-style modals)
#[component]
pub fn ModalContainer() -> impl IntoView {
  view! {
    <div id="modal-container"></div>
  }
}
