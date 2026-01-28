//! Tables page component

use leptos::*;
use crate::admin::state::{AppState, ToastLevel};
use crate::admin::apiclient;
use super::Icon;

#[component]
pub fn Tables() -> impl IntoView {
  let state = use_context::<AppState>().expect("AppState not found");
  let tables = state.tables;

  let (loading, set_loading) = create_signal(true);
  let show_create_modal = create_rw_signal(false);
  let (new_table_name, set_new_table_name) = create_signal(String::new());
  let (creating, set_creating) = create_signal(false);

  // Load tables on mount
  {
    let state = state.clone();
    create_effect(move |_| {
      let state = state.clone();
      spawn_local(async move {
        match apiclient::fetch_tables().await {
          Ok(list) => {
            state.tables.set(list);
          }
          Err(e) => {
            state.show_toast(&format!("Failed to load tables: {}", e), ToastLevel::Error);
          }
        }
        set_loading.set(false);
      });
    });
  }

  view! {
    <section id="tables" class="page active">
      <div class="page-header">
        <h2>"Tables"</h2>
        <div class="page-header-actions">
          <button class="btn btn-primary" on:click=move |_| show_create_modal.set(true)>
            <Icon name="plus" size=16/>
            " Create Table"
          </button>
        </div>
      </div>

      <Show when=move || loading.get()>
        <div class="card">
          <div class="card-body">
            <div class="loading-spinner"></div>
            " Loading tables..."
          </div>
        </div>
      </Show>

      <Show when=move || !loading.get()>
        <Show
          when=move || !tables.get().is_empty()
          fallback=|| view! {
            <div class="card">
              <div class="card-body">
                <div class="empty-state">
                  <div class="empty-state-icon">
                    <Icon name="table" size=48/>
                  </div>
                  <p>"No tables yet"</p>
                  <p class="text-muted">"Create a table to start storing documents"</p>
                </div>
              </div>
            </div>
          }
        >
          <div class="card">
            <table class="data-table">
              <thead>
                <tr>
                  <th>"Name"</th>
                  <th>"Documents"</th>
                  <th>"Actions"</th>
                </tr>
              </thead>
              <tbody>
                <For
                  each=move || tables.get()
                  key=|t| t.name.clone()
                  children=move |table| {
                    let table_name_drop = table.name.clone();
                    view! {
                      <tr>
                        <td>
                          <div class="table-name-cell">
                            <Icon name="table" size=16/>
                            <strong>{table.name.clone()}</strong>
                          </div>
                        </td>
                        <td>{table.count}</td>
                        <td class="actions">
                          <button class="btn btn-ghost btn-sm" title="View documents">
                            <Icon name="eye" size=14/>
                            " View"
                          </button>
                          <DropTableButton name=table_name_drop/>
                        </td>
                      </tr>
                    }
                  }
                />
              </tbody>
            </table>
          </div>
        </Show>
      </Show>

      // Create Table Modal
      <Show when=move || show_create_modal.get()>
        <CreateTableModal
          show=show_create_modal
          name=new_table_name
          set_name=set_new_table_name
          creating=creating
          set_creating=set_creating
        />
      </Show>
    </section>
  }
}

#[component]
fn DropTableButton(name: String) -> impl IntoView {
  let state = use_context::<AppState>().expect("AppState not found");
  let name_clone = name.clone();

  view! {
    <button
      class="btn btn-ghost btn-sm text-danger"
      title="Drop table"
      on:click=move |_| {
        let state = state.clone();
        let name = name_clone.clone();
        spawn_local(async move {
          match apiclient::drop_table(&name).await {
            Ok(_) => {
              state.show_toast(&format!("Table '{}' dropped", name), ToastLevel::Success);
              if let Ok(list) = apiclient::fetch_tables().await {
                state.tables.set(list);
              }
            }
            Err(e) => {
              state.show_toast(&format!("Failed to drop: {}", e), ToastLevel::Error);
            }
          }
        });
      }
    >
      <Icon name="trash-2" size=14/>
      " Drop"
    </button>
  }
}

#[component]
fn CreateTableModal(
  show: RwSignal<bool>,
  name: ReadSignal<String>,
  set_name: WriteSignal<String>,
  creating: ReadSignal<bool>,
  set_creating: WriteSignal<bool>,
) -> impl IntoView {
  let state = use_context::<AppState>().expect("AppState not found");

  view! {
    <div class="modal-overlay active">
      <div class="modal">
        <div class="modal-header">
          <h3>"Create Table"</h3>
          <button class="modal-close" on:click=move |_| show.set(false)>
            <Icon name="x" size=18/>
          </button>
        </div>
        <div class="modal-body">
          <div class="form-group">
            <label>"Table Name"</label>
            <input
              type="text"
              class="input"
              placeholder="users"
              prop:value=name
              on:input=move |ev| set_name.set(event_target_value(&ev))
            />
            <p class="form-hint">"Lowercase letters, numbers, and underscores only"</p>
          </div>
        </div>
        <div class="modal-footer">
          <button class="btn btn-secondary" on:click=move |_| show.set(false)>
            "Cancel"
          </button>
          <button
            class="btn btn-primary"
            disabled=move || creating.get()
            on:click=move |_| {
              let table_name = name.get();
              if table_name.is_empty() {
                state.show_toast("Table name is required", ToastLevel::Warning);
                return;
              }
              set_creating.set(true);
              let state = state.clone();
              spawn_local(async move {
                match apiclient::create_table(&table_name).await {
                  Ok(_) => {
                    state.show_toast(&format!("Table '{}' created", table_name), ToastLevel::Success);
                    show.set(false);
                    set_name.set(String::new());
                    if let Ok(list) = apiclient::fetch_tables().await {
                      state.tables.set(list);
                    }
                  }
                  Err(e) => {
                    state.show_toast(&format!("Failed to create table: {}", e), ToastLevel::Error);
                  }
                }
                set_creating.set(false);
              });
            }
          >
            {move || if creating.get() { "Creating..." } else { "Create Table" }}
          </button>
        </div>
      </div>
    </div>
  }
}
