use leptos::*;
use leptos_router::*;

use crate::admin::apiclient;
use crate::admin::state::{AppState, ProjectInfo, ToastLevel};

#[component]
pub fn Projects() -> impl IntoView {
  let state = use_context::<AppState>().expect("AppState");
  let projects = state.projects;
  let current_project = state.current_project;

  // Modal state
  let show_create_modal = create_rw_signal(false);
  let new_project_name = create_rw_signal(String::new());
  let new_project_description = create_rw_signal(String::new());
  let creating = create_rw_signal(false);

  // Store state for use in closures
  let state_stored = store_value(state.clone());

  // Fetch projects on mount
  create_effect(move |_| {
    let state = state_stored.get_value();
    spawn_local(async move {
      if let Ok(fetched) = apiclient::fetch_projects().await {
        state.projects.set(fetched);
      }
    });
  });

  let create_project = move |_| {
    creating.set(true);
    let name = new_project_name.get();
    let description = new_project_description.get();
    let state = state_stored.get_value();
    spawn_local(async move {
      let desc = if description.is_empty() {
        None
      } else {
        Some(description.as_str())
      };
      match apiclient::create_project(&name, desc).await {
        Ok(project) => {
          state.projects.update(|ps| ps.push(project));
          state.show_toast("Project created", ToastLevel::Success);
          show_create_modal.set(false);
          new_project_name.set(String::new());
          new_project_description.set(String::new());
        }
        Err(e) => {
          state.show_toast(
            &format!("Failed to create project: {}", e),
            ToastLevel::Error,
          );
        }
      }
      creating.set(false);
    });
  };

  let navigate = use_navigate();
  let navigate_stored = store_value(navigate);
  let select_project = move |project: ProjectInfo| {
    let state = state_stored.get_value();
    let nav = navigate_stored.get_value();
    current_project.set(Some(project.id.clone()));
    state.show_toast(
      &format!("Switched to project: {}", project.name),
      ToastLevel::Info,
    );
    nav("/dashboard", Default::default());
  };

  let delete_project = move |project_id: String| {
    let state = state_stored.get_value();
    spawn_local(async move {
      match apiclient::delete_project(&project_id).await {
        Ok(_) => {
          state
            .projects
            .update(|ps| ps.retain(|p| p.id != project_id));
          state.show_toast("Project deleted", ToastLevel::Success);
          // If we deleted the current project, switch to none
          if current_project.get() == Some(project_id.clone()) {
            current_project.set(None);
          }
        }
        Err(e) => {
          state.show_toast(&format!("Failed to delete: {}", e), ToastLevel::Error);
        }
      }
    });
  };

  view! {
    <div class="page-content">
      <div class="page-header">
        <h2>"Projects"</h2>
        <button class="btn btn-primary" on:click=move |_| show_create_modal.set(true)>
          "+ New Project"
        </button>
      </div>

      <div class="projects-grid">
        <For
          each=move || projects.get()
          key=|p| p.id.clone()
          children=move |project| {
            let project_id = project.id.clone();
            let project_id_for_class = project_id.clone();
            let project_for_select = project.clone();
            let is_default = project.id == "00000000-0000-0000-0000-000000000000";
            let project_name = project.name.clone();
            let project_description = project.description.clone().unwrap_or_else(|| "No description".to_string());
            let created_date = project.created_at.split('T').next().unwrap_or(&project.created_at).to_string();

            view! {
              <div
                class=move || format!("project-card {}", if current_project.get() == Some(project_id_for_class.clone()) { "active" } else { "" })
                on:click=move |_| select_project(project_for_select.clone())
              >
                <div class="project-card-header">
                  <h3>{project_name.clone()}</h3>
                  {move || {
                    if current_project.get() == Some(project_id.clone()) {
                      view! { <span class="badge badge-primary">"Current"</span> }.into_view()
                    } else {
                      view! {}.into_view()
                    }
                  }}
                </div>
                <p class="project-description">
                  {project_description.clone()}
                </p>
                <div class="project-card-footer">
                  <span class="project-date">
                    "Created: " {created_date.clone()}
                  </span>
                  {if !is_default {
                    let pid = project.id.clone();
                    view! {
                      <button
                        class="btn btn-sm btn-danger"
                        on:click=move |e| {
                          e.stop_propagation();
                          delete_project(pid.clone());
                        }
                      >
                        "Delete"
                      </button>
                    }.into_view()
                  } else {
                    view! {
                      <span class="badge badge-secondary">"Default"</span>
                    }.into_view()
                  }}
                </div>
              </div>
            }
          }
        />
      </div>

      // Create Project Modal
      <Show when=move || show_create_modal.get()>
        <div class="modal-overlay" on:click=move |_| show_create_modal.set(false)>
          <div class="modal" on:click=|e| e.stop_propagation()>
            <div class="modal-header">
              <h3>"Create Project"</h3>
              <button class="btn-close" on:click=move |_| show_create_modal.set(false)>"x"</button>
            </div>
            <div class="modal-body">
              <div class="form-group">
                <label>"Project Name"</label>
                <input
                  type="text"
                  class="form-control"
                  placeholder="my-project"
                  prop:value=move || new_project_name.get()
                  on:input=move |e| new_project_name.set(event_target_value(&e))
                />
              </div>
              <div class="form-group">
                <label>"Description (optional)"</label>
                <textarea
                  class="form-control"
                  placeholder="Project description"
                  rows="3"
                  prop:value=move || new_project_description.get()
                  on:input=move |e| new_project_description.set(event_target_value(&e))
                />
              </div>
            </div>
            <div class="modal-footer">
              <button class="btn btn-secondary" on:click=move |_| show_create_modal.set(false)>
                "Cancel"
              </button>
              <button
                class="btn btn-primary"
                disabled=move || creating.get() || new_project_name.get().trim().is_empty()
                on:click=create_project
              >
                {move || if creating.get() { "Creating..." } else { "Create" }}
              </button>
            </div>
          </div>
        </div>
      </Show>
    </div>
  }
}
