//! LLM Configuration & Monitoring Page
//!
//! Displays global LLM configuration and real-time metrics from Gateway.

use crate::api::{LlmConfigService, LlmGlobalConfig, LlmMetricsResponse, LlmHealthResponse};
use crate::components::{BarChart, InlineLoading, InfoItem, PieChart};
use crate::i18n::I18nContext;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos::view;
use leptos_meta::*;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

#[component]
pub fn LlmConfigPage() -> impl IntoView {
    let client = crate::api::create_client();
    let llm_service = LlmConfigService::new(client);
    let service_stored = StoredValue::new(llm_service);

    let i18n = use_context::<I18nContext>().expect("i18n context not found");
    let i18n_stored = StoredValue::new(i18n);

    let config: RwSignal<Option<LlmGlobalConfig>> = RwSignal::new(None);
    let metrics: RwSignal<Option<LlmMetricsResponse>> = RwSignal::new(None);
    let health: RwSignal<Option<LlmHealthResponse>> = RwSignal::new(None);
    let error: RwSignal<Option<String>> = RwSignal::new(None);
    let loading = RwSignal::new(true);

    let fetch_all = move || {
        let service = service_stored.get_value();
        loading.set(true);
        error.set(None);
        spawn_local(async move {
            match service.get_config().await {
                Ok(c) => config.set(Some(c)),
                Err(e) => error.set(Some(format!("Config: {}", e))),
            }
            match service.get_metrics().await {
                Ok(m) => metrics.set(Some(m)),
                Err(e) => {
                    let msg = error.get().unwrap_or_default();
                    error.set(Some(format!("{} Metrics: {}", msg, e)));
                }
            }
            match service.get_health().await {
                Ok(h) => health.set(Some(h)),
                Err(e) => {
                    let msg = error.get().unwrap_or_default();
                    error.set(Some(format!("{} Health: {}", msg, e)));
                }
            }
            loading.set(false);
        });
    };

    let fetch_stored = StoredValue::new(fetch_all);

    // Initial fetch
    Effect::new(move |_| {
        fetch_stored.get_value()();
    });

    // Auto-refresh metrics every 10s
    let should_poll = RwSignal::new(true);

    Effect::new(move |_| {
        let should_poll = should_poll;

        // Sync polling state with document visibility
        if let Some(document) = web_sys::window().and_then(|w| w.document()) {
            let hidden = document.hidden();
            should_poll.set(!hidden);

            let doc_for_handler = document.clone();
            let visibility_handler = Closure::wrap(Box::new(move || {
                let hidden = doc_for_handler.hidden();
                should_poll.set(!hidden);
            }) as Box<dyn FnMut()>);
            let _ = document.add_event_listener_with_callback(
                "visibilitychange",
                visibility_handler.as_ref().unchecked_ref(),
            );
            visibility_handler.forget();
        }

        // Stop polling when component unmounts
        on_cleanup(move || {
            should_poll.set(false);
        });

        spawn_local(async move {
            loop {
                gloo_timers::future::TimeoutFuture::new(10_000).await;
                if !should_poll.get() {
                    break;
                }
                let service = service_stored.get_value();
                if let Ok(m) = service.get_metrics().await {
                    metrics.set(Some(m));
                }
                if let Ok(h) = service.get_health().await {
                    health.set(Some(h));
                }
            }
        });
    });

    view! {
        <Title text={move || format!("{} - BeeBotOS", i18n_stored.get_value().t("llm-config-title"))} />
        <div class="page llm-config-page">
            <div class="page-header">
                <h1>{move || i18n_stored.get_value().t("llm-config-title")}</h1>
                <p class="page-description">{move || i18n_stored.get_value().t("llm-config-subtitle")}</p>
            </div>

            {move || if loading.get() {
                view! { <InlineLoading /> }.into_any()
            } else if let Some(err) = error.get() {
                view! {
                    <div class="error-state">
                        <div class="error-icon">"⚠️"</div>
                        <p>{err}</p>
                        <button class="btn btn-primary" on:click=move |_| fetch_stored.get_value()()>
                            {move || i18n_stored.get_value().t("error-retry")}
                        </button>
                    </div>
                }.into_any()
            } else {
                view! {
                    <div class="llm-config-grid">
                        // Global Config Card
                        {config.get().map(|cfg| view! {
                            <section class="card llm-section">
                                <h2>{move || i18n_stored.get_value().t("llm-global-config")}</h2>
                                <div class="info-grid">
                                    <InfoItem class="info-row" label=i18n_stored.get_value().t("llm-default-provider") value=cfg.default_provider />
                                    <InfoItem class="info-row" label=i18n_stored.get_value().t("llm-max-tokens") value=cfg.max_tokens.to_string() />
                                    <InfoItem class="info-row" label=i18n_stored.get_value().t("llm-request-timeout") value=format!("{}s", cfg.request_timeout) />
                                    <InfoItem class="info-row" label=i18n_stored.get_value().t("llm-cost-optimization") value=if cfg.cost_optimization { i18n_stored.get_value().t("llm-enabled") } else { i18n_stored.get_value().t("llm-disabled") }.to_string() />
                                    <InfoItem class="info-row" label=i18n_stored.get_value().t("llm-fallback-chain") value=cfg.fallback_chain.join(", ") />
                                </div>
                                <div class="form-group">
                                    <label>{move || i18n_stored.get_value().t("llm-system-prompt")}</label>
                                    <textarea readonly class="system-prompt">{cfg.system_prompt}</textarea>
                                </div>
                            </section>
                        })}

                        // Provider Cards
                        {config.get().map(|cfg| view! {
                            <section class="card llm-section">
                                <h2>{move || i18n_stored.get_value().t("llm-providers")}</h2>
                                <div class="provider-cards">
                                    {cfg.providers.into_iter().map(|p| {
                                        let health_status = health.get()
                                            .and_then(|h| h.providers.iter().find(|ph| ph.name == p.name).cloned());
                                        view! {
                                            <div class="provider-card">
                                                <div class="provider-header">
                                                    <h3>{p.name.clone()}</h3>
                                                    {health_status.map(|h| view! {
                                                        <span class=format!("health-badge {}", if h.healthy { "healthy" } else { "unhealthy" })>
                                                            {if h.healthy { format!("● {}", i18n_stored.get_value().t("llm-healthy")) } else { format!("● {} {}", h.consecutive_failures, i18n_stored.get_value().t("llm-failures")) }}
                                                        </span>
                                                    })}
                                                </div>
                                                <div class="info-grid">
                                                    <InfoItem class="info-row" label=i18n_stored.get_value().t("llm-model") value=p.model />
                                                    <InfoItem class="info-row" label=i18n_stored.get_value().t("llm-base-url") value=p.base_url />
                                                    <InfoItem class="info-row" label=i18n_stored.get_value().t("llm-api-key") value=p.api_key_masked />
                                                    <InfoItem class="info-row" label=i18n_stored.get_value().t("llm-temperature") value=format!("{:.2}", p.temperature) />
                                                    <InfoItem class="info-row" label=i18n_stored.get_value().t("llm-context-window") value=p.context_window.map(|c| c.to_string()).unwrap_or_else(|| i18n_stored.get_value().t("llm-default")) />
                                                </div>
                                            </div>
                                        }
                                    }).collect::<Vec<_>>()}
                                </div>
                            </section>
                        })}

                        // Metrics Card
                        {metrics.get().map(|m| view! {
                            <section class="card llm-section">
                                <h2>{move || i18n_stored.get_value().t("llm-realtime-metrics")}</h2>
                                <p class="timestamp">{format!("{}: {}", i18n_stored.get_value().t("llm-last-updated"), m.timestamp)}</p>
                                <div class="metrics-grid">
                                    <MetricCard
                                        label=i18n_stored.get_value().t("llm-total-requests")
                                        value=m.summary.total_requests.to_string()
                                        delta=Some(format!("{:.1}% {}", m.summary.success_rate_percent, i18n_stored.get_value().t("llm-success-rate")))
                                    />
                                    <MetricCard
                                        label=i18n_stored.get_value().t("llm-successful")
                                        value=m.summary.successful_requests.to_string()
                                        delta=None
                                    />
                                    <MetricCard
                                        label=i18n_stored.get_value().t("llm-failed")
                                        value=m.summary.failed_requests.to_string()
                                        delta=None
                                    />
                                    <MetricCard
                                        label=i18n_stored.get_value().t("llm-total-tokens")
                                        value=m.tokens.total_tokens.to_string()
                                        delta=Some(format!("{} {} / {} {}", m.tokens.input_tokens, i18n_stored.get_value().t("llm-input"), m.tokens.output_tokens, i18n_stored.get_value().t("llm-output")))
                                    />
                                </div>
                                <h3>{move || i18n_stored.get_value().t("llm-latency")}</h3>
                                <div class="latency-bars">
                                    <LatencyBar label=i18n_stored.get_value().t("llm-avg") value=m.latency.average_ms max=1000.0 />
                                    <LatencyBar label=i18n_stored.get_value().t("llm-p50") value=m.latency.p50_ms max=1000.0 />
                                    <LatencyBar label=i18n_stored.get_value().t("llm-p95") value=m.latency.p95_ms max=1000.0 />
                                    <LatencyBar label=i18n_stored.get_value().t("llm-p99") value=m.latency.p99_ms max=1000.0 />
                                </div>

                                <h3>{move || i18n_stored.get_value().t("llm-visual-overview")}</h3>
                                <div class="charts-grid">
                                    <PieChart
                                        title=i18n_stored.get_value().t("llm-request-distribution")
                                        labels=vec![i18n_stored.get_value().t("llm-success"), i18n_stored.get_value().t("llm-failed")]
                                        values=vec![m.summary.successful_requests as f64, m.summary.failed_requests as f64]
                                    />
                                    <PieChart
                                        title=i18n_stored.get_value().t("llm-token-usage")
                                        labels=vec![i18n_stored.get_value().t("llm-input"), i18n_stored.get_value().t("llm-output")]
                                        values=vec![m.tokens.input_tokens as f64, m.tokens.output_tokens as f64]
                                    />
                                    <BarChart
                                        title=i18n_stored.get_value().t("llm-latency-percentiles")
                                        labels=vec![i18n_stored.get_value().t("llm-avg"), i18n_stored.get_value().t("llm-p50"), i18n_stored.get_value().t("llm-p95"), i18n_stored.get_value().t("llm-p99")]
                                        values=vec![m.latency.average_ms, m.latency.p50_ms, m.latency.p95_ms, m.latency.p99_ms]
                                    />
                                </div>
                            </section>
                        })}
                    </div>
                }.into_any()
            }}
        </div>
    }
}

#[component]
fn MetricCard(
    label: String,
    #[prop(into)] value: String,
    delta: Option<String>,
) -> impl IntoView {
    view! {
        <div class="metric-card">
            <div class="metric-value">{value}</div>
            <div class="metric-label">{label}</div>
            {delta.map(|d| view! { <div class="metric-delta">{d}</div> })}
        </div>
    }
}

#[component]
fn LatencyBar(
    label: String,
    value: f64,
    max: f64,
) -> impl IntoView {
    let pct = (value / max * 100.0).min(100.0);
    let color_class = if pct < 30.0 {
        "latency-good"
    } else if pct < 70.0 {
        "latency-warning"
    } else {
        "latency-danger"
    };

    view! {
        <div class="latency-bar">
            <span class="latency-label">{label}</span>
            <span class="latency-value">{format!("{:.0}ms", value)}</span>
            <div class="latency-track">
                <div class=format!("latency-fill {}", color_class) style=format!("width: {}%", pct)></div>
            </div>
        </div>
    }
}
