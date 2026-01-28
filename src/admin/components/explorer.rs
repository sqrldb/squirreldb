//! Explorer component - query builder with results panel

use leptos::*;
use crate::admin::state::{AppState, ToastLevel};
use crate::admin::apiclient;
use super::Icon;

#[component]
pub fn Explorer() -> impl IntoView {
  let state = use_context::<AppState>().expect("AppState not found");
  let tables = state.tables;

  let (query, set_query) = create_signal(String::new());
  let (results, set_results) = create_signal::<Option<String>>(None);
  let (running, set_running) = create_signal(false);
  let (result_count, set_result_count) = create_signal::<Option<usize>>(None);

  let run_query = move |_| {
    let q = query.get().trim().to_string();
    if q.is_empty() {
      state.show_toast("Please enter a query", ToastLevel::Warning);
      return;
    }

    set_running.set(true);
    set_results.set(None);
    set_result_count.set(None);
    let state = state.clone();

    spawn_local(async move {
      match apiclient::run_query(&q).await {
        Ok(val) => {
          // Count results if it's an array
          let count = val.as_array().map(|arr| arr.len());
          set_result_count.set(count);

          let formatted = serde_json::to_string_pretty(&val)
            .unwrap_or_else(|_| val.to_string());
          set_results.set(Some(formatted));
        }
        Err(e) => {
          state.show_toast(&format!("Query failed: {}", e), ToastLevel::Error);
          set_results.set(Some(format!("Error: {}", e)));
        }
      }
      set_running.set(false);
    });
  };

  let insert_table_query = move |name: String| {
    set_query.set(format!("db.table('{}').run()", name));
  };

  view! {
    <section id="explorer" class="page active">
      <div class="page-header">
        <h2>"Explorer"</h2>
      </div>
      <div class="explorer-layout">
        <div class="explorer-sidebar">
          <div class="explorer-sidebar-header">
            <Icon name="database" size=16/>
            <span>"Tables"</span>
          </div>
          <ul class="explorer-table-list">
            <For
              each=move || tables.get()
              key=|t| t.name.clone()
              children=move |table| {
                let name = table.name.clone();
                let name_click = table.name.clone();
                view! {
                  <li>
                    <button
                      class="explorer-table-item"
                      on:click=move |_| insert_table_query(name_click.clone())
                    >
                      <Icon name="table" size=14/>
                      <span>{name}</span>
                      <span class="badge">{table.count}</span>
                    </button>
                  </li>
                }
              }
            />
          </ul>
          <Show when=move || tables.get().is_empty()>
            <div class="explorer-empty">
              <p class="text-muted">"No tables yet"</p>
            </div>
          </Show>
        </div>
        <div class="explorer-main">
          <div class="query-panel">
            <div class="query-editor">
              <textarea
                class="query-textarea"
                placeholder="db.table('users').filter(doc => doc.age > 21).run()"
                prop:value=query
                on:input=move |ev| set_query.set(event_target_value(&ev))
              ></textarea>
            </div>
            <div class="query-actions">
              <button
                class="btn btn-primary"
                disabled=move || running.get()
                on:click=run_query
              >
                {move || if running.get() {
                  view! { <><Icon name="refresh-cw" size=14/>" Running..."</> }.into_view()
                } else {
                  view! { <><Icon name="play" size=14/>" Run Query"</> }.into_view()
                }}
              </button>
              <button
                class="btn btn-secondary"
                on:click=move |_| {
                  set_query.set(String::new());
                  set_results.set(None);
                  set_result_count.set(None);
                }
              >
                <Icon name="x" size=14/>
                " Clear"
              </button>
            </div>
          </div>
          <div class="results-panel">
            <div class="results-header">
              <h3>"Results"</h3>
              <Show when=move || result_count.get().is_some()>
                <span class="results-count">
                  {move || format!("{} documents", result_count.get().unwrap_or(0))}
                </span>
              </Show>
            </div>
            <div class="results-content">
              {move || match results.get() {
                Some(r) => view! { <pre class="results-json">{r}</pre> }.into_view(),
                None => view! {
                  <div class="results-placeholder">
                    <Icon name="search" size=32/>
                    <p>"Run a query to see results"</p>
                  </div>
                }.into_view(),
              }}
            </div>
          </div>
        </div>
      </div>
    </section>
  }
}
