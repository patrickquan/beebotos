//! Gateway Configuration Wizard Page
//!
//! Interactive 10-step configuration wizard for BeeBotOS Gateway.

use crate::components::wizard::{ConfigPreview, SecretInput, WizardNavigation, WizardStepper};
use crate::i18n::I18nContext;
use crate::state::wizard::*;
use crate::utils::{download_file, event_target_checked, event_target_value};
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos::view;
use leptos_meta::*;
use leptos_router::components::A;

const TOTAL_STEPS: usize = 10;

#[component]
pub fn SetupPage() -> impl IntoView {
    provide_wizard_state();
    let state = use_wizard_state();
    let current_step = RwSignal::new(1usize);
    let is_submitting = RwSignal::new(false);
    let i18n = use_context::<I18nContext>().expect("i18n context not found");
    let i18n_stored = StoredValue::new(i18n);

    // Sync current_step with wizard state
    Effect::new(move |_| {
        let step = current_step.get();
        state.update(|s| s.current_step = step);
    });

    let can_proceed: Signal<bool> = Memo::new(move |_| {
        let s = state.get();
        s.can_proceed()
    }).into();

    let toml_preview: Signal<String> = Memo::new(move |_| state.get().generate_toml()).into();
    let env_preview: Signal<String> = Memo::new(move |_| state.get().generate_env()).into();
    let docker_preview: Signal<String> = Memo::new(move |_| state.get().generate_docker_compose()).into();
    let k8s_preview: Signal<String> = Memo::new(move |_| state.get().generate_k8s()).into();

    let on_back = move || {
        current_step.update(|s| {
            if *s > 1 {
                *s -= 1;
            }
        });
    };

    let on_next = move || {
        let step = current_step.get();
        state.update(|s| {
            let _ = s.validate_step(step);
        });
        current_step.update(|s| {
            if *s < TOTAL_STEPS {
                *s += 1;
            }
        });
    };

    let on_finish = move || {
        is_submitting.set(true);
        state.update(|s| {
            s.toml_preview = s.generate_toml();
            s.env_preview = s.generate_env();
        });
        // Simulate deployment
        spawn_local(async move {
            gloo_timers::future::TimeoutFuture::new(1500).await;
            is_submitting.set(false);
        });
    };

    view! {
        <Title text={move || i18n_stored.get_value().t("setup-page-title")} />
        <div class="page setup-page">
            <div class="wizard-container">
                <WizardStepper current_step=current_step />

                <div class="wizard-content">
                    {move || match current_step.get() {
                        1 => view! { <StepWelcome state=state current_step=current_step i18n=i18n_stored.get_value() /> }.into_any(),
                        2 => view! { <StepServer state=state i18n=i18n_stored.get_value() /> }.into_any(),
                        3 => view! { <StepDatabase state=state i18n=i18n_stored.get_value() /> }.into_any(),
                        4 => view! { <StepSecurity state=state i18n=i18n_stored.get_value() /> }.into_any(),
                        5 => view! { <StepLlmModels state=state i18n=i18n_stored.get_value() /> }.into_any(),
                        6 => view! { <StepChannels state=state i18n=i18n_stored.get_value() /> }.into_any(),
                        7 => view! { <StepBlockchain state=state i18n=i18n_stored.get_value() /> }.into_any(),
                        8 => view! { <StepLogging state=state i18n=i18n_stored.get_value() /> }.into_any(),
                        9 => view! { <StepReview state=state toml=toml_preview.into() env=env_preview.into() i18n=i18n_stored.get_value() /> }.into_any(),
                        10 => view! { <StepDeploy state=state toml=toml_preview.into() env=env_preview.into() docker=docker_preview.into() k8s=k8s_preview.into() i18n=i18n_stored.get_value() /> }.into_any(),
                        _ => view! { <StepWelcome state=state current_step=current_step i18n=i18n_stored.get_value() /> }.into_any(),
                    }}
                </div>

                <WizardNavigation
                    current_step=current_step
                    total_steps=TOTAL_STEPS
                    can_proceed=can_proceed.into()
                    on_back=Callback::new(move |_| { on_back(); })
                    on_next=Callback::new(move |_| { on_next(); })
                    on_finish=Callback::new(move |_| { on_finish(); })
                    is_submitting=is_submitting
                />
            </div>
        </div>
    }
}

