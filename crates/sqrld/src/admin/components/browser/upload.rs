//! Upload modal component with drag-drop support

use crate::admin::apiclient;
use crate::admin::state::{AppState, ToastLevel};
use leptos::*;
use wasm_bindgen::JsCast;
use web_sys::{DragEvent, FileList, HtmlInputElement};

#[component]
pub fn UploadModal<F>(bucket: String, prefix: String, on_close: F) -> impl IntoView
where
  F: Fn() + Clone + 'static,
{
  let state = use_context::<AppState>().expect("AppState not found");

  let (files, set_files) = create_signal(Vec::<web_sys::File>::new());
  let (uploading, set_uploading) = create_signal(false);
  let (drag_over, set_drag_over) = create_signal(false);
  let (progress, set_progress) = create_signal(0usize);

  // Handle file selection
  let handle_files = move |file_list: FileList| {
    let mut new_files = Vec::new();
    for i in 0..file_list.length() {
      if let Some(file) = file_list.get(i) {
        new_files.push(file);
      }
    }
    set_files.update(|f| f.extend(new_files));
  };

  // Handle file input change
  let on_file_input = move |ev: web_sys::Event| {
    if let Some(input) = ev.target().and_then(|t| t.dyn_into::<HtmlInputElement>().ok()) {
      if let Some(file_list) = input.files() {
        handle_files(file_list);
      }
    }
  };

  // Handle drag over
  let on_drag_over = move |ev: DragEvent| {
    ev.prevent_default();
    set_drag_over.set(true);
  };

  // Handle drag leave
  let on_drag_leave = move |_: DragEvent| {
    set_drag_over.set(false);
  };

  // Handle drop
  let on_drop = move |ev: DragEvent| {
    ev.prevent_default();
    set_drag_over.set(false);
    if let Some(dt) = ev.data_transfer() {
      if let Some(file_list) = dt.files() {
        handle_files(file_list);
      }
    }
  };

  // Remove file from list
  let remove_file = move |index: usize| {
    set_files.update(|f| {
      f.remove(index);
    });
  };

  // Upload files
  let on_close_clone = on_close.clone();
  let state_upload = state.clone();
  let bucket_upload = bucket.clone();
  let prefix_upload = prefix.clone();
  let upload_files = move |_| {
    let files_to_upload = files.get();
    if files_to_upload.is_empty() {
      return;
    }

    set_uploading.set(true);
    set_progress.set(0);

    let state = state_upload.clone();
    let bucket = bucket_upload.clone();
    let prefix = prefix_upload.clone();
    let on_close = on_close_clone.clone();

    spawn_local(async move {
      let mut uploaded = 0;
      let mut errors = 0;

      for file in files_to_upload {
        let name = file.name();
        let _key = if prefix.is_empty() {
          name.clone()
        } else {
          format!("{}{}", prefix, name)
        };

        // Read file as bytes
        let array_buffer = wasm_bindgen_futures::JsFuture::from(file.array_buffer())
          .await
          .ok();

        if let Some(buffer) = array_buffer {
          let uint8_array = js_sys::Uint8Array::new(&buffer);

          // Create form data
          let form_data = web_sys::FormData::new().unwrap();
          let blob = web_sys::Blob::new_with_u8_array_sequence(&js_sys::Array::of1(&uint8_array)).ok();
          if let Some(b) = blob {
            let _ = form_data.append_with_blob_and_filename(&name, &b, &name);
          }

          // Upload via fetch
          let url = format!("/api/s3/buckets/{}/upload", bucket);
          let token = apiclient::get_stored_token().unwrap_or_default();

          let window = web_sys::window().unwrap();
          let init = web_sys::RequestInit::new();
          init.set_method("POST");
          init.set_body(&form_data);
          let request = web_sys::Request::new_with_str_and_init(&url, &init).ok();

          if let Some(req) = request {
            let _ = req.headers().set("Authorization", &format!("Bearer {}", token));
            let response = wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(&req))
              .await
              .ok();

            if let Some(resp) = response {
              let resp: web_sys::Response = resp.dyn_into().unwrap();
              if resp.ok() {
                uploaded += 1;
              } else {
                errors += 1;
              }
            } else {
              errors += 1;
            }
          } else {
            errors += 1;
          }
        } else {
          errors += 1;
        }

        set_progress.set(uploaded + errors);
      }

      if errors > 0 {
        state.show_toast(&format!("Uploaded {} files, {} failed", uploaded, errors), ToastLevel::Warning);
      } else {
        state.show_toast(&format!("Uploaded {} files", uploaded), ToastLevel::Success);
      }

      on_close();
    });
  };

  let format_size = |size: f64| -> String {
    if size >= 1024.0 * 1024.0 * 1024.0 {
      format!("{:.1} GB", size / (1024.0 * 1024.0 * 1024.0))
    } else if size >= 1024.0 * 1024.0 {
      format!("{:.1} MB", size / (1024.0 * 1024.0))
    } else if size >= 1024.0 {
      format!("{:.1} KB", size / 1024.0)
    } else {
      format!("{:.0} B", size)
    }
  };

  let on_close_backdrop = on_close.clone();
  let on_close_x = on_close.clone();
  let on_close_cancel = on_close.clone();

  view! {
    <div class="modal-backdrop" on:click=move |_| on_close_backdrop()>
      <div class="modal upload-modal" on:click=move |ev: web_sys::MouseEvent| ev.stop_propagation()>
        <div class="modal-header">
          <h3>"Upload Files"</h3>
          <button class="btn btn-icon" on:click=move |_| on_close_x()>
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <line x1="18" y1="6" x2="6" y2="18"/>
              <line x1="6" y1="6" x2="18" y2="18"/>
            </svg>
          </button>
        </div>

        <div class="modal-body">
          // Drop zone
          <div
            class=move || if drag_over.get() { "upload-dropzone drag-over" } else { "upload-dropzone" }
            on:dragover=on_drag_over
            on:dragleave=on_drag_leave
            on:drop=on_drop
          >
            <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
              <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/>
              <polyline points="17 8 12 3 7 8"/>
              <line x1="12" y1="3" x2="12" y2="15"/>
            </svg>
            <p>"Drag and drop files here"</p>
            <p class="text-muted">"or"</p>
            <label class="btn btn-primary">
              "Choose Files"
              <input
                type="file"
                multiple=true
                style="display: none"
                on:change=on_file_input
              />
            </label>
          </div>

          // File list
          <Show when=move || !files.get().is_empty()>
            <div class="upload-file-list">
              <h4>"Selected Files (" {move || files.get().len()} ")"</h4>
              {move || {
                files.get().into_iter().enumerate().map(|(index, file)| {
                  let name = file.name();
                  let size = format_size(file.size() as f64);
                  view! {
                    <div class="upload-file-item">
                      <div class="upload-file-info">
                        <span class="upload-file-name">{name}</span>
                        <span class="upload-file-size">{size}</span>
                      </div>
                      <button
                        class="btn btn-icon btn-sm"
                        on:click=move |_| remove_file(index)
                        disabled=uploading
                      >
                        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                          <line x1="18" y1="6" x2="6" y2="18"/>
                          <line x1="6" y1="6" x2="18" y2="18"/>
                        </svg>
                      </button>
                    </div>
                  }
                }).collect_view()
              }}
            </div>
          </Show>

          // Progress
          <Show when=move || uploading.get()>
            <div class="upload-progress">
              <div class="progress-bar">
                <div
                  class="progress-fill"
                  style=move || format!("width: {}%", (progress.get() as f64 / files.get().len().max(1) as f64) * 100.0)
                />
              </div>
              <span>{move || format!("{}/{} files", progress.get(), files.get().len())}</span>
            </div>
          </Show>
        </div>

        <div class="modal-footer">
          <button class="btn" on:click=move |_| on_close_cancel() disabled=move || uploading.get()>
            "Cancel"
          </button>
          <button
            class="btn btn-primary"
            disabled=move || files.get().is_empty() || uploading.get()
            on:click=upload_files
          >
            {move || if uploading.get() { "Uploading..." } else { "Upload" }}
          </button>
        </div>
      </div>
    </div>
  }
}
