//! Skill Instance Management Page
//!
//! Create, manage, and execute skill instances bound to agents.

use crate::api::{CreateInstanceRequest, InstanceInfo};
use crate::i18n::I18nContext;
use crate::state::use_app_state;
use leptos::prelude::*;
use leptos::view;
use leptos_meta::*;

#[component]
pub fn SkillInstancesPage() -> impl IntoView {
    let app_state = use_app_state();
    let show_create_form = RwSignal::new(false);
    let create_skill_id = RwSignal::new(String::new());
    let create_agent_id = RwSignal::new(String::new());
    let is_creating = RwSignal::new(false);
    let is_executing = RwSignal::new(None::<String>);
    let i18n = use_context::<I18nContext>().expect("i18n context not found");
    let i18n_stored = StoredValue::new(i18n);

    // Fetch instances
    let instances = LocalResource::new({
        let app_state = app_state.clone();
        move || {
            let service = app_state.skill_service();
            let app_state = app_state.clone();
            async move {
                app_state.loading().skills.set(true);
                let result = service.list_instances().await;
                app_state.loading().skills.set(false);
                result
            }
        }
    });

    let reload_instances = {
        let instances = instances.clone();
        move || instances.refetch()
    };

    let create_instance = {
        let app_state = app_state.clone();
        let reload = reload_instances.clone();
        let i18n = i18n_stored.clone();
        move || {
            let skill_id = create_skill_id.get();
            let agent_id = create_agent_id.get();
            if skill_id.is_empty() || agent_id.is_empty() {
                app_state.notify(
                    crate::state::notification::NotificationType::Warning,
                    i18n.get_value().t("instances-missing-fields-title"),
                    i18n.get_value().t("instances-missing-fields-msg"),
                );
                return;
            }
            is_creating.set(true);
            let service = app_state.skill_service();
            let app_state = app_state.clone();
            let reload = reload.clone();
            let i18n = i18n.clone();
            leptos::task::spawn_local(async move {
                let req = CreateInstanceRequest {
                    skill_id,
                    agent_id,
                    config: std::collections::HashMap::new(),
                };
                match service.create_instance(req).await {
                    Ok(instance) => {
                        app_state.notify(
                            crate::state::notification::NotificationType::Success,
                            i18n.get_value().t("instances-create-success-title"),
                            format!("{} {}", instance.instance_id, i18n.get_value().t("instances-create-success-msg")),
                        );
                        create_skill_id.set(String::new());
                        create_agent_id.set(String::new());
                        show_create_form.set(false);
                        reload();
                    }
                    Err(e) => {
                        app_state.notify(
                            crate::state::notification::NotificationType::Error,
                            i18n.get_value().t("instances-create-fail-title"),
                            format!("{}: {}", i18n.get_value().t("instances-create-fail-msg"), e),
                        );
                    }
                }
                is_creating.set(false);
            });
        }
    };
    let create_instance_cb = StoredValue::new(create_instance);

    let delete_instance = {
        let app_state = app_state.clone();
        let reload = reload_instances.clone();
        let i18n = i18n_stored.clone();
        move |instance_id: String| {
            let service = app_state.skill_service();
            let app_state = app_state.clone();
            let reload = reload.clone();
            let i18n = i18n.clone();
            leptos::task::spawn_local(async move {
                match service.delete_instance(&instance_id).await {
                    Ok(()) => {
                        app_state.notify(
                            crate::state::notification::NotificationType::Success,
                            i18n.get_value().t("instances-delete-success-title"),
                            format!("{} {}", instance_id, i18n.get_value().t("instances-delete-success-msg")),
                        );
                        reload();
                    }
                    Err(e) => {
                        app_state.notify(
                            crate::state::notification::NotificationType::Error,
                            i18n.get_value().t("instances-delete-fail-title"),
                            format!("{}: {}", i18n.get_value().t("instances-delete-fail-msg"), e),
                        );
                    }
                }
            });
        }
    };
    let delete_instance_cb = StoredValue::new(delete_instance);

    let execute_instance = {
        let app_state = app_state.clone();
        let i18n = i18n_stored.clone();
        move |instance_id: String| {
            is_executing.set(Some(instance_id.clone()));
            let service = app_state.skill_service();
            let app_state = app_state.clone();
            let i18n = i18n.clone();
            leptos::task::spawn_local(async move {
                match service.execute_instance(&instance_id).await {
                    Ok(resp) => {
                        let msg = if resp.success {
                            format!("{} {}ms", i18n.get_value().t("instances-exec-completed"), resp.execution_time_ms)
                        } else {
                            format!("{}: {}", i18n.get_value().t("instances-exec-failed"), resp.output)
                        };
                        app_state.notify(
                            crate::state::notification::NotificationType::Success,
                            i18n.get_value().t("instances-exec-result-title"),
                            msg,
                        );
                    }
                    Err(e) => {
                        app_state.notify(
                            crate::state::notification::NotificationType::Error,
                            i18n.get_value().t("instances-exec-fail-title"),
                            format!("{}: {}", i18n.get_value().t("instances-exec-fail-msg"), e),
                        );
                    }
                }
                is_executing.set(None);
            });
        }
    };
    let execute_instance_cb = StoredValue::new(execute_instance);

    view! {
        <Title text={move || i18n_stored.get_value().t("instances-page-title")} />
        <div class="page skill-instances-page">
            <div class="page-header">
                <div>
                    <h1>{move || i18n_stored.get_value().t("instances-title")}</h1>
                    <p class="page-description">{move || i18n_stored.get_value().t("instances-subtitle")}</p>
                </div>
                <button
                    class="btn btn-primary"
                    on:click=move |_| show_create_form.update(|v| *v = !*v)
                >
                    {move || if show_create_form.get() { i18n_stored.get_value().t("instances-cancel") } else { i18n_stored.get_value().t("instances-new") }}
                </button>
            </div>

            {move || if show_create_form.get() {
                view! {
                    <div class="create-form card">
                        <h3>{move || i18n_stored.get_value().t("instances-create-title")}</h3>
                        <div class="form-group">
                            <label>{move || i18n_stored.get_value().t("instances-skill-id")}</label>
                            <input
                                type="text"
                                placeholder={move || i18n_stored.get_value().t("instances-skill-id-placeholder")}
                                prop:value=create_skill_id
                                on:input=move |e| create_skill_id.set(event_target_value(&e))
                            />
                        </div>
                        <div class="form-group">
                            <label>{move || i18n_stored.get_value().t("instances-agent-id")}</label>
                            <input
                                type="text"
                                placeholder={move || i18n_stored.get_value().t("instances-agent-id-placeholder")}
                                prop:value=create_agent_id
                                on:input=move |e| create_agent_id.set(event_target_value(&e))
                            />
                        </div>
                        <button
                            class="btn btn-primary"
                            disabled=move || is_creating.get()
                            on:click=move |_| create_instance_cb.with_value(|f| f())
                        >
                            {move || if is_creating.get() { i18n_stored.get_value().t("instances-creating") } else { i18n_stored.get_value().t("instances-create-btn") }}
                        </button>
                    </div>
                }.into_any()
            } else {
                view! { <></> }.into_any()
            }}

            <Suspense fallback=|| view! { <InstancesLoading/> }>
                {move || {
                    Suspend::new(async move {
                        match instances.await {
                            Ok(data) => {
                                if data.is_empty() {
                                    view! { <InstancesEmpty i18n=i18n_stored.get_value()/> }.into_any()
                                } else {
                                    view! {
                                        <InstancesTable
                                            instances=data
                                            on_delete=move |id| delete_instance_cb.with_value(|f| f(id))
                                            on_execute=move |id| execute_instance_cb.with_value(|f| f(id))
                                            executing_id=is_executing.clone()
                                            i18n=i18n_stored.get_value()
                                        />
                                    }.into_any()
                                }
                            }
                            Err(e) => view! { <InstancesError message=e.to_string() i18n=i18n_stored.get_value()/> }.into_any(),
                        }
                    })
                }}
            </Suspense>
        </div>
    }
}