// ============== Step 1: Welcome ==============
#[component]
fn StepWelcome(state: RwSignal<WizardState>, current_step: RwSignal<usize>, i18n: I18nContext) -> impl IntoView {
    let i18n_stored = StoredValue::new(i18n);
    let select_mode = move |mode: &str| {
        state.update(|s| {
            s.mode = match mode {
                "fresh" => WizardMode::Fresh,
                "minimal" => {
                    s.apply_template("minimal");
                    WizardMode::Template("minimal".to_string())
                }
                "standard" => {
                    s.apply_template("standard");
                    WizardMode::Template("standard".to_string())
                }
                "enterprise" => {
                    s.apply_template("enterprise");
                    WizardMode::Template("enterprise".to_string())
                }
                _ => WizardMode::Fresh,
            };
        });
        current_step.set(2);
    };

    view! {
        <div class="step-content">
            <div class="welcome-header">
                <div class="welcome-icon">"🐝"</div>
                <h1>{move || i18n_stored.get_value().t("setup-welcome-title")}</h1>
                <p>{move || i18n_stored.get_value().t("setup-welcome-subtitle")}</p>
            </div>

            <div class="setup-modes">
                <div class="mode-card" on:click={
                    let cb = select_mode.clone();
                    move |_| cb("fresh")
                }>
                    <div class="mode-icon">"🆕"</div>
                    <h3>{move || i18n_stored.get_value().t("setup-mode-fresh")}</h3>
                    <p>{move || i18n_stored.get_value().t("setup-mode-fresh-desc")}</p>
                </div>

                <div class="mode-card" on:click={
                    let cb = select_mode.clone();
                    move |_| cb("minimal")
                }>
                    <div class="mode-icon">"⚡"</div>
                    <h3>{move || i18n_stored.get_value().t("setup-mode-minimal")}</h3>
                    <p>{move || i18n_stored.get_value().t("setup-mode-minimal-desc")}</p>
                </div>

                <div class="mode-card" on:click={
                    let cb = select_mode.clone();
                    move |_| cb("standard")
                }>
                    <div class="mode-icon">"📦"</div>
                    <h3>{move || i18n_stored.get_value().t("setup-mode-standard")}</h3>
                    <p>{move || i18n_stored.get_value().t("setup-mode-standard-desc")}</p>
                </div>

                <div class="mode-card" on:click={
                    let cb = select_mode.clone();
                    move |_| cb("enterprise")
                }>
                    <div class="mode-icon">"🏢"</div>
                    <h3>{move || i18n_stored.get_value().t("setup-mode-enterprise")}</h3>
                    <p>{move || i18n_stored.get_value().t("setup-mode-enterprise-desc")}</p>
                </div>
            </div>
        </div>
    }
}

// ============== Step 2: Server ==============
#[component]
fn StepServer(state: RwSignal<WizardState>, i18n: I18nContext) -> impl IntoView {
    let i18n_stored = StoredValue::new(i18n);
    view! {
        <div class="step-content">
            <h2>{move || i18n_stored.get_value().t("setup-server-title")}</h2>
            <p class="step-description">{move || i18n_stored.get_value().t("setup-server-desc")}</p>

            <div class="form-grid">
                <div class="form-group">
                    <label>{move || i18n_stored.get_value().t("setup-server-host")}</label>
                    <input
                        type="text"
                        prop:value=move || state.get().server.host.clone()
                        on:input=move |e| state.update(|s| s.server.host = event_target_value(&e))
                    />
                </div>
                <div class="form-group">
                    <label>{move || i18n_stored.get_value().t("setup-server-http-port")}</label>
                    <input
                        type="number"
                        prop:value=move || state.get().server.http_port.to_string()
                        on:change=move |e| state.update(|s| s.server.http_port = event_target_value(&e).parse().unwrap_or(8000))
                    />
                </div>
                <div class="form-group">
                    <label>{move || i18n_stored.get_value().t("setup-server-grpc-port")}</label>
                    <input
                        type="number"
                        prop:value=move || state.get().server.grpc_port.to_string()
                        on:change=move |e| state.update(|s| s.server.grpc_port = event_target_value(&e).parse().unwrap_or(50051))
                    />
                </div>
                <div class="form-group">
                    <label>{move || i18n_stored.get_value().t("setup-server-timeout")}</label>
                    <input
                        type="number"
                        prop:value=move || state.get().server.request_timeout.to_string()
                        on:change=move |e| state.update(|s| s.server.request_timeout = event_target_value(&e).parse().unwrap_or(30))
                    />
                </div>
                <div class="form-group">
                    <label>{move || i18n_stored.get_value().t("setup-server-max-body")}</label>
                    <input
                        type="number"
                        prop:value=move || state.get().server.max_body_size.to_string()
                        on:change=move |e| state.update(|s| s.server.max_body_size = event_target_value(&e).parse().unwrap_or(10))
                    />
                </div>
                <div class="form-group">
                    <label>{move || i18n_stored.get_value().t("setup-server-cors")}</label>
                    <input
                        type="text"
                        prop:value=move || state.get().server.cors_origins.join(", ")
                        on:change=move |e| {
                            let val = event_target_value(&e);
                            state.update(|s| {
                                s.server.cors_origins = val.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
                            });
                        }
                    />
                </div>
            </div>

            <details class="advanced-section">
                <summary>{move || i18n_stored.get_value().t("setup-server-advanced-tls")}</summary>
                <div class="form-group checkbox-group">
                    <label class="checkbox-label">
                        <input
                            type="checkbox"
                            prop:checked=move || state.get().server.tls_enabled
                            on:change=move |e| state.update(|s| s.server.tls_enabled = event_target_checked(&e))
                        />
                        <span>{move || i18n_stored.get_value().t("setup-server-enable-tls")}</span>
                    </label>
                </div>
                {move || if state.get().server.tls_enabled {
                    view! {
                        <>
                            <div class="form-group">
                                <label>{move || i18n_stored.get_value().t("setup-server-tls-cert")}</label>
                                <input
                                    type="text"
                                    prop:value=move || state.get().server.tls_cert_path.clone()
                                    on:input=move |e| state.update(|s| s.server.tls_cert_path = event_target_value(&e))
                                />
                            </div>
                            <div class="form-group">
                                <label>{move || i18n_stored.get_value().t("setup-server-tls-key")}</label>
                                <input
                                    type="text"
                                    prop:value=move || state.get().server.tls_key_path.clone()
                                    on:input=move |e| state.update(|s| s.server.tls_key_path = event_target_value(&e))
                                />
                            </div>
                            <div class="form-group checkbox-group">
                                <label class="checkbox-label">
                                    <input
                                        type="checkbox"
                                        prop:checked=move || state.get().server.mtls_enabled
                                        on:change=move |e| state.update(|s| s.server.mtls_enabled = event_target_checked(&e))
                                    />
                                    <span>{move || i18n_stored.get_value().t("setup-server-enable-mtls")}</span>
                                </label>
                            </div>
                        </>
                    }.into_any()
                } else {
                    ().into_any()
                }}
            </details>
        </div>
    }
}

