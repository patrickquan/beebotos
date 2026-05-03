//! LLM Provider Management Page (QwenPaw-style)
//!
//! 暗黑主题供应商卡片网格，支持 hover 操作、三色状态、标签体系。

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::llm_provider_service::{
    LlmProvider, SetActiveLlmRequest,
};
use crate::components::modal::Modal;
use crate::pages::llm_provider_modals::{AddProviderModal, ModelManageModal, ProviderConfigModal};
use crate::state::use_app_state;
use crate::utils::event_target_value;

/// 计算供应商状态
/// - 可用（青色）：已启用，已配置（有 Base URL 且满足 API Key 要求），且至少有一个模型
/// - 部分（橙色）：已启用，已配置，但无模型
/// - 不可用（灰色）：未启用或未配置
fn provider_status(provider: &LlmProvider) -> (&'static str, &'static str, &'static str, &'static str) {
    let has_config = provider.base_url.is_some()
        && !provider.base_url.as_ref().unwrap().is_empty();
    let has_key = provider.api_key_masked.is_some();
    let all_models = provider.all_models();
    let has_models = !all_models.is_empty();
    let key_ok = !provider.require_api_key || has_key;

    if !provider.enabled {
        return ("offline", "disabled", "不可用", "已禁用");
    }
    if !has_config {
        return ("offline", "disabled", "不可用", "未配置 Base URL");
    }
    if !key_ok {
        return ("offline", "disabled", "不可用", "未配置 API Key");
    }
    if !has_models {
        return ("partial", "partial", "部分可用", "无模型");
    }
    ("online", "enabled", "可用", "正常")
}

/// 获取供应商标签列表
fn provider_tags(provider: &LlmProvider) -> Vec<(&'static str, &'static str)> {
    let mut tags = Vec::new();
    if provider.is_custom {
        tags.push(("自定义", "custom"));
    } else {
        tags.push(("内置", "builtin"));
    }
    tags
}

