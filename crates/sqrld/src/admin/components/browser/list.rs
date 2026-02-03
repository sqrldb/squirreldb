//! Object list component with folder navigation

use crate::admin::apiclient;
use crate::admin::state::{AppState, Page, ToastLevel};
use leptos::*;

#[component]
pub fn BucketBrowser(bucket: String) -> impl IntoView {
  let state = use_context::<AppState>().expect("AppState not found");

  let (prefix, set_prefix) = create_signal(String::new());
  let (objects, set_objects) = create_signal(Vec::new());
  let (folders, set_folders) = create_signal(Vec::new());
  let (loading, set_loading) = create_signal(true);
  let (selected, set_selected) = create_signal(std::collections::HashSet::<String>::new());
  let (show_upload, set_show_upload) = create_signal(false);
  let (preview_key, set_preview_key) = create_signal(Option::<String>::None);
  let (deleting, set_deleting) = create_signal(false);

  let bucket_clone = bucket.clone();
  let bucket_for_effect = bucket.clone();

  // Load objects on mount and when prefix changes
  create_effect(move |_| {
    let current_prefix = prefix.get();
    let bucket = bucket_for_effect.clone();
    set_loading.set(true);
    spawn_local(async move {
      match apiclient::list_bucket_objects(&bucket, Some(&current_prefix)).await {
        Ok((objs, fldrs)) => {
          set_objects.set(objs);
          set_folders.set(fldrs);
        }
        Err(e) => {
          leptos::logging::error!("Failed to list objects: {}", e);
        }
      }
      set_loading.set(false);
    });
  });

  // Breadcrumb parts
  let breadcrumb_parts = move || {
    let p = prefix.get();
    if p.is_empty() {
      return vec![];
    }
    let parts: Vec<String> = p
      .trim_end_matches('/')
      .split('/')
      .map(String::from)
      .collect();
    parts
  };

  // Navigate to folder
  let navigate_to_folder = move |folder: String| {
    set_prefix.set(folder);
    set_selected.set(std::collections::HashSet::new());
  };

  // Navigate up
  let navigate_up = move || {
    let p = prefix.get();
    if p.is_empty() {
      return;
    }
    let parts: Vec<&str> = p.trim_end_matches('/').split('/').collect();
    if parts.len() <= 1 {
      set_prefix.set(String::new());
    } else {
      let new_prefix = parts[..parts.len() - 1].join("/") + "/";
      set_prefix.set(new_prefix);
    }
    set_selected.set(std::collections::HashSet::new());
  };

  // Navigate to breadcrumb index
  let navigate_to_index = move |index: usize| {
    let parts = breadcrumb_parts();
    if index == 0 {
      set_prefix.set(String::new());
    } else {
      let new_prefix = parts[..index].join("/") + "/";
      set_prefix.set(new_prefix);
    }
    set_selected.set(std::collections::HashSet::new());
  };

  // Toggle selection
  let toggle_selection = move |key: String| {
    set_selected.update(|s| {
      if s.contains(&key) {
        s.remove(&key);
      } else {
        s.insert(key);
      }
    });
  };

  // Delete selected
  let state_delete = state.clone();
  let bucket_delete = bucket.clone();
  let delete_selected = move |_| {
    let keys: Vec<String> = selected.get().into_iter().collect();
    if keys.is_empty() {
      return;
    }
    set_deleting.set(true);
    let state = state_delete.clone();
    let bucket = bucket_delete.clone();
    let current_prefix = prefix.get();
    spawn_local(async move {
      let mut errors = 0;
      for key in &keys {
        if apiclient::delete_bucket_object(&bucket, key).await.is_err() {
          errors += 1;
        }
      }
      if errors > 0 {
        state.show_toast(
          &format!("{} objects failed to delete", errors),
          ToastLevel::Error,
        );
      } else {
        state.show_toast(
          &format!("{} objects deleted", keys.len()),
          ToastLevel::Success,
        );
      }
      // Refresh list
      if let Ok((objs, fldrs)) =
        apiclient::list_bucket_objects(&bucket, Some(&current_prefix)).await
      {
        set_objects.set(objs);
        set_folders.set(fldrs);
      }
      set_selected.set(std::collections::HashSet::new());
      set_deleting.set(false);
    });
  };

  // Back to buckets
  let state_back = state.clone();
  let go_back = move |_| {
    state_back.navigate(Page::Buckets);
  };

  // Format file size
  let format_size = |size: Option<i64>| -> String {
    match size {
      Some(s) if s >= 1024 * 1024 * 1024 => {
        format!("{:.1} GB", s as f64 / (1024.0 * 1024.0 * 1024.0))
      }
      Some(s) if s >= 1024 * 1024 => format!("{:.1} MB", s as f64 / (1024.0 * 1024.0)),
      Some(s) if s >= 1024 => format!("{:.1} KB", s as f64 / 1024.0),
      Some(s) => format!("{} B", s),
      None => "-".to_string(),
    }
  };

  // Get file icon
  let get_icon = |key: &str| -> &'static str {
    let ext = key.rsplit('.').next().unwrap_or("");
    match ext.to_lowercase().as_str() {
      "jpg" | "jpeg" | "png" | "gif" | "webp" | "svg" | "ico" => "image",
      "pdf" => "document",
      "doc" | "docx" | "txt" | "md" | "rtf" => "document",
      "xls" | "xlsx" | "csv" => "spreadsheet",
      "mp3" | "wav" | "ogg" | "flac" => "audio",
      "mp4" | "avi" | "mov" | "webm" => "video",
      "zip" | "tar" | "gz" | "rar" | "7z" => "archive",
      "js" | "ts" | "rs" | "py" | "go" | "java" | "c" | "cpp" | "h" => "code",
      "json" | "xml" | "yaml" | "yml" | "toml" => "config",
      _ => "file",
    }
  };

  let bucket_for_view = bucket_clone.clone();
  let bucket_for_upload = bucket_clone.clone();
  let bucket_for_upload_callback = bucket_clone.clone();
  let bucket_for_files_loop = bucket_clone.clone();
  let bucket_for_preview = bucket_clone.clone();

  view! {
    <div class="browser-container">
      // Header
      <div class="browser-header">
        <div class="browser-title">
          <button class="btn btn-icon" on:click=go_back title="Back to buckets">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M19 12H5M12 19l-7-7 7-7"/>
            </svg>
          </button>
          <h2>{bucket_for_view.clone()}</h2>
        </div>
        <div class="browser-actions">
          <button
            class="btn btn-primary"
            on:click=move |_| set_show_upload.set(true)
          >
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/>
              <polyline points="17 8 12 3 7 8"/>
              <line x1="12" y1="3" x2="12" y2="15"/>
            </svg>
            " Upload"
          </button>
          <button
            class="btn btn-danger"
            disabled=move || selected.get().is_empty() || deleting.get()
            on:click=delete_selected
          >
            {move || if deleting.get() { "Deleting..." } else { "Delete Selected" }}
          </button>
        </div>
      </div>

      // Breadcrumb
      <div class="browser-breadcrumb">
        <button
          class="breadcrumb-item"
          on:click=move |_| navigate_to_index(0)
        >
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M3 9l9-7 9 7v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/>
          </svg>
        </button>
        <For
          each=breadcrumb_parts
          key=|p| p.clone()
          children=move |part| {
            let part_clone = part.clone();
            let parts = breadcrumb_parts();
            let idx = parts.iter().position(|p| p == &part_clone).unwrap_or(0) + 1;
            view! {
              <span class="breadcrumb-separator">"/"</span>
              <button
                class="breadcrumb-item"
                on:click=move |_| navigate_to_index(idx)
              >
                {part.clone()}
              </button>
            }
          }
        />
      </div>

      // Object list
      <div class="browser-list">
        <Show when=move || loading.get()>
          <div class="browser-loading">
            "Loading..."
          </div>
        </Show>

        <Show when=move || !loading.get() && folders.get().is_empty() && objects.get().is_empty()>
          <div class="browser-empty">
            <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1">
              <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/>
            </svg>
            <p>"This folder is empty"</p>
          </div>
        </Show>

        // Back button (if in subfolder)
        <Show when=move || !prefix.get().is_empty()>
          <div class="browser-item" on:click=move |_| navigate_up()>
            <div class="browser-item-checkbox"></div>
            <div class="browser-item-icon folder">
              <svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor">
                <path d="M10 4H4c-1.1 0-1.99.9-1.99 2L2 18c0 1.1.9 2 2 2h16c1.1 0 2-.9 2-2V8c0-1.1-.9-2-2-2h-8l-2-2z"/>
              </svg>
            </div>
            <div class="browser-item-name">".."</div>
            <div class="browser-item-size">"-"</div>
            <div class="browser-item-modified">"-"</div>
            <div class="browser-item-actions"></div>
          </div>
        </Show>

        // Folders
        <For
          each=move || folders.get()
          key=|f| f.clone()
          children=move |folder| {
            let folder_name = folder.trim_end_matches('/').rsplit('/').next().unwrap_or(&folder).to_string();
            let folder_click = folder.clone();
            view! {
              <div
                class="browser-item"
                on:click=move |_| navigate_to_folder(folder_click.clone())
              >
                <div class="browser-item-checkbox"></div>
                <div class="browser-item-icon folder">
                  <svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor">
                    <path d="M10 4H4c-1.1 0-1.99.9-1.99 2L2 18c0 1.1.9 2 2 2h16c1.1 0 2-.9 2-2V8c0-1.1-.9-2-2-2h-8l-2-2z"/>
                  </svg>
                </div>
                <div class="browser-item-name">{folder_name}</div>
                <div class="browser-item-size">"-"</div>
                <div class="browser-item-modified">"-"</div>
                <div class="browser-item-actions">
                  <button class="btn btn-sm btn-ghost">"Open"</button>
                </div>
              </div>
            }
          }
        />

        // Files
        <For
          each=move || objects.get()
          key=|o| o.key.clone()
          children={
            let bucket_for_loop = bucket_for_files_loop.clone();
            move |obj| {
            let key = obj.key.clone();
            let key_for_class = key.clone();
            let key_for_checked = key.clone();
            let key_for_change = key.clone();
            let key_for_preview = key.clone();
            let key_for_download = key.clone();
            let key_for_delete = key.clone();
            let bucket_for_download = bucket_for_loop.clone();
            let bucket_for_delete = bucket_for_loop.clone();
            let filename = key.rsplit('/').next().unwrap_or(&key).to_string();
            let size = format_size(obj.size);
            let modified = obj.last_modified.clone().unwrap_or_else(|| "-".to_string());
            let icon = get_icon(&key);

            view! {
              <div class=move || if selected.get().contains(&key_for_class) { "browser-item selected" } else { "browser-item" }>
                <div class="browser-item-checkbox">
                  <input
                    type="checkbox"
                    prop:checked=move || selected.get().contains(&key_for_checked)
                    on:change=move |_| toggle_selection(key_for_change.clone())
                  />
                </div>
                <div class=format!("browser-item-icon {}", icon)>
                  <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                    <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>
                    <polyline points="14 2 14 8 20 8"/>
                  </svg>
                </div>
                <div class="browser-item-name">{filename}</div>
                <div class="browser-item-size">{size}</div>
                <div class="browser-item-modified">{modified}</div>
                <div class="browser-item-actions">
                  <button
                    class="btn btn-sm btn-ghost"
                    on:click=move |_| set_preview_key.set(Some(key_for_preview.clone()))
                  >
                    "View"
                  </button>
                  <a
                    class="btn btn-sm btn-ghost"
                    href=apiclient::get_download_url(&bucket_for_download, &key_for_download)
                    target="_blank"
                  >
                    "Download"
                  </a>
                  <button
                    class="btn btn-sm btn-ghost btn-danger"
                    on:click=move |_| {
                      let bucket = bucket_for_delete.clone();
                      let key = key_for_delete.clone();
                      let current_prefix = prefix.get();
                      spawn_local(async move {
                        if apiclient::delete_bucket_object(&bucket, &key).await.is_ok() {
                          if let Ok((objs, fldrs)) = apiclient::list_bucket_objects(&bucket, Some(&current_prefix)).await {
                            set_objects.set(objs);
                            set_folders.set(fldrs);
                          }
                        }
                      });
                    }
                  >
                    "Delete"
                  </button>
                </div>
              </div>
            }
          }}
        />
      </div>

      // Upload modal
      {
        let bucket_for_upload_cb = bucket_for_upload_callback.clone();
        view! {
          <Show when=move || show_upload.get()>
            <super::upload::UploadModal
              bucket=bucket_for_upload.clone()
              prefix=prefix.get()
              on_close={
                let bucket = bucket_for_upload_cb.clone();
                move || {
                  set_show_upload.set(false);
                  // Refresh list after upload
                  let bucket = bucket.clone();
                  let current_prefix = prefix.get();
                  spawn_local(async move {
                    if let Ok((objs, fldrs)) = apiclient::list_bucket_objects(&bucket, Some(&current_prefix)).await {
                      set_objects.set(objs);
                      set_folders.set(fldrs);
                    }
                  });
                }
              }
            />
          </Show>
        }
      }

      // Preview modal
      <Show when=move || preview_key.get().is_some()>
        <super::preview::PreviewModal
          bucket=bucket_for_preview.clone()
          object_key=preview_key.get().unwrap_or_default()
          on_close=move || set_preview_key.set(None)
        />
      </Show>
    </div>
  }
}