// ============== Step 3: Database ==============
#[component]
fn StepDatabase(state: RwSignal<WizardState>, i18n: I18nContext) -> impl IntoView {
    let i18n_stored = StoredValue::new(i18n);
    view! {
        <div class="step-content">
            <h2>{move || i18n_stored.get_value().t("setup-db-title")}</h2>
            <p class="step-description">{move || i18n_stored.get_value().t("setup-db-desc")}</p>

            <div class="form-group">
                <label>{move || i18n_stored.get_value().t("setup-db-type")}</label>
                <select
                    prop:value=move || state.get().database.db_type.clone()
                    on:change=move |e| state.update(|s| s.database.db_type = event_target_value(&e))
                >
                    <option value="sqlite">{move || i18n_stored.get_value().t("setup-db-sqlite")}</option>
                    <option value="postgres">{move || i18n_stored.get_value().t("setup-db-postgres")}</option>
                </select>
            </div>

            {move || if state.get().database.db_type == "sqlite" {
                view! {
                    <div class="form-group">
                        <label>{move || i18n_stored.get_value().t("setup-db-sqlite-path")}</label>
                        <input
                            type="text"
                            prop:value=move || state.get().database.sqlite_path.clone()
                            on:input=move |e| state.update(|s| s.database.sqlite_path = event_target_value(&e))
                        />
                    </div>
                }.into_any()
            } else {
                view! {
                    <div class="form-group">
                        <label>{move || i18n_stored.get_value().t("setup-db-postgres-url")}</label>
                        <input
                            type="text"
                            placeholder="postgres://user:pass@host/db"
                            prop:value=move || state.get().database.postgres_url.clone()
                            on:input=move |e| state.update(|s| s.database.postgres_url = event_target_value(&e))
                        />
                    </div>
                }.into_any()
            }}

            <div class="form-grid">
                <div class="form-group">
                    <label>{move || i18n_stored.get_value().t("setup-db-max-conn")}</label>
                    <input
                        type="number"
                        prop:value=move || state.get().database.max_connections.to_string()
                        on:change=move |e| state.update(|s| s.database.max_connections = event_target_value(&e).parse().unwrap_or(10))
                    />
                </div>
                <div class="form-group">
                    <label>{move || i18n_stored.get_value().t("setup-db-min-conn")}</label>
                    <input
                        type="number"
                        prop:value=move || state.get().database.min_connections.to_string()
                        on:change=move |e| state.update(|s| s.database.min_connections = event_target_value(&e).parse().unwrap_or(2))
                    />
                </div>
                <div class="form-group">
                    <label>{move || i18n_stored.get_value().t("setup-db-connect-timeout")}</label>
                    <input
                        type="number"
                        prop:value=move || state.get().database.connect_timeout.to_string()
                        on:change=move |e| state.update(|s| s.database.connect_timeout = event_target_value(&e).parse().unwrap_or(30))
                    />
                </div>
            </div>

            <div class="form-group checkbox-group">
                <label class="checkbox-label">
                    <input
                        type="checkbox"
                        prop:checked=move || state.get().database.auto_migrate
                        on:change=move |e| state.update(|s| s.database.auto_migrate = event_target_checked(&e))
                    />
                    <span>{move || i18n_stored.get_value().t("setup-db-auto-migrate")}</span>
                </label>
            </div>
        </div>
    }
}

