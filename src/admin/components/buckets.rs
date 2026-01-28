//! Buckets page component for S3 bucket management

use super::Icon;
use crate::admin::apiclient;
use crate::admin::state::{AppState, ToastLevel};
use leptos::*;

#[component]
pub fn Buckets() -> impl IntoView {
  let state = use_context::<AppState>().expect("AppState not found");
  let buckets = state.buckets;

  let (loading, set_loading) = create_signal(true);
  let show_create_modal = create_rw_signal(false);
  let (new_bucket_name, set_new_bucket_name) = create_signal(String::new());
  let (creating, set_creating) = create_signal(false);

  // Load buckets on mount
  {
    let state = state.clone();
    create_effect(move |_| {
      let state = state.clone();
      spawn_local(async move {
        match apiclient::fetch_buckets().await {
          Ok(list) => {
            state.buckets.set(list);
          }
          Err(e) => {
            state.show_toast(&format!("Failed to load buckets: {}", e), ToastLevel::Error);
          }
        }
        set_loading.set(false);
      });
    });
  }

  view! {
    <section id="buckets" class="page active">
      <div class="page-header">
        <h2>"Buckets"</h2>
        <div class="page-header-actions">
          <button class="btn btn-primary" on:click=move |_| show_create_modal.set(true)>
            <Icon name="plus" size=16/>
            " Create Bucket"
          </button>
        </div>
      </div>

      <Show when=move || loading.get()>
        <div class="card">
          <div class="card-body">
            <div class="loading-spinner"></div>
            " Loading buckets..."
          </div>
        </div>
      </Show>

      <Show when=move || !loading.get()>
        <Show
          when=move || !buckets.get().is_empty()
          fallback=|| view! {
            <div class="card">
              <div class="card-body">
                <div class="empty-state">
                  <div class="empty-state-icon">"🪣"</div>
                  <p>"No buckets yet"</p>
                  <p class="text-muted">"Create a bucket to start storing objects"</p>
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
                  <th>"Objects"</th>
                  <th>"Size"</th>
                  <th>"Actions"</th>
                </tr>
              </thead>
              <tbody>
                <For
                  each=move || buckets.get()
                  key=|b| b.name.clone()
                  children=move |bucket| {
                    let bucket_name_delete = bucket.name.clone();
                    view! {
                      <tr>
                        <td>
                          <strong>{bucket.name.clone()}</strong>
                        </td>
                        <td>{bucket.object_count}</td>
                        <td>{format_size(bucket.current_size)}</td>
                        <td class="actions">
                          <button
                            class="btn btn-ghost btn-sm"
                            title="View bucket"
                          >
                            <Icon name="eye" size=14/>
                            " View"
                          </button>
                          <DeleteBucketButton name=bucket_name_delete/>
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

      // Create Bucket Modal
      <Show when=move || show_create_modal.get()>
        <CreateBucketModal
          show=show_create_modal
          name=new_bucket_name
          set_name=set_new_bucket_name
          creating=creating
          set_creating=set_creating
        />
      </Show>
    </section>
  }
}

#[component]
fn DeleteBucketButton(name: String) -> impl IntoView {
  let state = use_context::<AppState>().expect("AppState not found");
  let name_clone = name.clone();

  view! {
    <button
      class="btn btn-ghost btn-sm text-danger"
      title="Delete bucket"
      on:click=move |_| {
        let state = state.clone();
        let name = name_clone.clone();
        spawn_local(async move {
          match apiclient::delete_bucket(&name).await {
            Ok(_) => {
              state.show_toast(&format!("Bucket '{}' deleted", name), ToastLevel::Success);
              if let Ok(list) = apiclient::fetch_buckets().await {
                state.buckets.set(list);
              }
            }
            Err(e) => {
              state.show_toast(&format!("Failed to delete: {}", e), ToastLevel::Error);
            }
          }
        });
      }
    >
      <Icon name="trash-2" size=14/>
      " Delete"
    </button>
  }
}

#[component]
fn CreateBucketModal(
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
          <h3>"Create Bucket"</h3>
          <button class="modal-close" on:click=move |_| show.set(false)>
            <Icon name="x" size=18/>
          </button>
        </div>
        <div class="modal-body">
          <div class="form-group">
            <label>"Bucket Name"</label>
            <input
              type="text"
              class="input"
              placeholder="my-bucket"
              prop:value=name
              on:input=move |ev| set_name.set(event_target_value(&ev))
            />
            <p class="form-hint">"3-63 characters, lowercase letters, numbers, and hyphens only"</p>
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
              let bucket_name = name.get();
              if bucket_name.is_empty() {
                state.show_toast("Bucket name is required", ToastLevel::Warning);
                return;
              }
              set_creating.set(true);
              let state = state.clone();
              spawn_local(async move {
                match apiclient::create_bucket(&bucket_name).await {
                  Ok(_) => {
                    state.show_toast(&format!("Bucket '{}' created", bucket_name), ToastLevel::Success);
                    show.set(false);
                    set_name.set(String::new());
                    if let Ok(list) = apiclient::fetch_buckets().await {
                      state.buckets.set(list);
                    }
                  }
                  Err(e) => {
                    state.show_toast(&format!("Failed to create bucket: {}", e), ToastLevel::Error);
                  }
                }
                set_creating.set(false);
              });
            }
          >
            {move || if creating.get() { "Creating..." } else { "Create Bucket" }}
          </button>
        </div>
      </div>
    </div>
  }
}

fn format_size(bytes: i64) -> String {
  if bytes < 1024 {
    format!("{} B", bytes)
  } else if bytes < 1024 * 1024 {
    format!("{:.1} KB", bytes as f64 / 1024.0)
  } else if bytes < 1024 * 1024 * 1024 {
    format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
  } else {
    format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
  }
}
