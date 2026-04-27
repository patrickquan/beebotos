//! 工具调用卡片组件
//!
//! 在聊天界面中显示 LLM 工具调用的状态、参数和结果。

use leptos::prelude::*;

/// 工具调用状态
#[derive(Clone, Debug, PartialEq)]
pub enum ToolCallStatus {
    /// 正在执行
    Running,
    /// 执行成功
    Success,
    /// 执行失败
    Error,
}

/// 工具调用卡片
#[component]
pub fn ToolCallCard(
    /// 工具名称（如 "read_file", "write_file"）
    #[prop(into)]
    tool_name: String,
    /// 工具参数（JSON 字符串）
    #[prop(default = "{}".to_string())]
    arguments: String,
    /// 执行状态
    #[prop(default = ToolCallStatus::Running)]
    status: ToolCallStatus,
    /// 执行结果（可选）
    #[prop(into, default = None)]
    result: Option<String>,
    /// 是否可展开查看详情
    #[prop(default = true)]
    expandable: bool,
) -> impl IntoView {
    let expanded = RwSignal::new(false);
    let args_store = StoredValue::new(arguments);

    let status_icon = match status {
        ToolCallStatus::Running => "⏳",
        ToolCallStatus::Success => "✅",
        ToolCallStatus::Error => "❌",
    };

    let status_class = match status {
        ToolCallStatus::Running => "tool-status-running",
        ToolCallStatus::Success => "tool-status-success",
        ToolCallStatus::Error => "tool-status-error",
    };

    // 格式化参数为可读形式
    let formatted_args = Memo::new(move |_| {
        let args = args_store.get_value();
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&args) {
            if let Some(obj) = json.as_object() {
                let pairs: Vec<String> = obj.iter().map(|(k, v)| format!("{}={}", k, v)).collect();
                pairs.join(", ")
            } else {
                args
            }
        } else {
            args
        }
    });

    view! {
        <div class=format!("tool-call-card {}", status_class)>
            <div class="tool-call-header"
                on:click=move |_| {
                    if expandable {
                        expanded.update(|v| *v = !*v);
                    }
                }
            >
                <span class="tool-status-icon">{status_icon}</span>
                <span class="tool-name">{tool_name.clone()}</span>
                <span class="tool-args-preview">{move || formatted_args.get()}</span>
                {if expandable {
                    view! {
                        <span class="tool-expand-icon">
                            {move || if expanded.get() { "▼" } else { "▶" }}
                        </span>
                    }.into_any()
                } else {
                    view! { <span /> }.into_any()
                }}
            </div>

            <Show when=move || expanded.get()>
                <div class="tool-call-body">
                    <div class="tool-section">
                        <div class="tool-section-title">"参数"</div>
                        <pre class="tool-code">
                            <code>{args_store.get_value()}</code>
                        </pre>
                    </div>

                    {if let Some(ref res) = result {
                        view! {
                            <div class="tool-section">
                                <div class="tool-section-title">"结果"</div>
                                <pre class="tool-code">
                                    <code>{res.clone()}</code>
                                </pre>
                            </div>
                        }.into_any()
                    } else {
                        view! { <div /> }.into_any()
                    }}
                </div>
            </Show>
        </div>
    }
}

/// 工具调用列表（显示多个工具调用）
#[component]
pub fn ToolCallList(#[prop(into)] tools: Vec<ToolCallItem>) -> impl IntoView {
    view! {
        <div class="tool-call-list">
            {tools.into_iter().map(|tool| {
                view! {
                    <ToolCallCard
                        tool_name=tool.name
                        arguments=tool.arguments
                        status=tool.status
                        result=tool.result
                    />
                }
            }).collect_view()}
        </div>
    }
}

/// 工具调用项数据
#[derive(Clone, Debug)]
pub struct ToolCallItem {
    pub name: String,
    pub arguments: String,
    pub status: ToolCallStatus,
    pub result: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_call_status() {
        assert_ne!(ToolCallStatus::Running, ToolCallStatus::Success);
        assert_eq!(ToolCallStatus::Error, ToolCallStatus::Error);
    }
}
