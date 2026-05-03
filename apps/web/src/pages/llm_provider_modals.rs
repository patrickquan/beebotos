//! LLM Provider Management Modals (QwenPaw-style)
//!
//! 暗黑主题弹窗：配置、模型管理、添加供应商。

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::llm_provider_service::{
    AddModelRequest, CreateProviderRequest, LlmModel, LlmProvider, UpdateModelConfigRequest,
    UpdateProviderConfigRequest, UpdateProviderRequest,
};
use crate::components::modal::Modal;
use crate::state::use_app_state;
use crate::utils::{event_target_checked, event_target_value};

// ============================================
// Provider Configuration Modal
// ============================================
#[component]
pub fn ProviderConfigModal(
    provider: LlmProvider,
    #[prop(into)] on_close: Callback<()>,
    #[prop(into)] on_updated: Callback<()>,
) -> impl IntoView {
    let app_state = StoredValue::new(use_app_state());

    let base_url = RwSignal::new(provider.base_url.clone().unwrap_or_default());
    let api_key = RwSignal::new(String::new());
    let show_api_key = RwSignal::new(false);
    let enabled = RwSignal::new(provider.enabled);
    let advanced_open = RwSignal::new(false);
    let generate_kwargs = RwSignal::new(
        provider
            .generate_kwargs
            .as_ref()
            .map(|v| v.to_string())
            .unwrap_or_else(|| "{\n  \"max_tokens\": null\n}".to_string()),
    );
    let saving = RwSignal::new(false);
    let testing = RwSignal::new(false);
    let error_msg: RwSignal<Option<String>> = RwSignal::new(None);
    let test_result: RwSignal<Option<(String, bool)>> = RwSignal::new(None);

    // 根据供应商类型显示占位提示
    let base_url_placeholder = move || {
        match provider.provider_id.as_str() {
            "openai" => "https://api.openai.com/v1",
            "anthropic" => "https://api.anthropic.com/v1",
            "kimi-cn" | "kimi-intl" => "https://api.moonshot.cn/v1",
            "deepseek" => "https://api.deepseek.com/v1",
            "zhipu-cn" => "https://open.bigmodel.cn/api/paas/v4",
            "zhipu-intl" => "https://api.z.ai/api/paas/v4",
            "ollama" => "http://localhost:11434",
            _ => "https://api.example.com/v1",
        }
        .to_string()
    };

    let on_save = move |_| {
        saving.set(true);
        error_msg.set(None);
        let service = app_state.get_value().llm_provider_service();
        let id = provider.id;
        let req = UpdateProviderRequest {
            name: None,
            base_url: Some(base_url.get()).filter(|s| !s.is_empty()),
            api_key: Some(api_key.get()).filter(|s| !s.is_empty()),
            enabled: Some(enabled.get()),
        };
        spawn_local(async move {
            match service.update_provider(id, req).await {
                Ok(_) => {
                    // 如果 advanced_open，也保存 generate_kwargs
                    if advanced_open.get() {
                        let gk_str = generate_kwargs.get();
                        let gk_value = serde_json::from_str(&gk_str).ok();
                        let _ = service
                            .update_provider_config(
                                id,
                                UpdateProviderConfigRequest {
                                    generate_kwargs: gk_value,
                                },
                            )
                            .await;
                    }
                    saving.set(false);
                    on_updated.run(());
                    on_close.run(());
                }
                Err(e) => {
                    saving.set(false);
                    error_msg.set(Some(format!("保存失败: {}", e)));
                }
            }
        });
    };

    let on_test = move |_| {
        testing.set(true);
        test_result.set(None);
        let service = app_state.get_value().llm_provider_service();
        let id = provider.id;
        spawn_local(async move {
            match service.test_provider_connection(id).await {
                Ok(resp) => {
                    testing.set(false);
                    test_result.set(Some((resp.message, resp.success)));
                }
                Err(e) => {
                    testing.set(false);
                    test_result.set(Some((format!("测试失败: {}", e), false)));
                }
            }
        });
    };

    let on_revoke = move |_: ()| {
        // 清空 API Key：发送空字符串表示清除
        saving.set(true);
        let service = app_state.get_value().llm_provider_service();
        let id = provider.id;
        spawn_local(async move {
            let req = UpdateProviderRequest {
                name: None,
                base_url: None,
                api_key: Some(String::new()),
                enabled: None,
            };
            let _ = service.update_provider(id, req).await;
            saving.set(false);
            on_updated.run(());
        });
    };

    let input_type = move || {
        if show_api_key.get() {
            "text"
        } else {
            "password"
        }
    };

    view! {
        <Modal title=format!("配置 {}", provider.name) on_close=move |_| on_close.run(()) class="modal-wide">
            <div class="modal-body llm-config-modal-body">
                {move || error_msg.get().map(|e| view! {
                    <div class="error-message">{e}</div>
                })}
                {move || test_result.get().map(|(msg, success)| view! {
                    <div class={if success { "success-message" } else { "error-message" }}>{msg}</div>
                })}

                <div class="form-group required">
                    <label>"基础 URL"</label>
                    <input
                        type="text"
                        placeholder={base_url_placeholder}
                        prop:value=base_url.get()
                        disabled=provider.freeze_url
                        on:input=move |ev| base_url.set(event_target_value(&ev))
                    />
                    {if provider.freeze_url {
                        view! {
                            <p class="field-hint">"内置供应商，Base URL 不可修改。"</p>
                        }.into_any()
                    } else {
                        view! { <span></span> }.into_any()
                    }}
                </div>

                <div class="form-group">
                    <label>"API 密钥"</label>
                    <div class="input-with-suffix">
                        <input
                            type={input_type}
                            placeholder="输入 API 密钥（可选）"
                            prop:value=api_key.get()
                            on:input=move |ev| api_key.set(event_target_value(&ev))
                        />
                        <button
                            class="input-suffix-btn"
                            on:click=move |_| show_api_key.update(|v| *v = !*v)
                            title=move || if show_api_key.get() { "隐藏" } else { "显示" }
                        >
                            {move || if show_api_key.get() { "🙈" } else { "👁️" }}
                        </button>
                    </div>
                    <p class="field-hint">
                        {if provider.api_key_masked.is_some() {
                            "已配置 API 密钥。留空则保留现有密钥，输入新值则覆盖。"
                        } else {
                            "尚未配置 API 密钥。"
                        }}
                    </p>
                </div>

                <div class="form-group">
                    <label>"启用"</label>
                    <label class="checkbox-label">
                        <input
                            type="checkbox"
                            prop:checked=enabled.get()
                            on:change=move |ev| enabled.set(event_target_checked(&ev))
                        />
                        "启用此供应商"
                    </label>
                </div>

                // Advanced Config
                <div class="advanced-section">
                    <button
                        class="advanced-toggle"
                        on:click=move |_| advanced_open.update(|v| *v = !*v)
                    >
                        <span class="toggle-icon">
                            {move || if advanced_open.get() { "▼" } else { "▶" }}
                        </span>
                        "进阶配置"
                    </button>
                    <Show when=move || advanced_open.get()>
                        <div class="advanced-content">
                            <div class="form-group">
                                <label>"生成参数配置"</label>
                                <textarea
                                    class="code-textarea"
                                    rows="6"
                                    prop:value=generate_kwargs.get()
                                    on:input=move |ev| generate_kwargs.set(event_target_value(&ev))
                                />
                                <p class="field-hint">
                                    "使用 JSON 格式表示的生成参数配置项，会被展开传入到生成请求（"
                                    <code>"openai.chat.completions"</code>
                                    " 或 "
                                    <code>"anthropic.messages"</code>
                                    "）中。"
                                </p>
                            </div>
                        </div>
                    </Show>
                </div>
            </div>
            <div class="modal-footer llm-config-footer">
                {if provider.support_connection_check {
                    view! {
                        <button
                            class="btn btn-test"
                            on:click=on_test
                            disabled=move || testing.get()
                        >
                            <span>"🔗"</span>
                            {if testing.get() { "测试中..." } else { "测试连接" }}
                        </button>
                    }.into_any()
                } else {
                    view! { <span></span> }.into_any()
                }}
                <div class="footer-spacer" />
                {if provider.api_key_masked.is_some() {
                    view! {
                        <button
                            class="btn btn-text danger"
                            on:click=move |_| on_revoke(())
                            disabled=move || saving.get()
                        >
                            "撤销授权"
                        </button>
                    }.into_any()
                } else {
                    view! { <span></span> }.into_any()
                }}
                <button class="btn btn-secondary" on:click=move |_| on_close.run(())>
                    "取消"
                </button>
                <button
                    class="btn btn-primary"
                    on:click=on_save
                    disabled=move || saving.get() || base_url.get().trim().is_empty()
                >
                    {if saving.get() { "保存中..." } else { "保存" }}
                </button>
            </div>
        </Modal>
    }
}