// ============== Step 4: Security ==============
#[component]
fn StepSecurity(state: RwSignal<WizardState>, i18n: I18nContext) -> impl IntoView {
    let i18n_stored = StoredValue::new(i18n);
    let secret = RwSignal::new(String::new());

    Effect::new(move |_| {
        secret.set(state.get().jwt.secret.clone());
    });

    let generate_error = RwSignal::new(None::<String>);

    let generate = move || {
        match generate_jwt_secret() {
            Ok(new_secret) => {
                generate_error.set(None);
                secret.set(new_secret.clone());
                state.update(|s| s.jwt.secret = new_secret);
            }
            Err(e) => {
                generate_error.set(Some(e));
            }
        }
    };

    view! {
        <div class="step-content">
            <h2>{move || i18n_stored.get_value().t("setup-security-title")}</h2>
            <p class="step-description">{move || i18n_stored.get_value().t("setup-security-desc")}</p>

            <div class="form-group">
                <label>{move || i18n_stored.get_value().t("setup-security-jwt-secret")}</label>
                <SecretInput
                    value=secret
                    placeholder=i18n_stored.get_value().t("setup-security-jwt-placeholder")
                    on_generate=Callback::new(move |_| { generate(); })
                />
                <p class="form-help">{move || i18n_stored.get_value().t("setup-security-jwt-help")}</p>
                {move || generate_error.get().map(|err| view! {
                    <p class="form-error">{err}</p>
                })}
            </div>

            <div class="form-grid">
                <div class="form-group">
                    <label>{move || i18n_stored.get_value().t("setup-security-token-expiry")}</label>
                    <input
                        type="number"
                        prop:value=move || state.get().jwt.token_expiry.to_string()
                        on:change=move |e| state.update(|s| s.jwt.token_expiry = event_target_value(&e).parse().unwrap_or(3600))
                    />
                </div>
                <div class="form-group">
                    <label>{move || i18n_stored.get_value().t("setup-security-refresh-expiry")}</label>
                    <input
                        type="number"
                        prop:value=move || state.get().jwt.refresh_token_expiry.to_string()
                        on:change=move |e| state.update(|s| s.jwt.refresh_token_expiry = event_target_value(&e).parse().unwrap_or(604800))
                    />
                </div>
            </div>

            <div class="form-group checkbox-group">
                <label class="checkbox-label">
                    <input
                        type="checkbox"
                        prop:checked=move || state.get().jwt.rate_limit_enabled
                        on:change=move |e| state.update(|s| s.jwt.rate_limit_enabled = event_target_checked(&e))
                    />
                    <span>{move || i18n_stored.get_value().t("setup-security-enable-rate-limit")}</span>
                </label>
            </div>

            {move || if state.get().jwt.rate_limit_enabled {
                view! {
                    <div class="form-grid">
                        <div class="form-group">
                            <label>{move || i18n_stored.get_value().t("setup-security-qps-limit")}</label>
                            <input
                                type="number"
                                prop:value=move || state.get().jwt.qps_limit.to_string()
                                on:change=move |e| state.update(|s| s.jwt.qps_limit = event_target_value(&e).parse().unwrap_or(100))
                            />
                        </div>
                        <div class="form-group">
                            <label>{move || i18n_stored.get_value().t("setup-security-burst-limit")}</label>
                            <input
                                type="number"
                                prop:value=move || state.get().jwt.burst_limit.to_string()
                                on:change=move |e| state.update(|s| s.jwt.burst_limit = event_target_value(&e).parse().unwrap_or(200))
                            />
                        </div>
                    </div>
                }.into_any()
            } else {
                ().into_any()
            }}
        </div>
    }
}

