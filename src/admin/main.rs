//! SquirrelDB Admin UI - Client-Side Rendered (WASM)

use leptos::*;
use squirreldb::admin::components::App;

fn main() {
  console_error_panic_hook::set_once();
  mount_to_body(|| view! { <App/> });
}
