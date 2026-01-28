//! Modal component (container for dynamic modals)

use leptos::*;

#[component]
pub fn ModalContainer() -> impl IntoView {
  // The modal container is a placeholder - individual components
  // manage their own modals inline now with Show/when
  view! {
    <div id="modal-container"></div>
  }
}
