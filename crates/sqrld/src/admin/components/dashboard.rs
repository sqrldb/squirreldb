//! Dashboard component

use super::Icon;
use crate::admin::state::AppState;
use leptos::*;

#[component]
pub fn Dashboard() -> impl IntoView {
  let state = use_context::<AppState>().expect("AppState not found");
  let stats = state.stats;
  let tables = state.tables;

  view! {
    <section id="dashboard" class="page active">
      <div class="page-header">
        <h2>"Dashboard"</h2>
      </div>
      <div class="stats-grid">
        <div class="stat-card">
          <div class="stat-icon"><Icon name="layers" size=24/></div>
          <div class="stat-value">{move || stats.get().tables}</div>
          <div class="stat-label">"Tables"</div>
        </div>
        <div class="stat-card">
          <div class="stat-icon"><Icon name="file-text" size=24/></div>
          <div class="stat-value">{move || stats.get().documents}</div>
          <div class="stat-label">"Documents"</div>
        </div>
        <div class="stat-card">
          <div class="stat-icon"><Icon name="server" size=24/></div>
          <div class="stat-value">{move || stats.get().backend.clone()}</div>
          <div class="stat-label">"Backend"</div>
        </div>
        <div class="stat-card">
          <div class="stat-icon"><Icon name="clock" size=24/></div>
          <div class="stat-value">{move || format_uptime(stats.get().uptime_secs)}</div>
          <div class="stat-label">"Uptime"</div>
        </div>
      </div>
      <div class="tables-overview">
        <div class="section-header">
          <h3>"Tables"</h3>
        </div>
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
                view! {
                  <tr>
                    <td>{table.name.clone()}</td>
                    <td>{table.count}</td>
                    <td class="actions">
                      <button class="btn btn-ghost btn-sm">
                        <Icon name="eye" size=14/>
                        " View"
                      </button>
                    </td>
                  </tr>
                }
              }
            />
          </tbody>
        </table>
        <Show when=move || tables.get().is_empty()>
          <div class="empty-state">
            <p class="text-muted">"No tables yet"</p>
          </div>
        </Show>
      </div>
    </section>
  }
}

fn format_uptime(secs: u64) -> String {
  if secs < 60 {
    format!("{}s", secs)
  } else if secs < 3600 {
    format!("{}m", secs / 60)
  } else if secs < 86400 {
    format!("{}h", secs / 3600)
  } else {
    format!("{}d", secs / 86400)
  }
}
