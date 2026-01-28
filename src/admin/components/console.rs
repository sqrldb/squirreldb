//! Console component - interactive query REPL

use leptos::*;
use crate::admin::state::{AppState, ToastLevel};
use crate::admin::apiclient;
use super::Icon;

#[derive(Clone)]
struct ConsoleEntry {
  id: u32,
  query: String,
  result: String,
  is_error: bool,
}

#[component]
pub fn Console() -> impl IntoView {
  let state = use_context::<AppState>().expect("AppState not found");
  let (input, set_input) = create_signal(String::new());
  let (history, set_history) = create_signal(Vec::<ConsoleEntry>::new());
  let (running, set_running) = create_signal(false);
  let next_id = create_rw_signal(0u32);
  let trigger = create_rw_signal(0u32);

  // Execute query when trigger changes
  create_effect(move |prev: Option<u32>| {
    let current = trigger.get();
    if prev.is_some() && current > 0 {
      let query = input.get().trim().to_string();
      if query.is_empty() || running.get() {
        return current;
      }

      set_running.set(true);
      let state = state.clone();
      let query_clone = query.clone();

      spawn_local(async move {
        let id = next_id.get();
        next_id.set(id + 1);

        let (result, is_error) = match apiclient::run_query(&query_clone).await {
          Ok(val) => {
            let formatted = serde_json::to_string_pretty(&val).unwrap_or_else(|_| val.to_string());
            (formatted, false)
          }
          Err(e) => {
            state.show_toast(&format!("Query failed: {}", e), ToastLevel::Error);
            (e, true)
          }
        };

        set_history.update(|h| {
          h.push(ConsoleEntry {
            id,
            query: query_clone,
            result,
            is_error,
          });
        });

        set_input.set(String::new());
        set_running.set(false);
      });
    }
    current
  });

  view! {
    <section id="console" class="page active">
      <div class="page-header">
        <h2>"Console"</h2>
        <div class="page-header-actions">
          <button class="btn btn-secondary btn-sm" on:click=move |_| set_history.set(Vec::new())>
            <Icon name="trash-2" size=14/>
            " Clear"
          </button>
        </div>
      </div>
      <div class="console-container">
        <div class="console-output">
          <Show
            when=move || history.get().is_empty()
            fallback=move || view! {
              <For
                each=move || history.get()
                key=|e| e.id
                children=move |entry| {
                  view! {
                    <div class="console-entry">
                      <div class="console-query">
                        <span class="console-prompt">">"</span>
                        <code>{entry.query.clone()}</code>
                      </div>
                      <div class=move || if entry.is_error { "console-result error" } else { "console-result" }>
                        <pre>{entry.result.clone()}</pre>
                      </div>
                    </div>
                  }
                }
              />
            }
          >
            <div class="console-welcome">
              <pre class="ascii-logo">"  ____              _               _  ____  ____\n / ___|  __ _ _   _(_)_ __ _ __ ___| ||  _ \\| __ )\n \\___ \\ / _` | | | | | '__| '__/ _ \\ || | | |  _ \\\n  ___) | (_| | |_| | | |  | | |  __/ || |_| | |_) |\n |____/ \\__, |\\__,_|_|_|  |_|  \\___|_||____/|____/\n           |_|"</pre>
              <p class="console-help">"Type a query and press Enter to execute."</p>
              <div class="console-examples">
                <p class="text-muted">"Examples:"</p>
                <code>"db.table('users').run()"</code>
                <code>"db.table('posts').filter(doc => doc.published).run()"</code>
                <code>"db.table('users').insert({ name: 'Alice' }).run()"</code>
              </div>
            </div>
          </Show>
        </div>
        <div class="console-input-container">
          <span class="console-prompt">">"</span>
          <input
            type="text"
            class="console-input"
            placeholder="db.table('users').run()"
            prop:value=input
            on:input=move |ev| set_input.set(event_target_value(&ev))
            on:keydown=move |ev: web_sys::KeyboardEvent| {
              if ev.key() == "Enter" && !ev.shift_key() && !running.get() {
                ev.prevent_default();
                trigger.update(|t| *t += 1);
              }
            }
            disabled=running
          />
          <button
            class="btn btn-primary console-run-btn"
            disabled=move || running.get() || input.get().trim().is_empty()
            on:click=move |_| trigger.update(|t| *t += 1)
          >
            {move || if running.get() {
              view! { <Icon name="refresh-cw" size=14/> }.into_view()
            } else {
              view! { <Icon name="play" size=14/> }.into_view()
            }}
          </button>
        </div>
      </div>
    </section>
  }
}