// ============== Step 5: LLM Models ==============
#[component]
fn StepLlmModels(state: RwSignal<WizardState>, i18n: I18nContext) -> impl IntoView {
    let i18n_stored = StoredValue::new(i18n);
    let new_provider_name = RwSignal::new(String::new());

    let add_provider = move || {
        let name = new_provider_name.get().trim().to_lowercase();
        if name.is_empty() {
            return;
        }
        let presets: std::collections::HashMap<&str, (&str, &str)> = [
            ("kimi", ("moonshot-v1-8k", "https://api.moonshot.cn")),
            ("openai", ("gpt-4", "https://api.openai.com/v1")),
            ("anthropic", ("claude-3-sonnet", "https://api.anthropic.com")),
            ("zhipu", ("glm-4", "https://open.bigmodel.cn/api/paas/v4")),
            ("deepseek", ("deepseek-chat", "https://api.deepseek.com")),
            ("ollama", ("llama2", "http://localhost:11434")),
        ].into_iter().collect();

        let (model, base_url) = presets.get(name.as_str()).copied().unwrap_or(("gpt-4", "https://api.openai.com/v1"));

        state.update(|s| {
            s.models.providers.push(ProviderDraft {
                name: name.clone(),
                api_key: String::new(),
                model: model.to_string(),
                base_url: base_url.to_string(),
                temperature: 0.7,
                context_window: Some(8192),
            });
            if s.models.default_provider.is_empty() {
                s.models.default_provider = name.clone();
            }
        });
        new_provider_name.set(String::new());
    };

    view! {
        <div class="step-content">
            <h2>{move || i18n_stored.get_value().t("setup-llm-title")}</h2>
            <p class="step-description">{move || i18n_stored.get_value().t("setup-llm-desc")}</p>

            <div class="form-grid">
                <div class="form-group">
                    <label>{move || i18n_stored.get_value().t("setup-llm-default-provider")}</label>
                    <select
                        prop:value=move || state.get().models.default_provider.clone()
                        on:change=move |e| state.update(|s| s.models.default_provider = event_target_value(&e))
                    >
                        {move || state.get().models.providers.iter().map(|p| {
                            let n = p.name.clone();
                            view! { <option value=n.clone()>{n.clone()}</option> }
                        }).collect::<Vec<_>>()}
                    </select>
                </div>
                <div class="form-group">
                    <label>{move || i18n_stored.get_value().t("setup-llm-max-tokens")}</label>
                    <input
                        type="number"
                        prop:value=move || state.get().models.max_tokens.to_string()
                        on:change=move |e| state.update(|s| s.models.max_tokens = event_target_value(&e).parse().unwrap_or(4096))
                    />
                </div>
                <div class="form-group">
                    <label>{move || i18n_stored.get_value().t("setup-llm-timeout")}</label>
                    <input
                        type="number"
                        prop:value=move || state.get().models.request_timeout.to_string()
                        on:change=move |e| state.update(|s| s.models.request_timeout = event_target_value(&e).parse().unwrap_or(60))
                    />
                </div>
            </div>

            <div class="form-group checkbox-group">
                <label class="checkbox-label">
                    <input
                        type="checkbox"
                        prop:checked=move || state.get().models.cost_optimization
                        on:change=move |e| state.update(|s| s.models.cost_optimization = event_target_checked(&e))
                    />
                    <span>{move || i18n_stored.get_value().t("setup-llm-cost-opt")}</span>
                </label>
            </div>

            <div class="form-group">
                <label>{move || i18n_stored.get_value().t("setup-llm-system-prompt")}</label>
                <textarea
                    rows="3"
                    prop:value=move || state.get().models.system_prompt.clone()
                    on:input=move |e| state.update(|s| s.models.system_prompt = event_target_value(&e))
                />
            </div>

            <h3>{move || i18n_stored.get_value().t("setup-llm-providers")}</h3>
            <div class="providers-list">
                {move || state.get().models.providers.iter().enumerate().map(|(idx, p)| {
                    let p = p.clone();
                    let idx = idx;
                    view! {
                        <div class="provider-card-config">
                            <div class="provider-card-header">
                                <h4>{p.name.clone()}</h4>
                                <button
                                    class="btn btn-icon btn-danger"
                                    on:click=move |_| state.update(|s| { s.models.providers.remove(idx); })
                                >
                                    "🗑"
                                </button>
                            </div>
                            <div class="form-group">
                                <label>{move || i18n_stored.get_value().t("setup-llm-api-key")}</label>
                                <input
                                    type="password"
                                    prop:value=p.api_key.clone()
                                    on:input=move |e| {
                                        let val = event_target_value(&e);
                                        state.update(|s| { if let Some(prov) = s.models.providers.get_mut(idx) { prov.api_key = val; } });
                                    }
                                />
                            </div>
                            <div class="form-group">
                                <label>{move || i18n_stored.get_value().t("setup-llm-model")}</label>
                                <input
                                    type="text"
                                    prop:value=p.model.clone()
                                    on:input=move |e| {
                                        let val = event_target_value(&e);
                                        state.update(|s| { if let Some(prov) = s.models.providers.get_mut(idx) { prov.model = val; } });
                                    }
                                />
                            </div>
                            <div class="form-group">
                                <label>{move || i18n_stored.get_value().t("setup-llm-base-url")}</label>
                                <input
                                    type="text"
                                    prop:value=p.base_url.clone()
                                    on:input=move |e| {
                                        let val = event_target_value(&e);
                                        state.update(|s| { if let Some(prov) = s.models.providers.get_mut(idx) { prov.base_url = val; } });
                                    }
                                />
                            </div>
                            <div class="form-row">
                                <div class="form-group">
                                    <label>{move || i18n_stored.get_value().t("setup-llm-temperature")}</label>
                                    <input
                                        type="number"
                                        step="0.1"
                                        min="0"
                                        max="2"
                                        prop:value=format!("{:.1}", p.temperature)
                                        on:change=move |e| {
                                            let val = event_target_value(&e).parse().unwrap_or(0.7);
                                            state.update(|s| { if let Some(prov) = s.models.providers.get_mut(idx) { prov.temperature = val; } });
                                        }
                                    />
                                </div>
                                <div class="form-group">
                                    <label>{move || i18n_stored.get_value().t("setup-llm-context-window")}</label>
                                    <input
                                        type="number"
                                        prop:value=p.context_window.map(|c| c.to_string()).unwrap_or_default()
                                        on:change=move |e| {
                                            let val = event_target_value(&e).parse().ok();
                                            state.update(|s| { if let Some(prov) = s.models.providers.get_mut(idx) { prov.context_window = val; } });
                                        }
                                    />
                                </div>
                            </div>
                        </div>
                    }
                }).collect::<Vec<_>>()}
            </div>

            <div class="add-provider-row">
                <input
                    type="text"
                    placeholder={move || i18n_stored.get_value().t("setup-llm-provider-placeholder")}
                    prop:value=new_provider_name
                    on:input=move |e| new_provider_name.set(event_target_value(&e))
                />
                <button class="btn btn-secondary" on:click=move |_| add_provider()>
                    {move || i18n_stored.get_value().t("setup-llm-add-provider")}
                </button>
            </div>
        </div>
    }
}