#[component]
pub fn LlmProvidersPage() -> impl IntoView {
    let app_state = StoredValue::new(use_app_state());

    let providers: RwSignal<Option<Vec<LlmProvider>>> = RwSignal::new(None);
    let is_loading = RwSignal::new(false);
    let error_msg: RwSignal<Option<String>> = RwSignal::new(None);
    let search_query = RwSignal::new(String::new());
    let save_success: RwSignal<Option<String>> = RwSignal::new(None);

    // Modal states
    let selected_provider: RwSignal<Option<LlmProvider>> = RwSignal::new(None);
    let show_config_modal = RwSignal::new(false);
    let show_model_modal = RwSignal::new(false);
    let show_add_modal = RwSignal::new(false);
    let show_delete_confirm: RwSignal<Option<LlmProvider>> = RwSignal::new(None);

    // Active LLM selection
    let active_provider_id: RwSignal<Option<i64>> = RwSignal::new(None);
    let active_model_name: RwSignal<Option<String>> = RwSignal::new(None);
    let saved_provider_id: RwSignal<Option<i64>> = RwSignal::new(None);
    let saved_model_name: RwSignal<Option<String>> = RwSignal::new(None);

    let fetch_providers = move || {
        is_loading.set(true);
        error_msg.set(None);
        save_success.set(None);
        let service = app_state.get_value().llm_provider_service();
        spawn_local(async move {
            match service.list_providers().await {
                Ok(resp) => {
                    // Try to get active LLM
                    if let Ok(active) = service.get_active_llm().await {
                        active_provider_id.set(Some(active.provider_id));
                        active_model_name.set(Some(active.model_name.clone()));
                        saved_provider_id.set(Some(active.provider_id));
                        saved_model_name.set(Some(active.model_name));
                    } else {
                        // Fallback: find default provider
                        for p in &resp.providers {
                            if p.is_default_provider {
                                active_provider_id.set(Some(p.id));
                                if let Some(m) = p.default_model() {
                                    active_model_name.set(Some(m.name.clone()));
                                }
                                break;
                            }
                        }
                    }
                    providers.set(Some(resp.providers));
                    is_loading.set(false);
                }
                Err(e) => {
                    error_msg.set(Some(format!("加载失败: {}", e)));
                    is_loading.set(false);
                }
            }
        });
    };

    let refresh = StoredValue::new(fetch_providers);

    Effect::new(move |_| {
        refresh.get_value()();
    });

    // Filtered providers based on search
    let filtered_providers = move || {
        let query = search_query.get().to_lowercase();
        providers.get().map(|list| {
            if query.is_empty() {
                list
            } else {
                list.into_iter()
                    .filter(|p| {
                        p.name.to_lowercase().contains(&query)
                            || p.provider_id.to_lowercase().contains(&query)
                            || p.base_url
                                .as_ref()
                                .map(|u| u.to_lowercase().contains(&query))
                                .unwrap_or(false)
                    })
                    .collect()
            }
        })
    };

    // Get available providers (enabled + configured + authorized) for active LLM dropdown
    let available_providers = move || {
        providers.get().map(|list| {
            list.into_iter()
                .filter(|p| {
                    let has_config = p.base_url.is_some()
                        && !p.base_url.as_ref().unwrap().is_empty();
                    let has_key = p.api_key_masked.is_some();
                    let key_ok = !p.require_api_key || has_key;
                    p.enabled && has_config && key_ok
                })
                .collect::<Vec<_>>()
        })
    };

    // Save active LLM
    let on_save_active = move || {
        if let (Some(pid), Some(mname)) = (active_provider_id.get(), active_model_name.get()) {
            let service = app_state.get_value().llm_provider_service();
            spawn_local(async move {
                let req = SetActiveLlmRequest {
                    provider_id: pid,
                    model_name: mname.clone(),
                };
                match service.set_active_llm(req).await {
                    Ok(_) => {
                        save_success.set(Some("默认 LLM 已保存".to_string()));
                        saved_provider_id.set(Some(pid));
                        saved_model_name.set(Some(mname));
                        // Refresh after a short delay
                        gloo_timers::future::TimeoutFuture::new(500).await;
                        refresh.get_value()();
                    }
                    Err(e) => {
                        save_success.set(Some(format!("保存失败: {}", e)));
                    }
                }
            });
        }
    };

    let on_delete_provider = move |provider: LlmProvider| {
        let service = app_state.get_value().llm_provider_service();
        spawn_local(async move {
            let _ = service.delete_provider(provider.id).await;
            show_delete_confirm.set(None);
            refresh.get_value()();
        });
    };

    // Remote provider card renderer
    let render_remote_card = move |provider: LlmProvider| {
        let icon = provider.icon.clone().unwrap_or_else(|| "🔧".to_string());
        let color = provider.icon_color.clone().unwrap_or_else(|| "#64748b".to_string());
        let provider_for_config = provider.clone();
        let provider_for_model = provider.clone();
        let provider_for_delete = provider.clone();
        let (dot_class, desc_class, status_label, _status_desc) = provider_status(&provider);
        let tags = provider_tags(&provider);
        let all_models = provider.all_models();
        let model_count = all_models.len();

        view! {
            <div class="provider-card" class:custom=provider.is_custom>
                // Card Header: Icon + Status
                <div class="provider-card-header">
                    <div
                        class="provider-avatar"
                        style=format!("background: {}; color: white;", color)
                    >
                        {icon}
                    </div>
                    <div class="card-status-header">
                        <span class={format!("status-dot {}", dot_class)}></span>
                        <span class={format!("status-desc {}", desc_class)}>{status_label}</span>
                    </div>
                </div>

                // Title Row: Name + Tags
                <div class="provider-name-row">
                    <h4>{provider.name.clone()}</h4>
                    {tags.into_iter().map(|(label, kind)| view! {
                        <span class={format!("provider-tag {}", kind)}>{label}</span>
                    }).collect_view()}
                </div>

                // Info Section
                <div class="provider-card-body">
                    <div class="provider-detail">
                        <span class="detail-label">"Base URL"</span>
                        <span class="detail-value url-value">
                            {provider.base_url.clone().unwrap_or_else(|| "未配置".to_string())}
                        </span>
                    </div>
                    <div class="provider-detail">
                        <span class="detail-label">"API Key"</span>
                        <span class="detail-value">
                            {if provider.api_key_masked.is_some() {
                                view! { <span class="status-set">"已设置"</span> }.into_any()
                            } else {
                                view! { <span class="status-unset">"未设置"</span> }.into_any()
                            }}
                        </span>
                    </div>
                    <div class="provider-detail">
                        <span class="detail-label">"Model"</span>
                        <span class="detail-value">
                            {if model_count == 0 {
                                "暂无模型".to_string()
                            } else {
                                format!("{} 个模型", model_count)
                            }}
                        </span>
                    </div>
                </div>

                // Actions - hover only
                <div class="provider-card-footer">
                    <button
                        class="btn btn-sm btn-secondary"
                        on:click=move |_| {
                            selected_provider.set(Some(provider_for_model.clone()));
                            show_model_modal.set(true);
                        }
                    >
                        "模型"
                    </button>
                    <button
                        class="btn btn-sm btn-secondary"
                        on:click=move |_| {
                            selected_provider.set(Some(provider_for_config.clone()));
                            show_config_modal.set(true);
                        }
                    >
                        "设置"
                    </button>
                    {if provider.is_custom {
                        view! {
                            <button
                                class="btn btn-sm btn-text danger"
                                on:click=move |_| {
                                    show_delete_confirm.set(Some(provider_for_delete.clone()));
                                }
                            >
                                "删除"
                            </button>
                        }.into_any()
                    } else {
                        view! { <span></span> }.into_any()
                    }}
                </div>
            </div>
        }
    };

    view! {
        <div class="page llm-providers-page">
            // Breadcrumb
            <div class="breadcrumb">
                <span>"设置"</span>
                <span class="breadcrumb-separator">"/"</span>
                <span class="breadcrumb-current">"模型"</span>
            </div>

            <h1 class="page-title">"模型"</h1>

            // Active LLM Section (ModelsSection style)
            <div class="default-llm-section">
                <div class="slot-header">
                    <span class="slot-title">"默认 LLM"</span>
                    {move || {
                        let pid = saved_provider_id.get();
                        let mname = saved_model_name.get();
                        if pid.is_some() && mname.is_some() {
                            let providers_list = providers.get().unwrap_or_default();
                            let provider_name = providers_list.iter()
                                .find(|p| Some(p.id) == pid)
                                .map(|p| p.name.clone())
                                .unwrap_or_default();
                            let badge_text = format!("{} / {}", provider_name, mname.unwrap_or_default());
                            view! {
                                <span class="current-model-badge">{badge_text}</span>
                            }.into_any()
                        } else {
                            view! { <span></span> }.into_any()
                        }
                    }}
                </div>
                <div class="default-llm-form">
                    <div class="form-row">
                        <div class="form-group">
                            <label>"提供商"</label>
                            <select
                                prop:value=move || active_provider_id.get().map(|id| id.to_string()).unwrap_or_default()
                                on:change=move |ev| {
                                    let val = event_target_value(&ev);
                                    if let Ok(id) = val.parse::<i64>() {
                                        active_provider_id.set(Some(id));
                                        // Update model selection
                                        if let Some(list) = providers.get() {
                                            if let Some(p) = list.iter().find(|p| p.id == id) {
                                                if let Some(m) = p.default_model() {
                                                    active_model_name.set(Some(m.name.clone()));
                                                } else if let Some(m) = p.all_models().first() {
                                                    active_model_name.set(Some(m.name.clone()));
                                                } else {
                                                    active_model_name.set(None);
                                                }
                                            }
                                        }
                                    }
                                }
                            >
                                <option value="">"选择提供商 (必须已授权)"</option>
                                {move || available_providers().unwrap_or_default().into_iter().map(|p| {
                                    let selected = active_provider_id.get() == Some(p.id);
                                    view! {
                                        <option value={p.id.to_string()} selected={selected}>
                                            {p.name.clone()}
                                        </option>
                                    }
                                }).collect_view()}
                            </select>
                        </div>
                        <div class="form-group">
                            <label>"模型"</label>
                            <select
                                prop:value=move || active_model_name.get().unwrap_or_default()
                                on:change=move |ev| {
                                    let val = event_target_value(&ev);
                                    if !val.is_empty() {
                                        active_model_name.set(Some(val));
                                    }
                                }
                            >
                                <option value="">"请先添加模型"</option>
                                {move || {
                                    let pid = active_provider_id.get();
                                    let list = providers.get().unwrap_or_default();
                                    let models = pid.and_then(|id| {
                                        list.iter().find(|p| p.id == id).map(|p| p.all_models().into_iter().cloned().collect::<Vec<_>>())
                                    }).unwrap_or_default();
                                    models.into_iter().map(|m| {
                                        let selected = active_model_name.get().as_ref() == Some(&m.name);
                                        view! {
                                            <option value={m.name.clone()} selected={selected}>
                                                {m.display_name.clone().unwrap_or_else(|| m.name.clone())}
                                            </option>
                                        }
                                    }).collect_view()
                                }}
                            </select>
                        </div>
                    </div>
                    {move || save_success.get().map(|msg| view! {
                        <p class="form-hint success-hint">{msg}</p>
                    })}
                    <p class="form-hint">
                        "在这里设置全局默认的 LLM 模型。你也可以在聊天页面为具体 Agent 单独选择使用的模型。"
                    </p>
                </div>
                <div class="slot-actions">
                    {move || {
                        let current_pid = active_provider_id.get();
                        let current_mname = active_model_name.get();
                        let saved_pid = saved_provider_id.get();
                        let saved_mname = saved_model_name.get();
                        let is_saved = current_pid.is_some()
                            && current_mname.is_some()
                            && saved_pid == current_pid
                            && saved_mname == current_mname;
                        view! {
                            <button
                                class="btn btn-primary save-default-btn"
                                on:click=move |_| on_save_active()
                                disabled=move || active_provider_id.get().is_none() || active_model_name.get().is_none() || is_saved
                            >
                                {if is_saved { "已保存" } else { "保存" }}
                            </button>
                        }
                    }}
                </div>
            </div>

            // Providers Section
            <div class="providers-section">
                <div class="section-header">
                    <h3>"提供商"</h3>
                    <div class="header-right">
                        <div class="search-row">
                            <div class="search-box">
                                <span class="search-icon">"🔍"</span>
                                <input
                                    type="text"
                                    placeholder="搜索提供商..."
                                    prop:value=search_query.get()
                                    on:input=move |ev| search_query.set(event_target_value(&ev))
                                />
                            </div>
                            <button
                                class="btn btn-icon"
                                on:click=move |_| refresh.get_value()()
                                title="刷新"
                            >
                                "🔄"
                            </button>
                        </div>
                        <button
                            class="btn btn-primary add-provider-btn"
                            on:click=move |_| show_add_modal.set(true)
                        >
                            <span>"+"</span>
                            "添加提供商"
                        </button>
                    </div>
                </div>

                {move || {
                    if is_loading.get() {
                        view! { <div class="loading-state">"加载中..."</div> }.into_any()
                    } else if let Some(error) = error_msg.get() {
                        view! {
                            <div class="error-state">
                                <div class="error-icon">"⚠️"</div>
                                <p>{error}</p>
                                <button class="btn btn-primary" on:click=move |_| refresh.get_value()()>
                                    "重试"
                                </button>
                            </div>
                        }.into_any()
                    } else if let Some(list) = filtered_providers() {
                        if list.is_empty() {
                            if search_query.get().is_empty() {
                                view! {
                                    <div class="empty-state">
                                        <div class="empty-icon">"📭"</div>
                                        <p>"暂无提供商"</p>
                                        <button
                                            class="btn btn-primary"
                                            on:click=move |_| show_add_modal.set(true)
                                        >
                                            "添加提供商"
                                        </button>
                                    </div>
                                }.into_any()
                            } else {
                                view! {
                                    <div class="empty-state">
                                        <p>"未找到匹配的提供商"</p>
                                    </div>
                                }.into_any()
                            }
                        } else {
                            view! {
                                <div class="providers-content">
                                    <div class="provider-group">
                                        <div class="providers-grid">
                                            {list.into_iter().map(|p| render_remote_card(p)).collect_view()}
                                        </div>
                                    </div>
                                </div>
                            }.into_any()
                        }
                    } else {
                        view! { <div>"暂无数据"</div> }.into_any()
                    }
                }}
            </div>

            // Modals
            <Show when=move || show_config_modal.get()>
                {move || selected_provider.get().map(|p| view! {
                    <ProviderConfigModal
                        provider=p
                        on_close=move || show_config_modal.set(false)
                        on_updated=move || refresh.get_value()()
                    />
                })}
            </Show>

            <Show when=move || show_model_modal.get()>
                {move || selected_provider.get().map(|p| view! {
                    <ModelManageModal
                        provider=p
                        on_close=move || show_model_modal.set(false)
                        on_updated=move || refresh.get_value()()
                    />
                })}
            </Show>

            <Show when=move || show_add_modal.get()>
                <AddProviderModal
                    on_close=move || show_add_modal.set(false)
                    on_created=move || refresh.get_value()()
                />
            </Show>

            // Delete confirmation modal
            <Show when=move || show_delete_confirm.get().is_some()>
                {move || show_delete_confirm.get().map(|p| view! {
                    <Modal title="确认删除" on_close=move || show_delete_confirm.set(None)>
                        <div class="modal-body">
                            <p>
                                "确定要删除供应商 "
                                <strong>{p.name.clone()}</strong>
                                " 吗？此操作不可撤销，关联的模型也将被删除。"
                            </p>
                        </div>
                        <div class="modal-footer">
                            <button class="btn btn-secondary" on:click=move |_| show_delete_confirm.set(None)>
                                "取消"
                            </button>
                            <button
                                class="btn btn-danger"
                                on:click=move |_| on_delete_provider(p.clone())
                            >
                                "删除"
                            </button>
                        </div>
                    </Modal>
                })}
            </Show>
        </div>
    }
}