// ============================================
// Model Item Component (extracted to avoid closure capture issues)
// ============================================
#[component]
fn ModelItem(
    model: LlmModel,
    is_builtin: bool,
    editing_model: RwSignal<Option<i64>>,
    edit_display_name: RwSignal<String>,
    testing_model: RwSignal<Option<i64>>,
    probing_model: RwSignal<Option<i64>>,
    #[prop(into)] on_test: Callback<i64>,
    #[prop(into)] on_set_default: Callback<i64>,
    #[prop(into)] on_probe: Callback<i64>,
    #[prop(into)] on_delete: Callback<i64>,
    #[prop(into)] on_save_edit: Callback<i64>,
) -> impl IntoView {
    let model_id = model.id;
    let is_default = model.is_default_model;
    let model_name = model.name.clone();
    let model_display_name = model.display_name.clone();
    let model_display_name_store = StoredValue::new(model_display_name.clone());

    let is_editing = move || editing_model.get() == Some(model_id);

    let mut tags = Vec::new();
    if model.supports_image == Some(true) || model.supports_multimodal == Some(true) {
        tags.push(("图片", "image"));
    }
    if model.supports_video == Some(true) {
        tags.push(("视频", "video"));
    }
    if tags.is_empty() {
        if model.supports_image.is_none()
            && model.supports_video.is_none()
            && model.supports_multimodal.is_none()
        {
            tags.push(("未探测", "unknown"));
        } else {
            tags.push(("纯文本", "text"));
        }
    }

    view! {
        <div class="model-item" class:default=is_default>
            <div class="model-info">
                <span class="model-name">{model_name.clone()}</span>
                {model_display_name.clone().map(|d| view! {
                    <span class="model-display">{"("}{d}{")"}</span>
                })}
                <div class="model-tags">
                    {if is_default {
                        view! { <span class="model-tag default">"默认"</span> }.into_any()
                    } else {
                        view! { <span></span> }.into_any()
                    }}
                    <span class={if is_builtin { "model-tag builtin" } else { "model-tag user" }}>
                        {if is_builtin { "内置" } else { "用户添加" }}
                    </span>
                    {tags.into_iter().map(|(label, kind)| view! {
                        <span class={format!("model-tag {}", kind)}>{label}</span>
                    }).collect_view()}
                </div>
            </div>
            <div class="model-actions">
                {move || if is_editing() {
                    view! {
                        <input
                            type="text"
                            class="edit-display-name-input"
                            placeholder="显示名称"
                            prop:value=edit_display_name.get()
                            on:input=move |ev| edit_display_name.set(event_target_value(&ev))
                            on:keyup=move |ev| {
                                if ev.key() == "Enter" {
                                    on_save_edit.run(model_id);
                                }
                            }
                        />
                        <button
                            class="btn btn-sm btn-text"
                            on:click=move |_| on_save_edit.run(model_id)
                        >
                            "保存"
                        </button>
                    }.into_any()
                } else {
                    view! {
                        <button
                            class="btn btn-sm btn-text"
                            on:click=move |_| on_test.run(model_id)
                            disabled=move || testing_model.get() == Some(model_id)
                        >
                            {if testing_model.get() == Some(model_id) {
                                "测试中..."
                            } else {
                                "测试"
                            }}
                        </button>
                        {if !is_default {
                            view! {
                                <button
                                    class="btn btn-sm btn-text"
                                    on:click=move |_| on_set_default.run(model_id)
                                >
                                    "设为默认"
                                </button>
                            }.into_any()
                        } else {
                            view! { <span></span> }.into_any()
                        }}
                        <button
                            class="btn btn-sm btn-text"
                            on:click=move |_| {
                                editing_model.set(Some(model_id));
                                edit_display_name.set(model_display_name_store.get_value().unwrap_or_default());
                            }
                        >
                            "配置"
                        </button>
                        <button
                            class="btn btn-sm btn-text"
                            on:click=move |_| on_probe.run(model_id)
                            disabled=move || probing_model.get() == Some(model_id)
                        >
                            {if probing_model.get() == Some(model_id) {
                                "探测中..."
                            } else {
                                "探测多模态"
                            }}
                        </button>
                        {if !is_builtin {
                            view! {
                                <button
                                    class="btn btn-sm btn-text danger"
                                    on:click=move |_| on_delete.run(model_id)
                                >
                                    "删除"
                                </button>
                            }.into_any()
                        } else {
                            view! { <span></span> }.into_any()
                        }}
                    }.into_any()
                }}
            </div>
        </div>
    }
}