#[component]
fn InstancesTable(
    instances: Vec<InstanceInfo>,
    on_delete: impl Fn(String) + Clone + Send + Sync + 'static,
    on_execute: impl Fn(String) + Clone + Send + Sync + 'static,
    executing_id: RwSignal<Option<String>>,
    i18n: I18nContext,
) -> impl IntoView {
    let i18n_stored = StoredValue::new(i18n);
    view! {
        <div class="instances-table-wrapper">
            <table class="instances-table">
                <thead>
                    <tr>
                        <th>{move || i18n_stored.get_value().t("instances-col-id")}</th>
                        <th>{move || i18n_stored.get_value().t("instances-col-skill")}</th>
                        <th>{move || i18n_stored.get_value().t("instances-col-agent")}</th>
                        <th>{move || i18n_stored.get_value().t("instances-col-status")}</th>
                        <th>{move || i18n_stored.get_value().t("instances-col-usage")}</th>
                        <th>{move || i18n_stored.get_value().t("instances-col-actions")}</th>
                    </tr>
                </thead>
                <tbody>
                    {instances.into_iter().map(|instance| {
                        let status_class = format!("status-badge status-{}", instance.status.to_lowercase());
                        let is_exec = {
                            let id = instance.instance_id.clone();
                            let executing_id = executing_id.clone();
                            move || executing_id.get().as_ref() == Some(&id)
                        };
                        let is_exec2 = is_exec.clone();
                        view! {
                            <tr>
                                <td class="mono">{instance.instance_id.clone()}</td>
                                <td>{instance.skill_id.clone()}</td>
                                <td>{instance.agent_id.clone()}</td>
                                <td><span class=status_class.clone()>{instance.status.clone()}</span></td>
                                <td>
                                    {format!(
                                        "{} {} · {}ms {}",
                                        instance.usage.total_calls,
                                        i18n_stored.get_value().t("instances-calls"),
                                        instance.usage.avg_latency_ms as u64,
                                        i18n_stored.get_value().t("instances-avg")
                                    )}
                                </td>
                                <td class="actions">
                                    <button
                                        class="btn btn-sm btn-primary"
                                        disabled=is_exec
                                        on:click={
                                            let id = instance.instance_id.clone();
                                            let on_execute = on_execute.clone();
                                            move |_| on_execute(id.clone())
                                        }
                                    >
                                        {move || if is_exec2() { i18n_stored.get_value().t("instances-running") } else { i18n_stored.get_value().t("instances-run") }}
                                    </button>
                                    <button
                                        class="btn btn-sm btn-danger"
                                        on:click={
                                            let id = instance.instance_id.clone();
                                            let on_delete = on_delete.clone();
                                            move |_| on_delete(id.clone())
                                        }
                                    >
                                        {move || i18n_stored.get_value().t("instances-delete")}
                                    </button>
                                </td>
                            </tr>
                        }
                    }).collect::<Vec<_>>()}
                </tbody>
            </table>
        </div>
    }
}

#[component]
fn InstancesLoading() -> impl IntoView {
    view! {
        <div class="instances-table-wrapper">
            <div class="skeleton-table">
                <div class="skeleton-row"></div>
                <div class="skeleton-row"></div>
                <div class="skeleton-row"></div>
            </div>
        </div>
    }
}

#[component]
fn InstancesEmpty(i18n: I18nContext) -> impl IntoView {
    let i18n_stored = StoredValue::new(i18n);
    view! {
        <div class="empty-state">
            <div class="empty-icon">"🤖"</div>
            <h3>{move || i18n_stored.get_value().t("instances-empty-title")}</h3>
            <p>{move || i18n_stored.get_value().t("instances-empty-desc")}</p>
        </div>
    }
}

#[component]
fn InstancesError(#[prop(into)] message: String, i18n: I18nContext) -> impl IntoView {
    let i18n_stored = StoredValue::new(i18n);
    view! {
        <div class="error-state">
            <div class="error-icon">"⚠️"</div>
            <h3>{move || i18n_stored.get_value().t("instances-error-title")}</h3>
            <p>{message}</p>
        </div>
    }
}
