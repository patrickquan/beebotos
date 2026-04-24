//! 用量面板组件

use crate::i18n::I18nContext;
use crate::webchat::{TokenUsage, UsagePanel};
use leptos::prelude::*;

/// 用量面板属性
pub struct UsagePanelProps {
    pub usage: UsagePanel,
    pub is_open: bool,
    pub on_close: Option<std::rc::Rc<dyn Fn()>>,
}

/// 用量面板组件
#[component]
pub fn UsagePanelComponent(
    usage: UsagePanel,
    #[prop(optional)] is_open: Option<bool>,
    #[prop(optional)] on_close: Option<Box<dyn Fn()>>,
) -> impl IntoView {
    let i18n = use_context::<I18nContext>().expect("i18n context not found");
    let i18n_stored = StoredValue::new(i18n);

    let is_open = is_open.unwrap_or(true);

    view! {
        {if !is_open {
            view! { <div /> }.into_any()
        } else {
            view! { <div class="usage-panel">
            <div class="usage-panel-header">
                <h4>{move || i18n_stored.get_value().t("usage-panel-title")}</h4>
                <button
                    class="btn btn-icon"
                    on:click=move |_| {
                        if let Some(ref cb) = on_close {
                            cb();
                        }
                    }
                >
                    "✕"
                </button>
            </div>

            <div class="usage-stats">
                <UsageStatItem
                    label=i18n_stored.get_value().t("usage-panel-session")
                    usage=usage.session_usage.clone()
                />
                <UsageStatItem
                    label=i18n_stored.get_value().t("usage-panel-daily")
                    usage=usage.daily_usage.clone()
                />
                <UsageStatItem
                    label=i18n_stored.get_value().t("usage-panel-monthly")
                    usage=usage.monthly_usage.clone()
                />
            </div>

            {if usage.limit_status.has_limit {
                view! {
                    <div class="limit-status">
                        <h5>{move || i18n_stored.get_value().t("usage-panel-limits")}</h5>
                        {if let Some(remaining) = usage.limit_status.daily_remaining {
                            view! {
                                <div class="limit-item">
                                    <span>{move || i18n_stored.get_value().t("usage-panel-daily-remaining")}</span>
                                    <span>{remaining.to_string()}</span>
                                </div>
                            }.into_any()
                        } else {
                            view! { <div /> }.into_any()
                        }}
                        {if let Some(remaining) = usage.limit_status.monthly_remaining {
                            view! {
                                <div class="limit-item">
                                    <span>{move || i18n_stored.get_value().t("usage-panel-monthly-remaining")}</span>
                                    <span>{remaining.to_string()}</span>
                                </div>
                            }.into_any()
                        } else {
                            view! { <div /> }.into_any()
                        }}
                        {if usage.limit_status.is_near_limit {
                            view! {
                                <div class="limit-warning">
                                    {move || i18n_stored.get_value().t("usage-panel-approaching-limit")}
                                </div>
                            }.into_any()
                        } else {
                            view! { <div /> }.into_any()
                        }}
                    </div>
                }.into_any()
            } else {
                view! { <div /> }.into_any()
            }}
        </div> }.into_any()
        }}
    }
}

/// 用量统计项组件
#[component]
fn UsageStatItem(label: String, usage: TokenUsage) -> impl IntoView {
    let i18n = use_context::<I18nContext>().expect("i18n context not found");
    let i18n_stored = StoredValue::new(i18n);

    view! {
        <div class="usage-stat-item">
            <div class="stat-header">
                <span class="stat-label">{label}</span>
                <span class="stat-model">{usage.model.clone()}</span>
            </div>
            <div class="stat-value">{usage.format()}</div>
            <div class="stat-details">
                <span>{move || i18n_stored.get_value().t("usage-panel-prompt").replace("{}", &usage.prompt_tokens.to_string())}</span>
                <span>{move || i18n_stored.get_value().t("usage-panel-completion").replace("{}", &usage.completion_tokens.to_string())}</span>
            </div>
        </div>
    }
}

/// 小型用量指示器组件
#[component]
pub fn UsageIndicator(usage: TokenUsage) -> impl IntoView {
    view! {
        <div class="usage-indicator">
            <span class="token-icon">"🪙"</span>
            <span class="token-count">{usage.total_tokens.to_string()}</span>
            <span class="token-cost">{format!("${:.4}", usage.estimated_cost)}</span>
        </div>
    }
}
