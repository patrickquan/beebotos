//! Channels Management Page (QwenPaw-style)
//!
//! 卡片网格 + 右侧抽屉编辑 + 筛选 Tab + 状态指示

use std::collections::HashMap;

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos::view;
use wasm_bindgen::JsCast;

use crate::api::{
    ChannelConfig, ChannelQrcodeResponse, ChannelQrcodeStatusResponse, ChannelService,
};
use crate::components::InlineLoading;
use crate::i18n::I18nContext;
use crate::state::use_auth_state;

// ==================== 常量 ====================

const BUILTIN_ORDER: &[&str] = &[
    "console", "dingtalk", "feishu", "imessage", "discord",
    "telegram", "qq", "matrix", "sip", "xiaoyi", "wecom",
    "weixin", "mqtt", "mattermost", "onebot", "voice", "webchat",
];

const CHANNELS_WITH_ACCESS_CONTROL: &[&str] = &[
    "dingtalk", "feishu", "wecom", "weixin", "telegram",
    "discord", "qq", "matrix", "mattermost", "onebot",
];

const CHANNELS_WITH_QRCODE: &[&str] = &["weixin", "dingtalk", "wecom"];

// ==================== 辅助函数 ====================

fn channel_display_name(key: &str) -> String {
    match key {
        "console" => "控制台".to_string(),
        "dingtalk" => "钉钉".to_string(),
        "feishu" => "飞书".to_string(),
        "wecom" => "企业微信".to_string(),
        "weixin" => "微信".to_string(),
        "telegram" => "Telegram".to_string(),
        "discord" => "Discord".to_string(),
        "qq" => "QQ".to_string(),
        "matrix" => "Matrix".to_string(),
        "mattermost" => "Mattermost".to_string(),
        "mqtt" => "MQTT".to_string(),
        "imessage" => "iMessage".to_string(),
        "onebot" => "OneBot".to_string(),
        "voice" => "语音".to_string(),
        "sip" => "SIP".to_string(),
        "xiaoyi" => "小艺".to_string(),
        "webchat" => "WebChat".to_string(),
        _ => {
            let mut result = String::new();
            let mut capitalize = true;
            for c in key.chars() {
                if c == '_' || c == '-' {
                    capitalize = true;
                } else if capitalize {
                    result.push(c.to_ascii_uppercase());
                    capitalize = false;
                } else {
                    result.push(c);
                }
            }
            result
        }
    }
}

fn channel_description(key: &str) -> String {
    match key {
        "console" => "本地控制台调试输出".to_string(),
        "dingtalk" => "钉钉群机器人与 Stream 协议".to_string(),
        "feishu" => "飞书开放平台的 WebSocket 客户端".to_string(),
        "wecom" => "企业微信自建应用".to_string(),
        "weixin" => "微信 iLink 个人号协议".to_string(),
        "telegram" => "Telegram Bot API 长轮询".to_string(),
        "discord" => "Discord Gateway WebSocket".to_string(),
        "qq" => "QQ Bot 官方 Gateway".to_string(),
        "matrix" => "Matrix 协议（支持 E2EE）".to_string(),
        "mattermost" => "Mattermost WS + REST".to_string(),
        "mqtt" => "MQTT 发布/订阅".to_string(),
        "imessage" => "macOS iMessage 本地数据库轮询".to_string(),
        "onebot" => "OneBot 反向 WebSocket".to_string(),
        "voice" => "Twilio ConversationRelay 电话".to_string(),
        "sip" => "SIP 协议电话（LiveKit/pyVoIP）".to_string(),
        "xiaoyi" => "华为小艺 A2A 协议".to_string(),
        "webchat" => "Web 管理后台内置聊天".to_string(),
        _ => format!("{} 频道", key),
    }
}

fn channel_emoji_icon(key: &str) -> &'static str {
    match key {
        "weixin" | "wecom" => "💬",
        "webchat" => "🌐",
        "dingtalk" => "💼",
        "feishu" => "🚀",
        "telegram" => "✈️",
        "discord" => "🎮",
        "qq" => "🐧",
        "matrix" => "🔷",
        "mqtt" => "📡",
        "imessage" => "💬",
        "onebot" => "🤖",
        "voice" => "📞",
        "sip" => "☎️",
        "xiaoyi" => "🌸",
        "console" => "🖥️",
        "mattermost" => "💻",
        _ => "📡",
    }
}

fn is_builtin(key: &str) -> bool {
    BUILTIN_ORDER.contains(&key)
}

fn get_builtin_order_index(key: &str) -> usize {
    BUILTIN_ORDER.iter().position(|k| *k == key).unwrap_or(999)
}

fn event_target_value(ev: &leptos::ev::Event) -> String {
    ev.target()
        .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
        .map(|i| i.value())
        .unwrap_or_default()
}

fn event_target_checked(ev: &leptos::ev::Event) -> bool {
    ev.target()
        .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
        .map(|i| i.checked())
        .unwrap_or(false)
}

fn event_target_selected_value(ev: &leptos::ev::Event) -> String {
    ev.target()
        .and_then(|t| t.dyn_into::<web_sys::HtmlSelectElement>().ok())
        .map(|s| s.value())
        .unwrap_or_default()
}

// ==================== 主页面组件 ====================