// ============== Step 6: Channels ==============
#[component]
fn StepChannels(state: RwSignal<WizardState>, i18n: I18nContext) -> impl IntoView {
    let i18n_stored = StoredValue::new(i18n);
    view! {
        <div class="step-content">
            <h2>{move || i18n_stored.get_value().t("setup-channels-title")}</h2>
            <p class="step-description">{move || i18n_stored.get_value().t("setup-channels-desc")}</p>

            <div class="form-grid">
                <div class="form-group">
                    <label>{move || i18n_stored.get_value().t("setup-channels-context-window")}</label>
                    <input
                        type="number"
                        prop:value=move || state.get().channels.context_window.to_string()
                        on:change=move |e| state.update(|s| s.channels.context_window = event_target_value(&e).parse().unwrap_or(20))
                    />
                </div>
                <div class="form-group">
                    <label>{move || i18n_stored.get_value().t("setup-channels-max-file")}</label>
                    <input
                        type="number"
                        prop:value=move || state.get().channels.max_file_size.to_string()
                        on:change=move |e| state.update(|s| s.channels.max_file_size = event_target_value(&e).parse().unwrap_or(50))
                    />
                </div>
                <div class="form-group">
                    <label>{move || i18n_stored.get_value().t("setup-channels-default-agent")}</label>
                    <input
                        type="text"
                        prop:value=move || state.get().channels.default_agent_id.clone()
                        on:input=move |e| state.update(|s| s.channels.default_agent_id = event_target_value(&e))
                    />
                </div>
            </div>

            <div class="form-group checkbox-group">
                <label class="checkbox-label">
                    <input
                        type="checkbox"
                        prop:checked=move || state.get().channels.auto_download_media
                        on:change=move |e| state.update(|s| s.channels.auto_download_media = event_target_checked(&e))
                    />
                    <span>{move || i18n_stored.get_value().t("setup-channels-auto-download")}</span>
                </label>
            </div>

            <div class="form-group checkbox-group">
                <label class="checkbox-label">
                    <input
                        type="checkbox"
                        prop:checked=move || state.get().channels.auto_reply
                        on:change=move |e| state.update(|s| s.channels.auto_reply = event_target_checked(&e))
                    />
                    <span>{move || i18n_stored.get_value().t("setup-channels-auto-reply")}</span>
                </label>
            </div>

            <h3>{move || i18n_stored.get_value().t("setup-channels-platforms")}</h3>
            <div class="platforms-list">
                {move || state.get().channels.platforms.iter().enumerate().map(|(idx, p)| {
                    let idx = idx;
                    let enabled = p.enabled;
                    view! {
                        <div class="platform-form">
                            <div class="platform-header">
                                <label class="checkbox-label">
                                    <input
                                        type="checkbox"
                                        prop:checked=enabled
                                        on:change=move |e| {
                                            let checked = event_target_checked(&e);
                                            state.update(|s| { if let Some(plat) = s.channels.platforms.get_mut(idx) { plat.enabled = checked; } });
                                        }
                                    />
                                    <span class="platform-name">{p.name.clone()}</span>
                                </label>
                                <span class=format!("status-badge {}", if enabled { "enabled" } else { "disabled" })>
                                    {if enabled { i18n_stored.get_value().t("status-enabled") } else { i18n_stored.get_value().t("status-disabled") }}
                                </span>
                            </div>
                        </div>
                    }
                }).collect::<Vec<_>>()}
            </div>
        </div>
    }
}

// ============== Step 7: Blockchain ==============
#[component]
fn StepBlockchain(state: RwSignal<WizardState>, i18n: I18nContext) -> impl IntoView {
    let i18n_stored = StoredValue::new(i18n);
    view! {
        <div class="step-content">
            <h2>{move || i18n_stored.get_value().t("setup-blockchain-title")}</h2>
            <p class="step-description">{move || i18n_stored.get_value().t("setup-blockchain-desc")}</p>

            <div class="form-group checkbox-group">
                <label class="checkbox-label">
                    <input
                        type="checkbox"
                        prop:checked=move || state.get().blockchain.enabled
                        on:change=move |e| state.update(|s| s.blockchain.enabled = event_target_checked(&e))
                    />
                    <span>{move || i18n_stored.get_value().t("setup-blockchain-enable")}</span>
                </label>
            </div>

            {move || if state.get().blockchain.enabled {
                view! {
                    <>
                        <div class="form-group">
                            <label>{move || i18n_stored.get_value().t("setup-blockchain-chain-id")}</label>
                            <input
                                type="number"
                                prop:value=move || state.get().blockchain.chain_id.to_string()
                                on:change=move |e| state.update(|s| s.blockchain.chain_id = event_target_value(&e).parse().unwrap_or(1))
                            />
                        </div>
                        <div class="form-group">
                            <label>{move || i18n_stored.get_value().t("setup-blockchain-rpc")}</label>
                            <input
                                type="text"
                                placeholder="https://ethereum-rpc.publicnode.com"
                                prop:value=move || state.get().blockchain.rpc_url.clone()
                                on:input=move |e| state.update(|s| s.blockchain.rpc_url = event_target_value(&e))
                            />
                        </div>
                        <div class="form-group">
                            <label>{move || i18n_stored.get_value().t("setup-blockchain-mnemonic")}</label>
                            <input
                                type="password"
                                placeholder={move || i18n_stored.get_value().t("setup-blockchain-mnemonic-placeholder")}
                                prop:value=move || state.get().blockchain.wallet_mnemonic.clone()
                                on:input=move |e| state.update(|s| s.blockchain.wallet_mnemonic = event_target_value(&e))
                            />
                        </div>
                    </>
                }.into_any()
            } else {
                ().into_any()
            }}
        </div>
    }
}

