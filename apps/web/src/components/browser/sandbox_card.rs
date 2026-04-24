//! 沙箱卡片组件

use crate::browser::sandbox::{BrowserSandbox, SandboxStatus};
use crate::i18n::I18nContext;
use leptos::prelude::*;

/// 沙箱卡片组件
#[component]
pub fn SandboxCard(
    sandbox: BrowserSandbox,
    #[prop(optional)] on_start: Option<Box<dyn Fn()>>,
    #[prop(optional)] on_stop: Option<Box<dyn Fn()>>,
    #[prop(optional)] on_delete: Option<Box<dyn Fn()>>,
) -> impl IntoView {
    let i18n = use_context::<I18nContext>().expect("i18n context not found");
    let i18n_stored = StoredValue::new(i18n);

    let is_running = matches!(sandbox.status, SandboxStatus::Running);

    view! {
        <div
            class="sandbox-card"
            style:border-left-color=sandbox.color.clone()
        >
            <div class="sandbox-header">
                <h4 class="sandbox-name">{sandbox.name.clone()}</h4>
                <span class={format!("sandbox-status {}", status_class(&sandbox.status))}>
                    {format!("{:?}", sandbox.status)}
                </span>
            </div>

            <div class="sandbox-details">
                <div class="detail-row">
                    <span class="detail-label">{move || i18n_stored.get_value().t("sandbox-cdp-port")}</span>
                    <span class="detail-value">{sandbox.cdp_port}</span>
                </div>
                <div class="detail-row">
                    <span class="detail-label">{move || i18n_stored.get_value().t("sandbox-isolation")}</span>
                    <span class="detail-value">{format!("{:?}", sandbox.isolation)}</span>
                </div>
                <div class="detail-row">
                    <span class="detail-label">{move || i18n_stored.get_value().t("sandbox-memory")}</span>
                    <span class="detail-value">{format!("{} MB", sandbox.resource_limits.memory_limit_mb)}</span>
                </div>
            </div>

            <div class="sandbox-actions">
                {if is_running {
                    view! {
                        <button
                            class="btn btn-warning btn-sm"
                            on:click=move |_| {
                                if let Some(ref cb) = on_stop {
                                    cb();
                                }
                            }
                        >
                            {move || i18n_stored.get_value().t("sandbox-stop")}
                        </button>
                    }.into_any()
                } else {
                    view! {
                        <button
                            class="btn btn-success btn-sm"
                            on:click=move |_| {
                                if let Some(ref cb) = on_start {
                                    cb();
                                }
                            }
                        >
                            {move || i18n_stored.get_value().t("sandbox-start")}
                        </button>
                    }.into_any()
                }}

                <button
                    class="btn btn-danger btn-sm btn-icon"
                    on:click=move |_| {
                        if let Some(ref cb) = on_delete {
                            cb();
                        }
                    }
                >
                    "🗑"
                </button>
            </div>
        </div>
    }
}

fn status_class(status: &SandboxStatus) -> &'static str {
    match status {
        SandboxStatus::Running => "running",
        SandboxStatus::Creating => "creating",
        SandboxStatus::Paused => "paused",
        SandboxStatus::Stopped => "stopped",
        SandboxStatus::Cleaning => "cleaning",
        SandboxStatus::Error(_) => "error",
    }
}