// ============================================
// Model Management Modal
// ============================================
#[component]
pub fn ModelManageModal(
    provider: LlmProvider,
    #[prop(into)] on_close: Callback<()>,
    #[prop(into)] on_updated: Callback<()>,
) -> impl IntoView {
    let app_state = StoredValue::new(use_app_state());

    let search_query = RwSignal::new(String::new());
    let new_model_name = RwSignal::new(String::new());
    let adding = RwSignal::new(false);
    let discovering = RwSignal::new(false);
    let testing_model: RwSignal<Option<i64>> = RwSignal::new(None);
    let probing_model: RwSignal<Option<i64>> = RwSignal::new(None);
    let editing_model: RwSignal<Option<i64>> = RwSignal::new(None);
    let edit_display_name = RwSignal::new(String::new());
    let error_msg: RwSignal<Option<String>> = RwSignal::new(None);
    let success_msg: RwSignal<Option<String>> = RwSignal::new(None);

    let add_model_action = move || {
        let name = new_model_name.get();
        if name.trim().is_empty() {
            error_msg.set(Some("模型名称不能为空".to_string()));
            return;
        }
        adding.set(true);
        error_msg.set(None);
        success_msg.set(None);
        let service = app_state.get_value().llm_provider_service();
        let provider_id = provider.id;
        let req = AddModelRequest {
            name: name.trim().to_string(),
            display_name: None,
        };
        spawn_local(async move {
            match service.add_model(provider_id, req).await {
                Ok(_) => {
                    adding.set(false);
                    new_model_name.set(String::new());
                    success_msg.set(Some("添加成功".to_string()));
                    on_updated.run(());
                }
                Err(e) => {
                    adding.set(false);
                    error_msg.set(Some(format!("添加失败: {}", e)));
                }
            }
        });
    };

    let on_discover_models = move |_| {
        discovering.set(true);
        error_msg.set(None);
        success_msg.set(None);
        let service = app_state.get_value().llm_provider_service();
        let provider_id = provider.id;
        spawn_local(async move {
            match service.discover_models(provider_id).await {
                Ok(resp) => {
                    discovering.set(false);
                    if resp.added_count > 0 {
                        success_msg.set(Some(format!(
                            "发现 {} 个新模型，已自动添加",
                            resp.added_count
                        )));
                    } else {
                        success_msg.set(Some("未发现新模型".to_string()));
                    }
                    on_updated.run(());
                }
                Err(e) => {
                    discovering.set(false);
                    error_msg.set(Some(format!("发现失败: {}", e)));
                }
            }
        });
    };

    let on_delete_model = move |model_id: i64| {
        let service = app_state.get_value().llm_provider_service();
        let provider_id = provider.id;
        spawn_local(async move {
            let _ = service.delete_model(provider_id, model_id).await;
            on_updated.run(());
        });
    };

    let on_set_default_model = move |model_id: i64| {
        let service = app_state.get_value().llm_provider_service();
        let provider_id = provider.id;
        spawn_local(async move {
            let _ = service.set_default_model(provider_id, model_id).await;
            on_updated.run(());
        });
    };

    let on_test_model = move |model_id: i64, model_name: String| {
        testing_model.set(Some(model_id));
        error_msg.set(None);
        success_msg.set(None);
        let service = app_state.get_value().llm_provider_service();
        let provider_id = provider.id;
        spawn_local(async move {
            match service.test_model_connection(provider_id, model_name).await {
                Ok(resp) => {
                    testing_model.set(None);
                    if resp.success {
                        success_msg.set(Some(resp.message));
                    } else {
                        error_msg.set(Some(resp.message));
                    }
                }
                Err(e) => {
                    testing_model.set(None);
                    error_msg.set(Some(format!("测试失败: {}", e)));
                }
            }
        });
    };

    let on_probe_multimodal = move |model_id: i64| {
        probing_model.set(Some(model_id));
        error_msg.set(None);
        success_msg.set(None);
        let service = app_state.get_value().llm_provider_service();
        let provider_id = provider.id;
        spawn_local(async move {
            match service.probe_model_multimodal(provider_id, model_id).await {
                Ok(resp) => {
                    probing_model.set(None);
                    if resp.success {
                        success_msg.set(Some(format!(
                            "多模态探测完成：图片={}, 视频={}",
                            if resp.supports_image { "支持" } else { "不支持" },
                            if resp.supports_video { "支持" } else { "不支持" }
                        )));
                        on_updated.run(());
                    } else {
                        error_msg.set(Some(resp.message));
                    }
                }
                Err(e) => {
                    probing_model.set(None);
                    error_msg.set(Some(format!("探测失败: {}", e)));
                }
            }
        });
    };

    let on_save_edit = move |model_id: i64| {
        let display_name = edit_display_name.get();
        let display_name_opt = if display_name.trim().is_empty() {
            None
        } else {
            Some(display_name.trim().to_string())
        };
        let service = app_state.get_value().llm_provider_service();
        let provider_id = provider.id;
        spawn_local(async move {
            let req = UpdateModelConfigRequest {
                display_name: display_name_opt,
            };
            let _ = service.update_model_config(provider_id, model_id, req).await;
            editing_model.set(None);
            on_updated.run(());
        });
    };

    // Filter models by search
    let filtered_models = move |models: Vec<LlmModel>| -> Vec<LlmModel> {
        let query = search_query.get().to_lowercase();
        if query.is_empty() {
            models
        } else {
            models
                .into_iter()
                .filter(|m| {
                    m.name.to_lowercase().contains(&query)
                        || m.display_name
                            .as_ref()
                            .map(|d| d.to_lowercase().contains(&query))
                            .unwrap_or(false)
                })
                .collect()
        }
    };

    view! {
        <Modal title=format!("{} — 模型管理", provider.name) on_close=move |_| on_close.run(()) class="modal-wide">
            <div class="modal-body model-manage-body">
                {move || error_msg.get().map(|e| view! {
                    <div class="error-message">{e}</div>
                })}
                {move || success_msg.get().map(|msg| view! {
                    <div class="success-message">{msg}</div>
                })}

                // Search
                <div class="search-box model-search">
                    <span class="search-icon">"🔍"</span>
                    <input
                        type="text"
                        placeholder="搜索模型..."
                        prop:value=search_query.get()
                        on:input=move |ev| search_query.set(event_target_value(&ev))
                    />
                </div>

                // Model list
                <div class="model-list-container">
                    {move || {
                        let builtin = filtered_models(provider.models.clone());
                        let extra = filtered_models(provider.extra_models.clone());

                        if builtin.is_empty() && extra.is_empty() {
                            view! {
                                <div class="empty-state compact">
                                    <p>"暂无模型"</p>
                                </div>
                            }.into_any()
                        } else {
                            view! {
                                <div class="model-list">
                                    // Built-in models section
                                    {if !builtin.is_empty() {
                                        view! {
                                            <div class="model-section-header">
                                                <span>"内置模型"</span>
                                            </div>
                                            {builtin.into_iter().map(|model| {
                                                let model_name = model.name.clone();
                                                view! {
                                                    <ModelItem
                                                        model=model
                                                        is_builtin=true
                                                        editing_model
                                                        edit_display_name
                                                        testing_model
                                                        probing_model
                                                        on_test=move |id: i64| on_test_model(id, model_name.clone())
                                                        on_set_default=on_set_default_model
                                                        on_probe=on_probe_multimodal
                                                        on_delete=move |_| {}
                                                        on_save_edit=on_save_edit
                                                    />
                                                }
                                            }).collect_view()}
                                        }.into_any()
                                    } else {
                                        view! { <span></span> }.into_any()
                                    }}
                                    // Extra models section
                                    {if !extra.is_empty() {
                                        view! {
                                            <div class="model-section-header">
                                                <span>"用户添加模型"</span>
                                            </div>
                                            {extra.into_iter().map(|model| {
                                                let model_name = model.name.clone();
                                                view! {
                                                    <ModelItem
                                                        model=model
                                                        is_builtin=false
                                                        editing_model
                                                        edit_display_name
                                                        testing_model
                                                        probing_model
                                                        on_test=move |id: i64| on_test_model(id, model_name.clone())
                                                        on_set_default=on_set_default_model
                                                        on_probe=on_probe_multimodal
                                                        on_delete=on_delete_model
                                                        on_save_edit=on_save_edit
                                                    />
                                                }
                                            }).collect_view()}
                                        }.into_any()
                                    } else {
                                        view! { <span></span> }.into_any()
                                    }}
                                </div>
                            }.into_any()
                        }
                    }}
                </div>

                // Add model inline
                <div class="add-model-inline">
                    <input
                        type="text"
                        placeholder="输入模型名称（如 llama2）"
                        prop:value=new_model_name.get()
                        on:input=move |ev| new_model_name.set(event_target_value(&ev))
                        on:keyup=move |ev| {
                            if ev.key() == "Enter" {
                                add_model_action();
                            }
                        }
                    />
                    <button
                        class="btn btn-primary"
                        on:click=move |_: leptos::ev::MouseEvent| add_model_action()
                        prop:disabled=move || adding.get() || new_model_name.get().trim().is_empty()
                    >
                        {if adding.get() { "添加中..." } else { "添加" }}
                    </button>
                </div>
            </div>
            <div class="modal-footer model-manage-footer">
                {if provider.support_model_discovery {
                    view! {
                        <button
                            class="btn btn-secondary"
                            on:click=on_discover_models
                            disabled=move || discovering.get()
                        >
                            <span>"🔍"</span>
                            {if discovering.get() { "发现中..." } else { "自动发现模型" }}
                        </button>
                    }.into_any()
                } else {
                    view! { <span></span> }.into_any()
                }}
                <div class="footer-spacer" />
                <button class="btn btn-secondary" on:click=move |_| on_close.run(())>
                    "关闭"
                </button>
            </div>
        </Modal>
    }
}