// ============== Step 8: Logging ==============
#[component]
fn StepLogging(state: RwSignal<WizardState>, i18n: I18nContext) -> impl IntoView {
    let i18n_stored = StoredValue::new(i18n);
    view! {
        <div class="step-content">
            <h2>{move || i18n_stored.get_value().t("setup-logging-title")}</h2>
            <p class="step-description">{move || i18n_stored.get_value().t("setup-logging-desc")}</p>

            <div class="form-grid">
                <div class="form-group">
                    <label>{move || i18n_stored.get_value().t("setup-logging-level")}</label>
                    <select
                        prop:value=move || state.get().logging.level.clone()
                        on:change=move |e| state.update(|s| s.logging.level = event_target_value(&e))
                    >
                        <option value="trace">{move || i18n_stored.get_value().t("log-trace")}</option>
                        <option value="debug">{move || i18n_stored.get_value().t("log-debug")}</option>
                        <option value="info">{move || i18n_stored.get_value().t("log-info")}</option>
                        <option value="warn">{move || i18n_stored.get_value().t("log-warn")}</option>
                        <option value="error">{move || i18n_stored.get_value().t("log-error")}</option>
                    </select>
                </div>
                <div class="form-group">
                    <label>{move || i18n_stored.get_value().t("setup-logging-format")}</label>
                    <select
                        prop:value=move || state.get().logging.format.clone()
                        on:change=move |e| state.update(|s| s.logging.format = event_target_value(&e))
                    >
                        <option value="json">{move || i18n_stored.get_value().t("log-json")}</option>
                        <option value="pretty">{move || i18n_stored.get_value().t("log-pretty")}</option>
                        <option value="compact">{move || i18n_stored.get_value().t("log-compact")}</option>
                    </select>
                </div>
                <div class="form-group">
                    <label>{move || i18n_stored.get_value().t("setup-logging-file-path")}</label>
                    <input
                        type="text"
                        prop:value=move || state.get().logging.file_path.clone()
                        on:input=move |e| state.update(|s| s.logging.file_path = event_target_value(&e))
                    />
                </div>
                <div class="form-group">
                    <label>{move || i18n_stored.get_value().t("setup-logging-rotation")}</label>
                    <select
                        prop:value=move || state.get().logging.rotation.clone()
                        on:change=move |e| state.update(|s| s.logging.rotation = event_target_value(&e))
                    >
                        <option value="minutely">{move || i18n_stored.get_value().t("log-minutely")}</option>
                        <option value="hourly">{move || i18n_stored.get_value().t("log-hourly")}</option>
                        <option value="daily">{move || i18n_stored.get_value().t("log-daily")}</option>
                        <option value="never">{move || i18n_stored.get_value().t("log-never")}</option>
                    </select>
                </div>
            </div>

            <div class="form-group checkbox-group">
                <label class="checkbox-label">
                    <input
                        type="checkbox"
                        prop:checked=move || state.get().logging.enable_metrics
                        on:change=move |e| state.update(|s| s.logging.enable_metrics = event_target_checked(&e))
                    />
                    <span>{move || i18n_stored.get_value().t("setup-logging-enable-metrics")}</span>
                </label>
            </div>

            {move || if state.get().logging.enable_metrics {
                view! {
                    <div class="form-group">
                        <label>{move || i18n_stored.get_value().t("setup-logging-metrics-port")}</label>
                        <input
                            type="number"
                            prop:value=move || state.get().logging.metrics_port.to_string()
                            on:change=move |e| state.update(|s| s.logging.metrics_port = event_target_value(&e).parse().unwrap_or(9090))
                        />
                    </div>
                }.into_any()
            } else {
                ().into_any()
            }}

            <div class="form-group checkbox-group">
                <label class="checkbox-label">
                    <input
                        type="checkbox"
                        prop:checked=move || state.get().logging.enable_tracing
                        on:change=move |e| state.update(|s| s.logging.enable_tracing = event_target_checked(&e))
                    />
                    <span>{move || i18n_stored.get_value().t("setup-logging-enable-tracing")}</span>
                </label>
            </div>

            {move || if state.get().logging.enable_tracing {
                view! {
                    <>
                        <div class="form-group">
                            <label>{move || i18n_stored.get_value().t("setup-logging-otlp")}</label>
                            <input
                                type="text"
                                prop:value=move || state.get().logging.otlp_endpoint.clone()
                                on:input=move |e| state.update(|s| s.logging.otlp_endpoint = event_target_value(&e))
                            />
                        </div>
                        <div class="form-group">
                            <label>{move || i18n_stored.get_value().t("setup-logging-sampling")}</label>
                            <input
                                type="number"
                                step="0.01"
                                min="0"
                                max="1"
                                prop:value=move || state.get().logging.trace_sampling_rate.to_string()
                                on:change=move |e| state.update(|s| s.logging.trace_sampling_rate = event_target_value(&e).parse().unwrap_or(0.1))
                            />
                        </div>
                    </>
                }.into_any()
            } else {
                ().into_any()
            }}
        </div>
    }
}

