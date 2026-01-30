//! File preview modal component

use crate::admin::apiclient;
use leptos::*;
use wasm_bindgen::JsCast;

#[component]
pub fn PreviewModal<F>(bucket: String, object_key: String, on_close: F) -> impl IntoView
where
  F: Fn() + Clone + 'static,
{
  let filename = object_key.rsplit('/').next().unwrap_or(&object_key).to_string();
  let ext = filename.rsplit('.').next().unwrap_or("").to_lowercase();
  let download_url = apiclient::get_download_url(&bucket, &object_key);

  // Determine preview type
  let preview_type = match ext.as_str() {
    "jpg" | "jpeg" | "png" | "gif" | "webp" | "svg" | "ico" | "bmp" => "image",
    "mp4" | "webm" | "ogg" => "video",
    "mp3" | "wav" | "flac" | "aac" => "audio",
    "pdf" => "pdf",
    "txt" | "md" | "json" | "xml" | "yaml" | "yml" | "toml" | "ini" | "cfg" | "conf" | "log" => "text",
    "js" | "ts" | "jsx" | "tsx" | "rs" | "py" | "go" | "java" | "c" | "cpp" | "h" | "hpp" | "cs" | "rb" | "php" | "swift" | "kt" | "scala" | "sh" | "bash" | "zsh" | "ps1" | "sql" | "html" | "css" | "scss" | "sass" | "less" => "code",
    _ => "unsupported",
  };

  let on_close_backdrop = on_close.clone();
  let on_close_btn = on_close.clone();

  view! {
    <div class="modal-backdrop" on:click=move |_| on_close_backdrop()>
      <div class="modal preview-modal" on:click=move |ev: web_sys::MouseEvent| ev.stop_propagation()>
        <div class="modal-header">
          <h3>{filename.clone()}</h3>
          <div class="modal-header-actions">
            <a
              class="btn btn-primary"
              href=download_url.clone()
              target="_blank"
            >
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/>
                <polyline points="7 10 12 15 17 10"/>
                <line x1="12" y1="15" x2="12" y2="3"/>
              </svg>
              " Download"
            </a>
            <button class="btn btn-icon" on:click=move |_| on_close_btn()>
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <line x1="18" y1="6" x2="6" y2="18"/>
                <line x1="6" y1="6" x2="18" y2="18"/>
              </svg>
            </button>
          </div>
        </div>

        <div class="modal-body preview-content">
          {match preview_type {
            "image" => view! {
              <div class="preview-image">
                <img src=download_url alt=filename />
              </div>
            }.into_view(),
            "video" => view! {
              <div class="preview-video">
                <video controls>
                  <source src=download_url />
                  "Your browser does not support the video tag."
                </video>
              </div>
            }.into_view(),
            "audio" => view! {
              <div class="preview-audio">
                <audio controls>
                  <source src=download_url />
                  "Your browser does not support the audio tag."
                </audio>
              </div>
            }.into_view(),
            "pdf" => view! {
              <div class="preview-pdf">
                <iframe src=download_url />
              </div>
            }.into_view(),
            "text" | "code" => {
              let (content, set_content) = create_signal(Option::<String>::None);
              let (loading, set_loading) = create_signal(true);
              let url = download_url.clone();

              spawn_local(async move {
                let window = web_sys::window().unwrap();
                let response = wasm_bindgen_futures::JsFuture::from(window.fetch_with_str(&url))
                  .await
                  .ok();

                if let Some(resp) = response {
                  let resp: web_sys::Response = resp.dyn_into().unwrap();
                  if resp.ok() {
                    let text = wasm_bindgen_futures::JsFuture::from(resp.text().unwrap())
                      .await
                      .ok()
                      .and_then(|t| t.as_string());
                    set_content.set(text);
                  }
                }
                set_loading.set(false);
              });

              view! {
                <div class="preview-text">
                  <Show when=move || loading.get()>
                    <div class="preview-loading">"Loading..."</div>
                  </Show>
                  <Show when=move || !loading.get()>
                    <pre><code>{move || content.get().unwrap_or_else(|| "Failed to load content".to_string())}</code></pre>
                  </Show>
                </div>
              }.into_view()
            },
            _ => view! {
              <div class="preview-unsupported">
                <svg width="64" height="64" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1">
                  <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>
                  <polyline points="14 2 14 8 20 8"/>
                </svg>
                <p>"Preview not available for this file type"</p>
                <a class="btn btn-primary" href=download_url target="_blank">"Download to view"</a>
              </div>
            }.into_view(),
          }}
        </div>
      </div>
    </div>
  }
}