// ============================================
// Add Custom Provider Modal
// ============================================
#[component]
pub fn AddProviderModal(
    #[prop(into)] on_close: Callback<()>,
    #[prop(into)] on_created: Callback<()>,
) -> impl IntoView {
    let app_state = StoredValue::new(use_app_state());

    let provider_id = RwSignal::new(String::new());
    let name = RwSignal::new(String::new());
    let protocol = RwSignal::new("openai-compatible".to_string());
    let base_url = RwSignal::new(String::new());
    let creating = RwSignal::new(false);
    let error_msg: RwSignal<Option<String>> = RwSignal::new(None);

    let on_create = move |_| {
        let pid = provider_id.get().trim().to_string();
        let pname = name.get().trim().to_string();

        if pid.is_empty() {
            error_msg.set(Some("Provider ID 不能为空".to_string()));
            return;
        }
        if !pid
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
        {
            error_msg.set(Some(
                "Provider ID 只能包含小写字母、数字、连字符和下划线".to_string(),
            ));
            return;
        }
        if pname.is_empty() {
            error_msg.set(Some("显示名称不能为空".to_string()));
            return;
        }

        creating.set(true);
        error_msg.set(None);
        let service = app_state.get_value().llm_provider_service();
        let req = CreateProviderRequest {
            provider_id: pid,
            name: pname,
            protocol: protocol.get(),
            base_url: Some(base_url.get()).filter(|s| !s.is_empty()),
            api_key: None,
        };
        spawn_local(async move {
            match service.create_provider(req).await {
                Ok(_) => {
                    creating.set(false);
                    on_created.run(());
                    on_close.run(());
                }
                Err(e) => {
                    creating.set(false);
                    error_msg.set(Some(format!("创建失败: {}", e)));
                }
            }
        });
    };

    view! {
        <Modal title="添加自定义提供商" on_close=move |_| on_close.run(()) class="modal-wide">
            <div class="modal-body add-provider-body">
                {move || error_msg.get().map(|e| view! {
                    <div class="error-message">{e}</div>
                })}

                <div class="form-group required">
                    <label>"提供商 ID"</label>
                    <input
                        type="text"
                        placeholder="例如 openai, google, anthropic"
                        prop:value=provider_id.get()
                        on:input=move |ev| provider_id.set(event_target_value(&ev))
                    />
                    <p class="field-hint">
                        "小写字母、数字、连字符、下划线，创建后不可更改。"
                    </p>
                </div>

                <div class="form-group required">
                    <label>"显示名称"</label>
                    <input
                        type="text"
                        placeholder="例如 OpenAI, Google Gemini"
                        prop:value=name.get()
                        on:input=move |ev| name.set(event_target_value(&ev))
                    />
                </div>

                <div class="form-group">
                    <label>"默认 Base URL"</label>
                    <input
                        type="text"
                        placeholder="例如 https://api.example.com"
                        prop:value=base_url.get()
                        on:input=move |ev| base_url.set(event_target_value(&ev))
                    />
                </div>

                <div class="form-group required">
                    <label>"协议"</label>
                    <select
                        prop:value=protocol.get()
                        on:change=move |ev| protocol.set(event_target_value(&ev))
                    >
                        <option value="openai-compatible">"OpenAI 兼容（Chat Completions）"</option>
                        <option value="anthropic">"Anthropic（Messages API）"</option>
                    </select>
                </div>
            </div>
            <div class="modal-footer">
                <button class="btn btn-secondary" on:click=move |_| on_close.run(())>
                    "取消"
                </button>
                <button
                    class="btn btn-primary"
                    on:click=on_create
                    disabled=move || creating.get()
                >
                    {if creating.get() { "创建中..." } else { "创建" }}
                </button>
            </div>
        </Modal>
    }
}