// ============== Step 9: Review ==============
#[component]
fn StepReview(
    #[allow(unused_variables)] state: RwSignal<WizardState>,
    toml: Signal<String>,
    env: Signal<String>,
    i18n: I18nContext,
) -> impl IntoView {
    let i18n_stored = StoredValue::new(i18n);
    let errors = Signal::derive(move || state.get().validation_errors.clone());

    view! {
        <div class="step-content">
            <h2>{move || i18n_stored.get_value().t("setup-review-title")}</h2>
            <p class="step-description">{move || i18n_stored.get_value().t("setup-review-desc")}</p>

            {move || if !errors.get().is_empty() {
                view! {
                    <div class="validation-summary">
                        <h4>{move || i18n_stored.get_value().t("setup-review-warnings")}</h4>
                        <ul>
                            {errors.get().into_iter().map(|err| {
                                view! { <li>{format!("{}: {}", err.field, err.message)}</li> }
                            }).collect::<Vec<_>>()}
                        </ul>
                    </div>
                }.into_any()
            } else {
                view! {
                    <div class="validation-summary success">
                        {move || i18n_stored.get_value().t("setup-review-success")}
                    </div>
                }.into_any()
            }}

            <ConfigPreview toml_content=toml env_content=env />
        </div>
    }
}

// ============== Step 10: Deploy ==============
#[component]
fn StepDeploy(
    #[allow(unused_variables)] state: RwSignal<WizardState>,
    toml: Signal<String>,
    env: Signal<String>,
    docker: Signal<String>,
    k8s: Signal<String>,
    i18n: I18nContext,
) -> impl IntoView {
    let i18n_stored = StoredValue::new(i18n);
    let download_toml = move || {
        let content = toml.get();
        download_file("beebotos.toml", &content, "text/toml");
    };

    let download_env = move || {
        let content = env.get();
        download_file(".env", &content, "text/plain");
    };

    let download_docker = move || {
        let content = docker.get();
        download_file("docker-compose.yml", &content, "text/yaml");
    };

    let download_k8s = move || {
        let content = k8s.get();
        download_file("beebotos-k8s.yaml", &content, "text/yaml");
    };

    view! {
        <div class="step-content">
            <h2>{move || i18n_stored.get_value().t("setup-deploy-title")}</h2>
            <p class="step-description">{move || i18n_stored.get_value().t("setup-deploy-desc")}</p>

            <div class="deploy-options">
                <div class="deploy-card">
                    <h3>"📝 TOML Config"</h3>
                    <p>{move || i18n_stored.get_value().t("setup-deploy-toml-desc")}</p>
                    <button class="btn btn-primary" on:click=move |_| download_toml()>
                        {move || i18n_stored.get_value().t("setup-deploy-toml-btn")}
                    </button>
                </div>

                <div class="deploy-card">
                    <h3>"🔑 Environment Variables"</h3>
                    <p>{move || i18n_stored.get_value().t("setup-deploy-env-desc")}</p>
                    <button class="btn btn-primary" on:click=move |_| download_env()>
                        {move || i18n_stored.get_value().t("setup-deploy-env-btn")}
                    </button>
                </div>

                <div class="deploy-card">
                    <h3>"🐳 Docker Compose"</h3>
                    <p>{move || i18n_stored.get_value().t("setup-deploy-docker-desc")}</p>
                    <button class="btn btn-primary" on:click=move |_| download_docker()>
                        {move || i18n_stored.get_value().t("setup-deploy-docker-btn")}
                    </button>
                </div>

                <div class="deploy-card">
                    <h3>"☸️ Kubernetes"</h3>
                    <p>{move || i18n_stored.get_value().t("setup-deploy-k8s-desc")}</p>
                    <button class="btn btn-primary" on:click=move |_| download_k8s()>
                        {move || i18n_stored.get_value().t("setup-deploy-k8s-btn")}
                    </button>
                </div>
            </div>

            <div class="deploy-instructions">
                <h3>{move || i18n_stored.get_value().t("setup-deploy-instructions-title")}</h3>
                <ol>
                    <li>{move || i18n_stored.get_value().t("setup-deploy-step1")}</li>
                    <li>{move || i18n_stored.get_value().t("setup-deploy-step2")}</li>
                    <li>{move || i18n_stored.get_value().t("setup-deploy-step3")}</li>
                    <li>{move || i18n_stored.get_value().t("setup-deploy-step4")}</li>
                </ol>
                <A href="/settings" attr:class="btn btn-primary">
                    {move || i18n_stored.get_value().t("setup-deploy-go-settings")}
                </A>
            </div>
        </div>
    }
}

// DOM helpers imported from crate::utils