#[component]
pub fn ChannelsPage() -> impl IntoView {
    let i18n = use_context::<I18nContext>().expect("i18n context not found");
    let i18n_stored = StoredValue::new(i18n);

    // API 客户端
    let auth_state = use_auth_state();
    let auth_state_stored = StoredValue::new(auth_state);

    // 创建带认证的 ChannelService
    let make_service = move || {
        let client = crate::api::create_client();
        client.set_auth_token(auth_state_stored.get_value().get_token());
        ChannelService::new(client)
    };

    // 数据状态
    let (channels, set_channels) = signal::<HashMap<String, ChannelConfig>>(HashMap::new());
    let (channel_types, set_channel_types) = signal::<Vec<String>>(Vec::new());
    let (loading, set_loading) = signal(true);
    let (error_msg, set_error_msg) = signal::<Option<String>>(None);

    // 筛选状态
    let (filter, set_filter) = signal("all".to_string());

    // 抽屉状态
    let (drawer_open, set_drawer_open) = signal(false);
    let (selected_key, set_selected_key) = signal::<Option<String>>(None);

    // 二维码状态
    let (qr_data, set_qr_data) = signal::<Option<ChannelQrcodeResponse>>(None);
    let (qr_loading, set_qr_loading) = signal(false);
    let (qr_status, set_qr_status) = signal::<Option<ChannelQrcodeStatusResponse>>(None);
    let (qr_polling, set_qr_polling) = signal(false);

    // Toast 提示
    let (toast_msg, set_toast_msg) = signal::<Option<(String, bool)>>(None);

    // 加载数据
    let fetch_data = move || {
        set_loading.set(true);
        set_error_msg.set(None);
        let service = make_service();
        spawn_local(async move {
            match service.list_channels_config().await {
                Ok(data) => {
                    set_channels.set(data);
                }
                Err(e) => {
                    set_error_msg.set(Some(format!("加载频道配置失败: {}", e)));
                }
            }
            match service.list_channel_types().await {
                Ok(types_resp) => {
                    set_channel_types.set(types_resp.types);
                }
                Err(e) => {
                    web_sys::console::error_1(&format!("加载频道类型失败: {}", e).into());
                }
            }
            set_loading.set(false);
        });
    };

    // 初始加载
    Effect::new(move |_| {
        fetch_data();
    });

    // 计算排序后的 key 列表
    let ordered_keys = Memo::new(move |_| {
        let all_types = channel_types.get();
        let mut keys: Vec<String> = channels
            .get()
            .keys()
            .cloned()
            .collect();

        // 补充 channel_types 中但尚未在 channels 中的类型
        for t in &all_types {
            if !keys.contains(t) {
                keys.push(t.clone());
            }
        }

        // 按 builtinOrder 排序
        keys.sort_by_key(|k| get_builtin_order_index(k));
        keys
    });

    // 根据筛选条件过滤并排序卡片
    let filtered_cards = Memo::new(move |_| {
        let all_keys = ordered_keys.get();
        let chs = channels.get();
        let f = filter.get();
        let mut enabled_cards: Vec<(String, ChannelConfig)> = Vec::new();
        let mut disabled_cards: Vec<(String, ChannelConfig)> = Vec::new();

        for key in all_keys {
            let builtin = is_builtin(&key);
            if f == "builtin" && !builtin {
                continue;
            }
            if f == "custom" && builtin {
                continue;
            }
            let config = chs.get(&key).cloned().unwrap_or_default();
            if config.enabled {
                enabled_cards.push((key, config));
            } else {
                disabled_cards.push((key, config));
            }
        }

        enabled_cards.extend(disabled_cards);
        enabled_cards
    });

    // 打开抽屉
    let open_drawer = move |key: String| {
        let chs = channels.get();
        let _config = chs.get(&key).cloned().unwrap_or_default();
        set_selected_key.set(Some(key));
        set_drawer_open.set(true);
        set_qr_data.set(None);
        set_qr_status.set(None);
        set_qr_polling.set(false);
    };

    // 关闭抽屉
    let close_drawer = move || {
        set_drawer_open.set(false);
        set_qr_polling.set(false);
        set_qr_data.set(None);
        set_qr_status.set(None);
    };

    // 保存配置
    let save_config = move |key: String, config: ChannelConfig| {
        let service = make_service();
        spawn_local(async move {
            match service.update_channel_config(&key, &config).await {
                Ok(_) => {
                    set_toast_msg.set(Some(("配置已保存".to_string(), true)));
                    close_drawer();
                    // 刷新数据
                    fetch_data();
                }
                Err(e) => {
                    set_toast_msg.set(Some((format!("保存失败: {}", e), false)));
                }
            }
        });
    };

    // 获取二维码
    let get_qrcode = move |channel: String| {
        set_qr_loading.set(true);
        set_qr_data.set(None);
        set_qr_status.set(None);
        set_qr_polling.set(false);
        let service = make_service();
        spawn_local(async move {
            match service.get_channel_qrcode(&channel).await {
                Ok(qr) => {
                    set_qr_data.set(Some(qr.clone()));
                    set_qr_loading.set(false);
                    // 开始轮询
                    set_qr_polling.set(true);
                    let poll_service = service.clone();
                    spawn_local(async move {
                        loop {
                            gloo_timers::future::TimeoutFuture::new(2000).await;
                            if !qr_polling.get() {
                                break;
                            }
                            match poll_service.get_channel_qrcode_status(&channel, &qr.poll_token).await {
                                Ok(status) => {
                                    let should_stop = status.status == "confirmed" || status.status == "expired";
                                    set_qr_status.set(Some(status));
                                    if should_stop {
                                        set_qr_polling.set(false);
                                        break;
                                    }
                                }
                                Err(e) => {
                                    web_sys::console::error_1(&format!("轮询二维码状态失败: {}", e).into());
                                    set_qr_polling.set(false);
                                    break;
                                }
                            }
                        }
                    });
                }
                Err(e) => {
                    set_qr_loading.set(false);
                    web_sys::console::error_1(&format!("获取二维码失败: {}", e).into());
                }
            }
        });
    };

    // 清除 toast
    Effect::new(move |_| {
        if toast_msg.get().is_some() {
            spawn_local(async move {
                gloo_timers::future::TimeoutFuture::new(3000).await;
                set_toast_msg.set(None);
            });
        }
    });

    view! {
        <div class="channels-page">
            // 频道页专用样式
            <style>
                ".channels-page {
                    padding: 12px 20px 20px;
                    max-width: 100%;
                    overflow: hidden;
                }"
                ".channels-top-bar {
                    display: flex;
                    align-items: center;
                    justify-content: space-between;
                    margin-bottom: 0;
                    padding-bottom: 8px;
                    border-bottom: 1px solid var(--border-color);
                }"
                ".filter-tabs {
                    display: flex;
                    gap: 4px;
                    background: rgba(255,255,255,0.04);
                    padding: 4px;
                    border-radius: 8px;
                    width: fit-content;
                }"
                ".filter-tab {
                    padding: 6px 16px;
                    border-radius: 6px;
                    font-size: 13px;
                    cursor: pointer;
                    transition: all 0.18s ease;
                    color: var(--text-secondary);
                    background: transparent;
                    border: none;
                }"
                ".filter-tab:hover {
                    color: var(--text-primary);
                }"
                ".filter-tab.active {
                    background: rgba(255,255,255,0.08);
                    color: var(--text-primary);
                    box-shadow: 0 1px 4px rgba(0,0,0,0.2);
                }"
                ".channels-grid {
                    display: grid;
                    grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
                    gap: 12px;
                    overflow-y: auto;
                    max-height: calc(100vh - 160px);
                    padding-top: 12px;
                }"
                ".channel-card {
                    min-height: 140px;
                    padding: 16px;
                    border-radius: 8px;
                    background: var(--bg-card);
                    border: 1px solid var(--border-color);
                    transition: all 0.2s ease-in-out;
                    cursor: pointer;
                    display: flex;
                    flex-direction: column;
                    gap: 12px;
                }"
                ".channel-card:hover {
                    border-color: rgba(148, 163, 184, 0.3);
                    box-shadow: 0 2px 8px rgba(0,0,0,0.3);
                }"
                ".channel-card-header {
                    display: flex;
                    align-items: center;
                    justify-content: space-between;
                }"
                ".channel-card-icon {
                    font-size: 32px;
                    width: 48px;
                    height: 48px;
                    display: flex;
                    align-items: center;
                    justify-content: center;
                    border-radius: 10px;
                    background: rgba(255,255,255,0.05);
                }"
                ".channel-card-status {
                    display: flex;
                    align-items: center;
                    gap: 6px;
                }"
                ".status-dot {
                    width: 6px;
                    height: 6px;
                    border-radius: 50%;
                }"
                ".status-dot.enabled {
                    background-color: #14B8A6;
                }"
                ".status-dot.disabled {
                    background-color: #64748b;
                }"
                ".status-text {
                    font-size: 12px;
                }"
                ".status-text.enabled {
                    color: #14B8A6;
                }"
                ".status-text.disabled {
                    color: #64748b;
                }"
                ".channel-card-body {
                    flex: 1;
                }"
                ".channel-card-name {
                    font-size: 14px;
                    font-weight: 500;
                    margin-bottom: 4px;
                    display: flex;
                    align-items: center;
                    gap: 8px;
                }"
                ".channel-chip {
                    font-size: 11px;
                    padding: 2px 8px;
                    border-radius: 4px;
                    background: rgba(255,255,255,0.06);
                    color: var(--text-muted);
                }"
                ".channel-card-desc {
                    font-size: 12px;
                    color: var(--text-muted);
                }"
                ".channel-card-footer {
                    font-size: 12px;
                    color: var(--text-secondary);
                }"
                ".drawer-overlay {
                    position: fixed;
                    top: 0;
                    left: 0;
                    right: 0;
                    bottom: 0;
                    background: rgba(0,0,0,0.5);
                    z-index: 1000;
                    opacity: 0;
                    transition: opacity 0.3s ease;
                    pointer-events: none;
                }"
                ".drawer-overlay.open {
                    opacity: 1;
                    pointer-events: auto;
                }"
                ".drawer-panel {
                    position: fixed;
                    top: 0;
                    right: 0;
                    bottom: 0;
                    width: 420px;
                    max-width: 90vw;
                    background: var(--bg-dark);
                    border-left: 1px solid var(--border-color);
                    z-index: 1001;
                    transform: translateX(100%);
                    transition: transform 0.3s cubic-bezier(0.7, 0.3, 0.1, 1);
                    display: flex;
                    flex-direction: column;
                }"
                ".drawer-panel.open {
                    transform: translateX(0);
                }"
                ".drawer-header {
                    padding: 20px;
                    border-bottom: 1px solid var(--border-color);
                    display: flex;
                    align-items: center;
                    justify-content: space-between;
                }"
                ".drawer-header h3 {
                    font-size: 16px;
                    font-weight: 600;
                }"
                ".drawer-close-btn {
                    background: none;
                    border: none;
                    color: var(--text-secondary);
                    font-size: 20px;
                    cursor: pointer;
                    padding: 4px;
                }"
                ".drawer-body {
                    flex: 1;
                    overflow-y: auto;
                    padding: 20px;
                }"
                ".drawer-footer {
                    padding: 16px 20px;
                    border-top: 1px solid var(--border-color);
                    display: flex;
                    gap: 12px;
                    justify-content: flex-end;
                }"
                ".form-section {
                    margin-bottom: 24px;
                }"
                ".form-section-title {
                    font-size: 13px;
                    font-weight: 600;
                    margin-bottom: 12px;
                    color: var(--text-secondary);
                }"
                ".form-group {
                    margin-bottom: 16px;
                }"
                ".form-group label {
                    display: block;
                    font-size: 13px;
                    margin-bottom: 6px;
                    color: var(--text-secondary);
                }"
                ".form-group input[type=\"text\"],
                .form-group input[type=\"password\"],
                .form-group select {
                    width: 100%;
                    padding: 8px 12px;
                    border-radius: 6px;
                    border: 1px solid var(--border-color);
                    background: var(--bg-glass);
                    color: var(--text-primary);
                    font-size: 13px;
                    outline: none;
                    transition: border-color 0.2s;
                }"
                ".form-group input:focus,
                .form-group select:focus {
                    border-color: var(--primary-color);
                }"
                ".form-group input[type=\"checkbox\"] {
                    margin-right: 8px;
                }"
                ".checkbox-label {
                    display: flex;
                    align-items: center;
                    cursor: pointer;
                    font-size: 13px;
                }"
                ".btn-primary {
                    padding: 8px 20px;
                    border-radius: 6px;
                    background: var(--primary-color);
                    color: white;
                    border: none;
                    font-size: 13px;
                    font-weight: 500;
                    cursor: pointer;
                    transition: background 0.2s;
                }"
                ".btn-primary:hover {
                    background: var(--primary-hover);
                }"
                ".btn-secondary {
                    padding: 8px 20px;
                    border-radius: 6px;
                    background: transparent;
                    color: var(--text-secondary);
                    border: 1px solid var(--border-color);
                    font-size: 13px;
                    cursor: pointer;
                    transition: all 0.2s;
                }"
                ".btn-secondary:hover {
                    background: var(--bg-hover);
                    color: var(--text-primary);
                }"
                ".qr-container {
                    text-align: center;
                    padding: 16px;
                    background: rgba(255,255,255,0.03);
                    border-radius: 8px;
                    margin-top: 12px;
                }"
                ".qr-container img {
                    max-width: 200px;
                    border-radius: 8px;
                }"
                ".qr-status {
                    margin-top: 8px;
                    font-size: 12px;
                }"
                ".qr-status.confirmed { color: #14B8A6; }"
                ".qr-status.expired { color: #ef4444; }"
                ".qr-status.pending { color: var(--text-muted); }"
                ".toast {
                    position: fixed;
                    top: 20px;
                    right: 20px;
                    padding: 12px 20px;
                    border-radius: 8px;
                    font-size: 13px;
                    z-index: 2000;
                    animation: toastIn 0.3s ease;
                }"
                ".toast.success {
                    background: rgba(20, 184, 166, 0.15);
                    color: #14B8A6;
                    border: 1px solid rgba(20, 184, 166, 0.3);
                }"
                ".toast.error {
                    background: rgba(239, 68, 68, 0.15);
                    color: #ef4444;
                    border: 1px solid rgba(239, 68, 68, 0.3);
                }"
                "@keyframes toastIn {
                    from { transform: translateX(100%); opacity: 0; }
                    to { transform: translateX(0); opacity: 1; }
                }"
                ".loading-text {
                    text-align: center;
                    padding: 60px;
                    color: var(--text-muted);
                }"
                ".doc-link {
                    font-size: 12px;
                    color: var(--primary-color);
                    text-decoration: none;
                }"
                ".doc-link:hover {
                    text-decoration: underline;
                }"
                ".toggle-switch {
                    position: relative;
                    display: inline-block;
                    width: 44px;
                    height: 24px;
                    flex-shrink: 0;
                }"
                ".toggle-switch input {
                    opacity: 0;
                    width: 0;
                    height: 0;
                }"
                ".toggle-slider {
                    position: absolute;
                    cursor: pointer;
                    top: 0;
                    left: 0;
                    right: 0;
                    bottom: 0;
                    background-color: #475569;
                    transition: 0.3s;
                    border-radius: 24px;
                }"
                ".toggle-slider:before {
                    position: absolute;
                    content: '';
                    height: 18px;
                    width: 18px;
                    left: 3px;
                    bottom: 3px;
                    background-color: white;
                    transition: 0.3s;
                    border-radius: 50%;
                }"
                ".toggle-switch input:checked + .toggle-slider {
                    background-color: #FF7F16;
                }"
                ".toggle-switch input:checked + .toggle-slider:before {
                    transform: translateX(20px);
                }"
                ".label-row {
                    display: flex;
                    align-items: center;
                    gap: 6px;
                    margin-bottom: 8px;
                }"
                ".label-row label {
                    font-size: 13px;
                    color: var(--text-secondary);
                    margin-bottom: 0;
                }"
                ".info-icon {
                    position: relative;
                    display: inline-flex;
                    align-items: center;
                    justify-content: center;
                    width: 16px;
                    height: 16px;
                    border-radius: 50%;
                    background: rgba(255,255,255,0.1);
                    color: var(--text-muted);
                    font-size: 11px;
                    cursor: help;
                    flex-shrink: 0;
                }"
                ".info-tooltip {
                    position: absolute;
                    bottom: 24px;
                    left: 50%;
                    transform: translateX(-50%);
                    background: var(--bg-dark);
                    border: 1px solid var(--border-color);
                    border-radius: 6px;
                    padding: 8px 12px;
                    font-size: 12px;
                    color: var(--text-secondary);
                    white-space: nowrap;
                    z-index: 10;
                    opacity: 0;
                    visibility: hidden;
                    transition: all 0.2s;
                    box-shadow: 0 4px 12px rgba(0,0,0,0.3);
                }"
                ".info-icon:hover .info-tooltip {
                    opacity: 1;
                    visibility: visible;
                }"
                ".alert-box {
                    padding: 12px 16px;
                    border-radius: 8px;
                    font-size: 12px;
                    line-height: 1.6;
                    margin-bottom: 16px;
                    display: flex;
                    align-items: flex-start;
                    gap: 10px;
                }"
                ".alert-box.info {
                    background: rgba(59, 130, 246, 0.1);
                    border: 1px solid rgba(59, 130, 246, 0.2);
                    color: #60a5fa;
                }"
                ".alert-box.warning {
                    background: rgba(245, 158, 11, 0.1);
                    border: 1px solid rgba(245, 158, 11, 0.2);
                    color: #fbbf24;
                }"
                ".alert-icon {
                    font-size: 14px;
                    flex-shrink: 0;
                    margin-top: 1px;
                }"
                ".alert-content {
                    flex: 1;
                }"
                ".password-wrapper {
                    position: relative;
                    display: flex;
                    align-items: center;
                }"
                ".password-wrapper input {
                    padding-right: 36px;
                }"
                ".eye-icon {
                    position: absolute;
                    right: 10px;
                    background: none;
                    border: none;
                    color: var(--text-muted);
                    cursor: pointer;
                    font-size: 14px;
                    padding: 2px;
                }"
                ".eye-icon:hover {
                    color: var(--text-primary);
                }"
                ".shield-icon {
                    color: #60a5fa;
                    font-size: 14px;
                }"
                ".qr-section {
                    margin-bottom: 20px;
                }"
                ".qr-btn {
                    width: 100%;
                    padding: 10px;
                    border-radius: 8px;
                    background: #FF7F16;
                    color: white;
                    border: none;
                    font-size: 14px;
                    font-weight: 500;
                    cursor: pointer;
                    transition: background 0.2s;
                    margin-bottom: 12px;
                }"
                ".qr-btn:hover {
                    background: #e66d0a;
                }"
                ".qr-tip {
                    font-size: 12px;
                    color: var(--text-muted);
                    text-align: center;
                    margin-top: 8px;
                }"
                ".drawer-title-row {
                    display: flex;
                    align-items: center;
                    gap: 12px;
                }"
                ".drawer-title-link {
                    font-size: 12px;
                    color: var(--primary-color);
                    text-decoration: none;
                    display: flex;
                    align-items: center;
                    gap: 4px;
                }"
                ".form-group-compact {
                    margin-bottom: 12px;
                }"
                ".toggle-row {
                    display: flex;
                    align-items: center;
                    justify-content: space-between;
                    padding: 4px 0;
                }"
                ".toggle-row .toggle-label {
                    display: flex;
                    align-items: center;
                    gap: 6px;
                    font-size: 13px;
                    color: var(--text-secondary);
                }"
                ".btn-save {
                    padding: 8px 24px;
                    border-radius: 6px;
                    background: #FF7F16;
                    color: white;
                    border: none;
                    font-size: 13px;
                    font-weight: 500;
                    cursor: pointer;
                    transition: background 0.2s;
                }"
                ".btn-save:hover {
                    background: #e66d0a;
                }"
                ".btn-cancel {
                    padding: 8px 24px;
                    border-radius: 6px;
                    background: transparent;
                    color: var(--text-secondary);
                    border: 1px solid var(--border-color);
                    font-size: 13px;
                    cursor: pointer;
                    transition: all 0.2s;
                }"
                ".btn-cancel:hover {
                    background: var(--bg-hover);
                    color: var(--text-primary);
                }"
            </style>

            // Toast 提示
            {move || toast_msg.get().map(|(msg, is_success)| view! {
                <div class={format!("toast {}", if is_success { "success" } else { "error" })}>{msg}</div>
            })}

            // 顶部栏（筛选标签）
            <div class="channels-top-bar">
                <div class="filter-tabs">
                <button
                    class={move || format!("filter-tab {}", if filter.get() == "all" { "active" } else { "" })}
                    on:click=move |_| set_filter.set("all".to_string())
                >
                    {move || i18n_stored.get_value().t("channels-filter-all")}
                </button>
                <button
                    class={move || format!("filter-tab {}", if filter.get() == "builtin" { "active" } else { "" })}
                    on:click=move |_| set_filter.set("builtin".to_string())
                >
                    {move || i18n_stored.get_value().t("channels-filter-builtin")}
                </button>
                <button
                    class={move || format!("filter-tab {}", if filter.get() == "custom" { "active" } else { "" })}
                    on:click=move |_| set_filter.set("custom".to_string())
                >
                    {move || i18n_stored.get_value().t("channels-filter-custom")}
                </button>
                </div>
            </div>

            // 错误提示
            {move || error_msg.get().map(|msg| view! {
                <div class="alert alert-error">{msg}</div>
            })}

            // 加载态
            {move || if loading.get() {
                view! { <div class="loading-text">{i18n_stored.get_value().t("channels-loading")}</div> }.into_any()
            } else {
                // 卡片网格
                let cards = filtered_cards.get();
                if cards.is_empty() {
                    view! { <div class="loading-text">{i18n_stored.get_value().t("channels-empty")}</div> }.into_any()
                } else {
                    view! {
                        <div class="channels-grid">
                            {cards.into_iter().map(|(key, config)| {
                                let key_for_click = key.clone();
                                let is_enabled = config.enabled;
                                let bot_prefix = config.bot_prefix.clone().filter(|s| !s.is_empty());
                                let builtin = is_builtin(&key);

                                view! {
                                    <div
                                        class="channel-card"
                                        on:click=move |_| open_drawer(key_for_click.clone())
                                    >
                                        <div class="channel-card-header">
                                            <div class="channel-card-icon">
                                                {channel_emoji_icon(&key)}
                                            </div>
                                            <div class="channel-card-status">
                                                <span class={format!("status-dot {}", if is_enabled { "enabled" } else { "disabled" })} />
                                                <span class={format!("status-text {}", if is_enabled { "enabled" } else { "disabled" })}>
                                                    {if is_enabled {
                                                        i18n_stored.get_value().t("channels-status-enabled")
                                                    } else {
                                                        i18n_stored.get_value().t("channels-status-disabled")
                                                    }}
                                                </span>
                                            </div>
                                        </div>
                                        <div class="channel-card-body">
                                            <div class="channel-card-name">
                                                {channel_display_name(&key)}
                                                <span class="channel-chip">
                                                    {if builtin {
                                                        i18n_stored.get_value().t("channels-chip-builtin")
                                                    } else {
                                                        i18n_stored.get_value().t("channels-chip-custom")
                                                    }}
                                                </span>
                                            </div>
                                            <div class="channel-card-desc">{channel_description(&key)}</div>
                                        </div>
                                        <div class="channel-card-footer">
                                            {match bot_prefix {
                                                Some(prefix) => format!("Bot Prefix: {}", prefix),
                                                None => i18n_stored.get_value().t("channels-not-set"),
                                            }}
                                        </div>
                                    </div>
                                }
                            }).collect_view()}
                        </div>
                    }.into_any()
                }
            }}

            // 抽屉
            {move || {
                let open = drawer_open.get();
                let key = selected_key.get();
                if !open || key.is_none() {
                    return None;
                }
                let key = key.unwrap();
                let chs = channels.get();
                let config = chs.get(&key).cloned().unwrap_or_default();

                // 表单状态（复制当前配置）
                let (form_enabled, set_form_enabled) = signal(config.enabled);
                let (form_prefix, set_form_prefix) = signal(config.bot_prefix.clone().unwrap_or_default());
                let (form_filter_tool, set_form_filter_tool) = signal(config.filter_tool_messages.unwrap_or(false));
                let (form_filter_think, set_form_filter_think) = signal(config.filter_thinking.unwrap_or(false));
                let (form_dm_policy, set_form_dm_policy) = signal(config.dm_policy.clone().unwrap_or_else(|| "open".to_string()));
                let (form_group_policy, set_form_group_policy) = signal(config.group_policy.clone().unwrap_or_else(|| "open".to_string()));
                let (form_require_mention, set_form_require_mention) = signal(config.require_mention.unwrap_or(false));
                let (form_allow_from, set_form_allow_from) = signal(config.allow_from.clone().unwrap_or_default().join(", "));
                // 平台专属字段
                let (form_base_url, set_form_base_url) = signal(config.base_url.clone().unwrap_or_default());
                let (form_bot_token, set_form_bot_token) = signal(config.bot_token.clone().unwrap_or_default());
                let (form_client_id, set_form_client_id) = signal(config.client_id.clone().unwrap_or_default());
                let (form_client_secret, set_form_client_secret) = signal(config.client_secret.clone().unwrap_or_default());
                let (form_app_id, set_form_app_id) = signal(config.app_id.clone().unwrap_or_default());
                let (form_app_secret, set_form_app_secret) = signal(config.app_secret.clone().unwrap_or_default());
                let (form_webhook_url, set_form_webhook_url) = signal(config.webhook_url.clone().unwrap_or_default());
                let (form_api_key, set_form_api_key) = signal(config.api_key.clone().unwrap_or_default());
                let (form_api_secret, _set_form_api_secret) = signal(config.api_secret.clone().unwrap_or_default());
                let (form_region, set_form_region) = signal(config.region.clone().unwrap_or_else(|| "china".to_string()));
                let (form_message_type, set_form_message_type) = signal(config.message_type.clone().unwrap_or_else(|| "text".to_string()));
                let (form_auto_reconnect, set_form_auto_reconnect) = signal(config.auto_reconnect.unwrap_or(true));
                let (form_bot_base_url, set_form_bot_base_url) = signal(config.bot_base_url.clone().unwrap_or_default());
                let (form_reconnect_interval, set_form_reconnect_interval) = signal(config.reconnect_interval_secs.unwrap_or(300).to_string());
                let (form_warning_before, set_form_warning_before) = signal(config.warning_before_secs.unwrap_or(7200).to_string());
                let (form_force_before, set_form_force_before) = signal(config.force_before_secs.unwrap_or(1800).to_string());
                let (form_message_merge, set_form_message_merge) = signal(config.message_merge_enabled.unwrap_or(true));
                let (form_merge_delay, set_form_merge_delay) = signal(config.message_merge_delay_ms.unwrap_or(2000).to_string());
                let (form_media_dir, set_form_media_dir) = signal(config.media_dir.clone().unwrap_or_default());
                // Token 显示/隐藏开关
                let (show_token, set_show_token) = signal(false);

                let key_for_save = key.clone();
                let key_for_qr = key.clone();
                let has_access_control = CHANNELS_WITH_ACCESS_CONTROL.contains(&key.as_str());
                let has_qrcode = CHANNELS_WITH_QRCODE.contains(&key.as_str());

                // 扫码成功后自动填充 Bot Token 和 Base URL
                Effect::new(move |_| {
                    if let Some(status) = qr_status.get() {
                        if status.status == "confirmed" {
                            if let Some(token) = status.bot_token {
                                if !token.is_empty() && form_bot_token.get().is_empty() {
                                    set_form_bot_token.set(token);
                                }
                            }
                            if let Some(base) = status.base_url {
                                if !base.is_empty() && form_base_url.get().is_empty() {
                                    set_form_base_url.set(base);
                                }
                            }
                        }
                    }
                });

                Some(view! {
                    <>
                        // 遮罩层
                        <div
                            class={move || format!("drawer-overlay {}", if drawer_open.get() { "open" } else { "" })}
                            on:click=move |_| close_drawer()
                        />
                        // 抽屉面板
                        <div class={move || format!("drawer-panel {}", if drawer_open.get() { "open" } else { "" })}>
                            // 抽屉头部
                            <div class="drawer-header">
                                <div class="drawer-title-row">
                                    <h3>{channel_display_name(&key)} {i18n_stored.get_value().t("channels-settings")}</h3>
                                    {if key == "weixin" {
                                        Some(view! {
                                            <a
                                                class="drawer-title-link"
                                                href="https://ilinkai.weixin.qq.com"
                                                target="_blank"
                                            >
                                                {i18n_stored.get_value().t("channels-weixin-doc")}
                                                " ↗"
                                            </a>
                                        })
                                    } else {
                                        None
                                    }}
                                </div>
                                <button class="drawer-close-btn" on:click=move |_| close_drawer()>"✕"</button>
                            </div>

                            // 抽屉内容
                            <div class="drawer-body">
                                // 基础设置
                                <div class="form-section">
                                    <div class="form-section-title">{i18n_stored.get_value().t("channels-basic-settings")}</div>

                                    <div class="form-group-compact">
                                        <div class="toggle-row">
                                            <span class="toggle-label">{i18n_stored.get_value().t("channels-enabled")}</span>
                                            <label class="toggle-switch">
                                                <input
                                                    type="checkbox"
                                                    checked={move || form_enabled.get()}
                                                    on:change=move |e| set_form_enabled.set(event_target_checked(&e))
                                                />
                                                <span class="toggle-slider"></span>
                                            </label>
                                        </div>
                                    </div>

                                    <div class="form-group">
                                        <div class="label-row">
                                            <label>{i18n_stored.get_value().t("channels-bot-prefix")}</label>
                                        </div>
                                        <input
                                            type="text"
                                            placeholder="@bot"
                                            prop:value=move || form_prefix.get()
                                            on:input=move |e| set_form_prefix.set(event_target_value(&e))
                                        />
                                    </div>

                                    <div class="form-group-compact">
                                        <div class="toggle-row">
                                            <span class="toggle-label">
                                                {i18n_stored.get_value().t("channels-show-tool")}
                                                <span class="info-icon" title="控制是否显示工具调用相关的系统消息">"ⓘ"</span>
                                            </span>
                                            <label class="toggle-switch">
                                                <input
                                                    type="checkbox"
                                                    checked={move || form_filter_tool.get()}
                                                    on:change=move |e| set_form_filter_tool.set(event_target_checked(&e))
                                                />
                                                <span class="toggle-slider"></span>
                                            </label>
                                        </div>
                                    </div>

                                    <div class="form-group-compact">
                                        <div class="toggle-row">
                                            <span class="toggle-label">
                                                {i18n_stored.get_value().t("channels-show-thinking")}
                                                <span class="info-icon" title="控制是否显示模型的思考过程">"ⓘ"</span>
                                            </span>
                                            <label class="toggle-switch">
                                                <input
                                                    type="checkbox"
                                                    checked={move || form_filter_think.get()}
                                                    on:change=move |e| set_form_filter_think.set(event_target_checked(&e))
                                                />
                                                <span class="toggle-slider"></span>
                                            </label>
                                        </div>
                                    </div>
                                </div>

                                // 平台专属字段
                                <div class="form-section">
                                    <div class="form-section-title">{i18n_stored.get_value().t("channels-platform-fields")}</div>

                                    {move || match key.as_str() {
                                        "weixin" | "personal_wechat" => view! {
                                            <>
                                                // 蓝色提示框
                                                <div class="alert-box info"
                                                    title="iLink 协议说明"
                                                >
                                                    <span class="alert-icon">"ℹ️"</span>
                                                    <span class="alert-content">{i18n_stored.get_value().t("channels-ilink-desc")}</span>
                                                </div>
                                                // 黄色警告框
                                                <div class="alert-box warning"
                                                    title="平台限制警告"
                                                >
                                                    <span class="alert-icon">"⚠️"</span>
                                                    <span class="alert-content">{i18n_stored.get_value().t("channels-ilink-warning")}</span>
                                                </div>

                                                // Bot Token（带显示/隐藏）
                                                <div class="form-group">
                                                    <div class="label-row"
                                                        title="iLink Bot Token，扫码登录后自动获取"
                                                    >
                                                        <label>{i18n_stored.get_value().t("channels-bot-token")}</label>
                                                        <span class="info-icon">"ⓘ"</span>
                                                    </div>
                                                    <div class="password-wrapper">
                                                        <input
                                                            type={move || if show_token.get() { "text" } else { "password" }}
                                                            placeholder="••••••••"
                                                            prop:value=move || form_bot_token.get()
                                                            on:input=move |e| set_form_bot_token.set(event_target_value(&e))
                                                        />
                                                        <button
                                                            class="eye-icon"
                                                            on:click=move |_| set_show_token.set(!show_token.get())
                                                            title={move || if show_token.get() { "隐藏" } else { "显示" }}
                                                        >
                                                            {move || if show_token.get() { "🙈" } else { "👁️" }}
                                                        </button>
                                                    </div>
                                                </div>

                                                <div class="form-group">
                                                    <label>{i18n_stored.get_value().t("channels-media-dir")}</label>
                                                    <input type="text" placeholder="./data/media/weixin"
                                                        prop:value=move || form_media_dir.get()
                                                        on:input=move |e| set_form_media_dir.set(event_target_value(&e)) />
                                                </div>

                                                <div class="form-group">
                                                    <label>{i18n_stored.get_value().t("channels-base-url")}</label>
                                                    <input type="text" placeholder="https://ilinkai.weixin.qq.com"
                                                        prop:value=move || form_base_url.get()
                                                        on:input=move |e| set_form_base_url.set(event_target_value(&e)) />
                                                </div>

                                                <div class="form-group">
                                                    <label>{i18n_stored.get_value().t("channels-bot-base-url")}</label>
                                                    <input type="text" placeholder="https://ilinkai.weixin.qq.com"
                                                        prop:value=move || form_bot_base_url.get()
                                                        on:input=move |e| set_form_bot_base_url.set(event_target_value(&e)) />
                                                </div>

                                                // 消息合并 toggle
                                                <div class="form-group-compact">
                                                    <div class="toggle-row"
                                                        title="开启后，短时间内连续收到的消息会合并为一条"
                                                    >
                                                        <span class="toggle-label">
                                                            {i18n_stored.get_value().t("channels-message-merge")}
                                                            <span class="info-icon">"ⓘ"</span>
                                                        </span>
                                                        <label class="toggle-switch">
                                                            <input type="checkbox" checked={move || form_message_merge.get()}
                                                                on:change=move |e| set_form_message_merge.set(event_target_checked(&e)) />
                                                            <span class="toggle-slider"></span>
                                                        </label>
                                                    </div>
                                                </div>

                                                {move || if form_message_merge.get() {
                                                    Some(view! {
                                                        <div class="form-group">
                                                            <label>{i18n_stored.get_value().t("channels-merge-delay")}</label>
                                                            <input type="number" placeholder="2000"
                                                                prop:value=move || form_merge_delay.get()
                                                                on:input=move |e| set_form_merge_delay.set(event_target_value(&e)) />
                                                        </div>
                                                    })
                                                } else {
                                                    None
                                                }}
                                                <div class="form-group">
                                                    <label>{i18n_stored.get_value().t("channels-reconnect-interval")}</label>
                                                    <input type="number" placeholder="300"
                                                        prop:value=move || form_reconnect_interval.get()
                                                        on:input=move |e| set_form_reconnect_interval.set(event_target_value(&e)) />
                                                </div>
                                                <div class="form-group">
                                                    <label>{i18n_stored.get_value().t("channels-warning-before")}</label>
                                                    <input type="number" placeholder="7200"
                                                        prop:value=move || form_warning_before.get()
                                                        on:input=move |e| set_form_warning_before.set(event_target_value(&e)) />
                                                </div>
                                                <div class="form-group">
                                                    <label>{i18n_stored.get_value().t("channels-force-before")}</label>
                                                    <input type="number" placeholder="1800"
                                                        prop:value=move || form_force_before.get()
                                                        on:input=move |e| set_form_force_before.set(event_target_value(&e)) />
                                                </div>
                                            </>
                                        }.into_any(),
                                        "dingtalk" => view! {
                                            <>
                                                <div class="form-group">
                                                    <label>{i18n_stored.get_value().t("channels-client-id")}</label>
                                                    <input type="text"
                                                        prop:value=move || form_client_id.get()
                                                        on:input=move |e| set_form_client_id.set(event_target_value(&e)) />
                                                </div>
                                                <div class="form-group">
                                                    <label>{i18n_stored.get_value().t("channels-client-secret")}</label>
                                                    <input type="password" placeholder="••••••••"
                                                        prop:value=move || form_client_secret.get()
                                                        on:input=move |e| set_form_client_secret.set(event_target_value(&e)) />
                                                </div>
                                                <div class="form-group">
                                                    <label>{i18n_stored.get_value().t("channels-message-type")}</label>
                                                    <select
                                                        prop:value=move || form_message_type.get()
                                                        on:change=move |e| set_form_message_type.set(event_target_selected_value(&e))
                                                    >
                                                        <option value="text">{i18n_stored.get_value().t("channels-msg-type-text")}</option>
                                                        <option value="markdown">{i18n_stored.get_value().t("channels-msg-type-markdown")}</option>
                                                    </select>
                                                </div>
                                            </>
                                        }.into_any(),
                                        "feishu" => view! {
                                            <>
                                                <div class="form-group">
                                                    <label>{i18n_stored.get_value().t("channels-app-id")}</label>
                                                    <input type="text"
                                                        prop:value=move || form_app_id.get()
                                                        on:input=move |e| set_form_app_id.set(event_target_value(&e)) />
                                                </div>
                                                <div class="form-group">
                                                    <label>{i18n_stored.get_value().t("channels-app-secret")}</label>
                                                    <input type="password" placeholder="••••••••"
                                                        prop:value=move || form_app_secret.get()
                                                        on:input=move |e| set_form_app_secret.set(event_target_value(&e)) />
                                                </div>
                                                <div class="form-group">
                                                    <label>{i18n_stored.get_value().t("channels-region")}</label>
                                                    <select
                                                        prop:value=move || form_region.get()
                                                        on:change=move |e| set_form_region.set(event_target_selected_value(&e))
                                                    >
                                                        <option value="china">{i18n_stored.get_value().t("channels-region-china")}</option>
                                                        <option value="international">{i18n_stored.get_value().t("channels-region-intl")}</option>
                                                    </select>
                                                </div>
                                            </>
                                        }.into_any(),
                                        "wecom" => view! {
                                            <>
                                                <div class="form-group">
                                                    <label>{i18n_stored.get_value().t("channels-bot-id")}</label>
                                                    <input type="text"
                                                        prop:value=move || form_app_id.get()
                                                        on:input=move |e| set_form_app_id.set(event_target_value(&e)) />
                                                </div>
                                                <div class="form-group">
                                                    <label>{i18n_stored.get_value().t("channels-bot-secret")}</label>
                                                    <input type="password" placeholder="••••••••"
                                                        prop:value=move || form_app_secret.get()
                                                        on:input=move |e| set_form_app_secret.set(event_target_value(&e)) />
                                                </div>
                                            </>
                                        }.into_any(),
                                        "telegram" | "discord" | "qq" | "matrix" | "mattermost" => view! {
                                            <>
                                                <div class="form-group">
                                                    <label>{i18n_stored.get_value().t("channels-bot-token")}</label>
                                                    <input type="password" placeholder="••••••••"
                                                        prop:value=move || form_bot_token.get()
                                                        on:input=move |e| set_form_bot_token.set(event_target_value(&e)) />
                                                </div>
                                                <div class="form-group">
                                                    <label class="checkbox-label">
                                                        <input type="checkbox" checked={move || form_auto_reconnect.get()}
                                                            on:change=move |e| set_form_auto_reconnect.set(event_target_checked(&e)) />
                                                        <span>{i18n_stored.get_value().t("channels-auto-reconnect")}</span>
                                                    </label>
                                                </div>
                                            </>
                                        }.into_any(),
                                        "mqtt" => view! {
                                            <>
                                                <div class="form-group">
                                                    <label>{i18n_stored.get_value().t("channels-webhook-url")}</label>
                                                    <input type="text" placeholder="mqtt://broker.example.com:1883"
                                                        prop:value=move || form_webhook_url.get()
                                                        on:input=move |e| set_form_webhook_url.set(event_target_value(&e)) />
                                                </div>
                                                <div class="form-group">
                                                    <label>{i18n_stored.get_value().t("channels-api-key")}</label>
                                                    <input type="text"
                                                        prop:value=move || form_api_key.get()
                                                        on:input=move |e| set_form_api_key.set(event_target_value(&e)) />
                                                </div>
                                            </>
                                        }.into_any(),
                                        "voice" => view! {
                                            <>
                                                <div class="form-group">
                                                    <label>{i18n_stored.get_value().t("channels-account-sid")}</label>
                                                    <input type="text"
                                                        prop:value=move || form_client_id.get()
                                                        on:input=move |e| set_form_client_id.set(event_target_value(&e)) />
                                                </div>
                                                <div class="form-group">
                                                    <label>{i18n_stored.get_value().t("channels-auth-token")}</label>
                                                    <input type="password"
                                                        prop:value=move || form_client_secret.get()
                                                        on:input=move |e| set_form_client_secret.set(event_target_value(&e)) />
                                                </div>
                                                <div class="form-group">
                                                    <label>{i18n_stored.get_value().t("channels-phone-number")}</label>
                                                    <input type="text"
                                                        prop:value=move || form_bot_token.get()
                                                        on:input=move |e| set_form_bot_token.set(event_target_value(&e)) />
                                                </div>
                                            </>
                                        }.into_any(),
                                        _ => view! { <div></div> }.into_any(),
                                    }}
                                </div>

                                // 二维码认证
                                {if has_qrcode {
                                    Some(view! {
                                        <div class="form-section qr-section">
                                            <div class="form-section-title">{i18n_stored.get_value().t("channels-scan-login")}</div>
                                            <button
                                                class="qr-btn"
                                                on:click=move |_| get_qrcode(key_for_qr.clone())
                                            >
                                                {if qr_data.get().is_some() {
                                                    i18n_stored.get_value().t("channels-refresh-qr")
                                                } else {
                                                    i18n_stored.get_value().t("channels-get-qr")
                                                }}
                                            </button>
                                            {move || {
                                                if qr_loading.get() {
                                                    return view! { <InlineLoading /> }.into_any();
                                                }
                                                if let Some(qr) = qr_data.get() {
                                                    return view! {
                                                        <div class="qr-container">
                                                            {qr.qrcode_img_content.map(|img| view! {
                                                                <img src={img} alt="QR Code" />
                                                            })}
                                                            {move || qr_status.get().map(|status| {
                                                                let (icon, text, class) = match status.status.as_str() {
                                                                    "confirmed" => ("✅", i18n_stored.get_value().t("channels-qr-confirmed"), "confirmed"),
                                                                    "scanned" => ("📱", i18n_stored.get_value().t("channels-qr-scanned"), "pending"),
                                                                    "expired" => ("❌", i18n_stored.get_value().t("channels-qr-expired"), "expired"),
                                                                    _ => ("⏳", i18n_stored.get_value().t("channels-qr-waiting"), "pending"),
                                                                };
                                                                view! {
                                                                    <div class={format!("qr-status {}", class)}>
                                                                        {icon} " " {text}
                                                                    </div>
                                                                }
                                                            })}
                                                            <div class="qr-tip">
                                                                {i18n_stored.get_value().t("channels-qr-scan-tip")}
                                                            </div>
                                                        </div>
                                                    }.into_any();
                                                }
                                                view! { <div></div> }.into_any()
                                            }}
                                        </div>
                                    })
                                } else {
                                    None
                                }}

                                // 访问控制
                                {if has_access_control {
                                    Some(view! {
                                        <div class="form-section">
                                            <div class="form-section-title">{i18n_stored.get_value().t("channels-access-control")}</div>

                                            <div class="form-group">
                                                <div class="label-row" title="私聊消息的处理策略">
                                                    <label>{i18n_stored.get_value().t("channels-dm-policy")}</label>
                                                    <span class="info-icon">"ⓘ"</span>
                                                </div>
                                                <select
                                                    prop:value=move || form_dm_policy.get()
                                                    on:change=move |e| set_form_dm_policy.set(event_target_selected_value(&e))
                                                >
                                                    <option value="open">{i18n_stored.get_value().t("channels-policy-open")}</option>
                                                    <option value="allowlist">{i18n_stored.get_value().t("channels-policy-allowlist")}</option>
                                                </select>
                                            </div>

                                            <div class="form-group">
                                                <div class="label-row" title="群聊消息的处理策略">
                                                    <label>{i18n_stored.get_value().t("channels-group-policy")}</label>
                                                    <span class="info-icon">"ⓘ"</span>
                                                </div>
                                                <select
                                                    prop:value=move || form_group_policy.get()
                                                    on:change=move |e| set_form_group_policy.set(event_target_selected_value(&e))
                                                >
                                                    <option value="open">{i18n_stored.get_value().t("channels-policy-open")}</option>
                                                    <option value="allowlist">{i18n_stored.get_value().t("channels-policy-allowlist")}</option>
                                                </select>
                                            </div>

                                            <div class="form-group-compact">
                                                <div class="toggle-row" title="群聊中是否只响应 @ 提及的消息">
                                                    <span class="toggle-label">
                                                        {i18n_stored.get_value().t("channels-require-mention")}
                                                        <span class="info-icon">"ⓘ"</span>
                                                    </span>
                                                    <label class="toggle-switch">
                                                        <input
                                                            type="checkbox"
                                                            checked={move || form_require_mention.get()}
                                                            on:change=move |e| set_form_require_mention.set(event_target_checked(&e))
                                                        />
                                                        <span class="toggle-slider"></span>
                                                    </label>
                                                </div>
                                            </div>

                                            <div class="form-group">
                                                <div class="label-row">
                                                    <label>{i18n_stored.get_value().t("channels-white-list")}</label>
                                                    <span class="shield-icon">"🛡️"</span>
                                                </div>
                                                <input
                                                    type="text"
                                                    placeholder={i18n_stored.get_value().t("channels-white-list-placeholder")}
                                                    prop:value=move || form_allow_from.get()
                                                    on:input=move |e| set_form_allow_from.set(event_target_value(&e))
                                                />
                                            </div>
                                        </div>
                                    })
                                } else {
                                    None
                                }}
                            </div>

                            // 抽屉底部
                            <div class="drawer-footer">
                                <button class="btn-cancel" on:click=move |_| close_drawer()>
                                    {i18n_stored.get_value().t("action-cancel")}
                                </button>
                                <button
                                    class="btn-save"
                                    on:click=move |_| {
                                        let mut new_config = ChannelConfig::default();
                                        new_config.enabled = form_enabled.get();
                                        new_config.bot_prefix = Some(form_prefix.get()).filter(|s| !s.is_empty());
                                        new_config.filter_tool_messages = Some(form_filter_tool.get());
                                        new_config.filter_thinking = Some(form_filter_think.get());
                                        new_config.dm_policy = Some(form_dm_policy.get()).filter(|s| !s.is_empty());
                                        new_config.group_policy = Some(form_group_policy.get()).filter(|s| !s.is_empty());
                                        new_config.require_mention = Some(form_require_mention.get());
                                        new_config.allow_from = Some(form_allow_from.get().split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()).filter(|v: &Vec<String>| !v.is_empty());
                                        new_config.base_url = Some(form_base_url.get()).filter(|s| !s.is_empty());
                                        new_config.bot_token = Some(form_bot_token.get()).filter(|s| !s.is_empty());
                                        new_config.client_id = Some(form_client_id.get()).filter(|s| !s.is_empty());
                                        new_config.client_secret = Some(form_client_secret.get()).filter(|s| !s.is_empty());
                                        new_config.app_id = Some(form_app_id.get()).filter(|s| !s.is_empty());
                                        new_config.app_secret = Some(form_app_secret.get()).filter(|s| !s.is_empty());
                                        new_config.webhook_url = Some(form_webhook_url.get()).filter(|s| !s.is_empty());
                                        new_config.api_key = Some(form_api_key.get()).filter(|s| !s.is_empty());
                                        new_config.api_secret = Some(form_api_secret.get()).filter(|s| !s.is_empty());
                                        new_config.region = Some(form_region.get()).filter(|s| !s.is_empty());
                                        new_config.message_type = Some(form_message_type.get()).filter(|s| !s.is_empty());
                                        new_config.auto_reconnect = Some(form_auto_reconnect.get());
                                        new_config.bot_base_url = Some(form_bot_base_url.get()).filter(|s| !s.is_empty());
                                        new_config.reconnect_interval_secs = form_reconnect_interval.get().parse().ok();
                                        new_config.warning_before_secs = form_warning_before.get().parse().ok();
                                        new_config.force_before_secs = form_force_before.get().parse().ok();
                                        new_config.message_merge_enabled = Some(form_message_merge.get());
                                        new_config.message_merge_delay_ms = form_merge_delay.get().parse().ok();
                                        new_config.media_dir = Some(form_media_dir.get()).filter(|s| !s.is_empty());
                                        save_config(key_for_save.clone(), new_config);
                                    }
                                >
                                    {i18n_stored.get_value().t("action-save")}
                                </button>
                            </div>
                        </div>
                    </>
                })
            }}
        </div>
    }
}
