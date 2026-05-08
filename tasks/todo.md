# 频道管理模块重构 —— 仿照 QwenPaw 频道模块

## 目标
将 BeeBotOS 现有的简陋频道管理（简单卡片网格 + 基础配置面板）重构为与 QwenPaw 控制台频道页完全对齐的实现：卡片网格布局、筛选 Tab、右侧抽屉编辑、状态指示、品牌图标、访问控制、二维码认证。

## 设计原则
- **不兼容旧数据**：清除旧数据结构，使用新模型
- **不动 agents 底层通信层**：只改 Gateway API 层和 Web 前端，不改 `crates/agents/src/communication/` 下的核心通信逻辑
- **前后端对齐 QwenPaw API 契约**：路径改为 `/config/channels/*`，数据模型扩展 QwenPaw 风格字段

---

## Phase 1：后端 Gateway API 重构 ✅

### 1.1 扩展数据模型（`apps/gateway/src/handlers/http/channels.rs`）
- [x] 重写 `ChannelInfo`：添加 `is_builtin`, `bot_prefix`, `filter_tool_messages`, `filter_thinking`, `dm_policy`, `group_policy`, `allow_from`, `require_mention`
- [x] 新增 `ChannelConfig` 结构：基础字段 + 平台专属字段
- [x] 新增 `ChannelType` / `ChannelTypesResponse`
- [x] 新增 `ChannelQrcodeResponse`, `ChannelQrcodeStatusResponse`

### 1.2 新增/修改 API 端点
- [x] `GET /config/channels/types` → 返回所有可用频道 key 列表
- [x] `GET /config/channels` → 返回 `Record<String, ChannelConfig>` 形式的全量配置
- [x] `PUT /config/channels/:name` → 更新单个频道配置（QwenPaw 主写接口）
- [x] `GET /config/channels/:name/qrcode` → 获取 base64 QR 图 + poll_token
- [x] `GET /config/channels/:name/qrcode/status?token=` → 轮询扫码状态
- [x] 保留并改造原有微信二维码逻辑，适配新路径

### 1.3 路由注册
- [x] 修改 `apps/gateway/src/main.rs` 注册新路由

---

## Phase 2：前端 API 层扩展 ✅

### 2.1 扩展数据模型（`apps/web/src/api/services.rs`）
- [x] 重写 `ChannelConfig`：对齐后端新模型（40+ 字段）
- [x] 新增 `ChannelTypesResponse`, `ChannelQrcodeResponse`, `ChannelQrcodeStatusResponse`

### 2.2 扩展 ChannelService
- [x] `list_channel_types()` → `GET /config/channels/types`
- [x] `list_channels_config()` → `GET /config/channels`
- [x] `update_channel_config(name, config)` → `PUT /config/channels/:name`
- [x] `get_channel_qrcode(channel)` → `GET /config/channels/:name/qrcode`
- [x] `get_channel_qrcode_status(channel, token)` → `GET /config/channels/:name/qrcode/status`

---

## Phase 3：前端频道页面完全重写 ✅

### 3.1 页面主容器（`apps/web/src/pages/channels.rs`）
- [x] 页头：标题 "频道管理" + 副标题 + 分段筛选 Tab（全部/内置/自定义）
- [x] 卡片网格：`grid-template-columns: repeat(auto-fill, minmax(320px, 1fr))`
- [x] 已启用卡片排在前面，未启用排在后面
- [x] 加载态、空状态

### 3.2 频道卡片
- [x] 三段式布局：顶栏(emoji 图标+状态点) / 中段(名称+chip) / 底部(Bot Prefix)
- [x] 状态点：6px 圆点，启用 teal `#14B8A6`，未启用灰 `#64748b`
- [x] 内置/自定义 chip
- [x] hover 效果

### 3.3 右侧抽屉（420px）
- [x] 基础字段：Enabled Switch、Bot Prefix Input、Filter Tool Messages、Filter Thinking
- [x] 平台专属字段：根据 channel key 动态渲染（微信/钉钉/飞书/企微/Telegram/Discord/QQ/Matrix/Mattermost/MQTT/语音）
- [x] 二维码认证区域：微信/钉钉/企微支持
- [x] 访问控制（10 个频道）：DM Policy、Group Policy、Require Mention、Allow From
- [x] 底部操作：取消 + 保存

### 3.4 状态管理
- [x] 数据拉取、抽屉开关、表单状态、二维码轮询、Toast 提示

---

## Phase 4：样式系统 ✅
- [x] 频道页内联样式：网格、卡片、Tab、抽屉、表单、Toast、暗色模式兼容
- [x] 使用 CSS 变量适配项目主题

---

## Phase 5：国际化 ✅
- [x] 新增 40+ 频道管理相关 i18n key（`apps/web/src/i18n.rs`）
- [x] 中英文双语支持

---

## Phase 6：验证 ✅

- [x] `cargo check -p beebotos-gateway` 编译通过
- [x] `./beebotos-dev.sh build web` 前端编译通过（0 warnings）
- [x] 频道页结构完整：卡片网格、筛选 Tab、抽屉滑出、表单保存、二维码区域

---

## Review

### 已修改文件
1. `apps/gateway/src/handlers/http/channels.rs` — 重写后端 API，新增 QwenPaw 风格数据模型和端点
2. `apps/gateway/src/main.rs` — 注册新路由 `/config/channels/*`
3. `apps/web/src/api/services.rs` — 扩展 `ChannelConfig`（40+ 字段）和 `ChannelService`（5 个新方法）
4. `apps/web/src/api/mod.rs` — 更新 re-export
5. `apps/web/src/pages/channels.rs` — 完全重写为 QwenPaw 风格页面
6. `apps/web/src/i18n.rs` — 添加 40+ 频道管理翻译键

### 待办（未来增强）
- [ ] 二维码轮询实际测试（需要后端接入真实扫码服务）
- [ ] 频道图标从 emoji 升级为品牌色圆角字母图标（LETTER_ICON_COLORS 逻辑已预留）
- [ ] 抽屉内文档链接根据语言动态切换
