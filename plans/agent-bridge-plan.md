# Agent Bridge 开发计划

借鉴 Mori Agent Bridge，为 Amux 实现跨 Pane Agent 发现、状态感知和通信系统。

## 现状分析

Amux 已有的基础设施：

| 组件 | 状态 | 位置 |
|---|---|---|
| `AgentKind` 枚举 (Claude/Aider/OpenCode/Codex/Gemini/Copilot) | 已实现 | `manager.rs` |
| `AgentStatus` 枚举 (Thinking/Waiting/Done/Error) | 已实现 | `manager.rs` |
| `poll_activity()` → 从终端输出检测 Agent 状态 | 已实现 | `manager.rs` |
| `agent_summaries()` → 收集所有 Agent 状态摘要 | 已实现 | `manager.rs` |
| `AgentSummary` + 状态栏展示 | 已实现 | `gpui_status_bar.rs` |
| `send_paste_input()` → 向 Pane 写入文本 | 已实现 | `alacritty_view.rs` |
| `last_lines(n)` → 读取终端最后 N 行 | 已实现 | `alacritty_view.rs` |
| `try_intercept_amux_command()` → 终端命令拦截 | 已实现 | `gpui_entry.rs` |
| IPC / Unix Socket / Named Pipe | **不存在** | — |
| CLI 二进制（独立于 GUI） | **不存在** | — |
| Sidebar Agents 视图 | **不存在** | — |
| Agent 间消息协议 | **不存在** | — |

**关键架构差异**：Mori 依赖 tmux 做 pane 管理，通过 Unix Socket IPC + CLI 实现 Bridge。Amux 是单进程直接持有所有 PTY，所以可以用更轻量的方式实现——不需要 IPC socket，直接在进程内读写 pane 即可。外部 CLI 通过 Named Pipe (Windows) / Unix Socket (Linux) 与 GUI 进程通信。

---

## 三个功能模块

### 模块一：Agent Bridge 协议与 CLI

**目标**：让 Agent（或用户脚本）通过 CLI 命令发现、观察、通信其他 Pane 中的 Agent。

#### Phase 1.1：进程内 Bridge API

在 `crates/amux-platform/src/terminal/manager.rs` 的 `TerminalManager` 上新增方法：

```rust
// 列出所有 pane 及其 agent 状态
pub fn pane_list(&self) -> Vec<PaneInfo>;

// 读取指定 pane 的最后 N 行输出
pub fn pane_read(&self, pane_id: &PaneId, lines: usize) -> Option<Vec<String>>;

// 向指定 pane 发送消息（自动包裹信封格式）
pub fn pane_message(&self, target: &PaneId, from: &PaneId, text: &str) -> Result<(), String>;

// 获取指定 pane 的身份信息
pub fn pane_identity(&self, pane_id: &PaneId) -> Option<PaneIdentity>;
```

新增数据结构：

```rust
pub struct PaneInfo {
    pub pane_id: PaneId,
    pub tab_title: String,
    pub agent_kind: Option<AgentKind>,
    pub agent_status: Option<AgentStatus>,
    pub workspace_name: String,
}

pub struct PaneIdentity {
    pub workspace: String,
    pub pane_id: PaneId,
    pub tab_title: String,
    pub agent_kind: Option<AgentKind>,
}
```

**实现要点**：
- `pane_list` 遍历 `self.panes`，收集每个 pane 的 active tab 信息
- `pane_read` 调用已有的 `AlacrittyTerminal::last_lines(n)`，上限 200 行
- `pane_message` 调用已有的 `send_paste_input()`，先包裹信封格式
- `pane_identity` 从 pane + workspace 上下文组装

**涉及文件**：
- `crates/amux-platform/src/terminal/manager.rs` — 新增 4 个方法 + 2 个 struct

#### Phase 1.2：消息信封格式

定义 Amux 的消息信封（参考 Mori 但简化，因为 Amux 没有 worktree 概念）：

```
[amux-bridge workspace:<name> pane:<id> agent:<kind>] <text>
```

示例：
```
[amux-bridge workspace:myapp pane:pane-3 agent:claude] 请检查 src/auth.rs 的测试
```

在 `crates/amux-core/` 新增 `bridge.rs`：

```rust
pub struct BridgeMessage {
    pub workspace: String,
    pub pane_id: String,
    pub agent: Option<String>,  // agent kind or "user"
    pub text: String,
}

impl BridgeMessage {
    pub fn format(&self) -> String;
    pub fn parse(line: &str) -> Option<Self>;
}
```

**涉及文件**：
- `crates/amux-core/src/bridge.rs` — 新文件，信封格式定义
- `crates/amux-core/src/lib.rs` — 添加 `pub mod bridge;`

#### Phase 1.3：终端内命令扩展

扩展现有的 `try_intercept_amux_command()` 支持 bridge 命令：

```bash
amux pane list                        # JSON 输出所有 pane
amux pane read <pane-id> [--lines N]  # 读取 pane 输出
amux pane message <pane-id> "text"    # 发送消息
amux pane id                          # 显示当前 pane 身份
```

命令输出直接写入当前终端（通过 `send_paste_input` 或新增的直接输出机制）。

**实现要点**：
- 在 `gpui_entry.rs` 的 `try_intercept_amux_command()` 扩展 match 分支
- `amux pane list` 的输出需要写回当前终端，可用 `send_paste_input` 但更好的方式是直接向 PTY master 写入格式化文本（避免被当作用户输入）
- 考虑增加一个 `write_output_to_pane()` 方法，向 pane 写入不经过 shell 解析的纯文本输出

**涉及文件**：
- `apps/desktop/src/gpui_entry.rs` — 扩展 command interception
- `crates/amux-platform/src/terminal/alacritty_view.rs` — 可能需要新增直接输出方法

#### Phase 1.4：外部 CLI 二进制（可选，后续）

创建独立的 `amux-cli` crate，通过 Named Pipe (Windows) / Unix Socket (Linux) 与运行中的 GUI 通信。这样外部脚本和 Agent 可以在 Amux 之外调用 bridge 命令。

**暂不在首期实现**，原因：
- 终端内命令已覆盖 Agent 使用场景（Agent 在 Amux pane 内运行，直接执行 `amux pane ...`）
- IPC 机制增加复杂度，可在需求明确后再做

---

### 模块二：Agent 状态感知 + Sidebar 状态分组

**目标**：在 sidebar 增加 Agents 视图，按状态分组显示所有 Agent，提供 Hover Peek 和 Quick Reply。

#### Phase 2.1：Sidebar Agents 视图模式

在 workspace sidebar 顶部增加模式切换：**Workspaces** | **Agents**

Agents 模式按状态分组显示：

```
⚠ ATTENTION (需要输入)
  ├─ Claude [pane-3] — waiting     ❗ [回复]
  └─ Codex [pane-5] — error        ✗

⚡ RUNNING
  └─ Aider [pane-7] — thinking     ⟳

✅ COMPLETED
  └─ Gemini [pane-2] — done        ✓

💤 IDLE (无 Agent)
  └─ Shell [pane-1]
```

**数据源**：直接从 `TerminalManager::agent_summaries()` 获取，已有完整数据。

**涉及文件**：
- `apps/desktop/src/gpui_workspace_sidebar.rs` — 新增 Agents 视图渲染
- `apps/desktop/src/gpui_entry.rs` — 新增 sidebar mode 状态切换

**WorkspaceSidebarState 修改**：
```rust
pub enum SidebarMode {
    Workspaces,
    Agents,
}
// 新增字段
pub sidebar_mode: SidebarMode,
```

#### Phase 2.2：Hover Peek

鼠标悬停在 Agents 列表的任意 Agent 行上，300ms 后弹出 popover 显示该 pane 最后 8 行输出。

**实现要点**：
- 使用 GPUI 的 tooltip 或自定义 popover（absolute positioned div）
- 调用 `pane_read(pane_id, 8)` 获取内容
- 缓存 5 秒，避免频繁读取
- Popover 使用等宽字体，Tomorrow Night 配色，模拟终端外观

**涉及文件**：
- `apps/desktop/src/gpui_workspace_sidebar.rs` — 悬停检测 + popover 渲染

#### Phase 2.3：Quick Reply

点击 "waiting" 状态的 Agent 行上的回复按钮，展开内联输入框。输入文本后按 Enter 发送到该 pane。

**实现要点**：
- 点击 ❗ 图标 → 在行下方展开 input div
- 输入框获取焦点，Enter 发送（调用 `pane_message`），Escape 关闭
- 发送后自动关闭输入框

**涉及文件**：
- `apps/desktop/src/gpui_workspace_sidebar.rs` — 内联输入渲染
- `apps/desktop/src/gpui_input_handler.rs` — 可能需要处理输入焦点切换

#### Phase 2.4：Agent 通知增强

当 Agent 状态从 Thinking 变为 Waiting/Done/Error 时：
- Tab 标题闪烁（已有 `has_activity` 标记）
- 系统通知（Windows toast / Linux notify-send）
- Sidebar badge 更新（实时，不需要切换视图）

**涉及文件**：
- `apps/desktop/src/gpui_entry.rs` — 处理 `AgentNotification`，触发通知
- `apps/desktop/src/gpui_layout_renderer.rs` — Tab 标题 agent 状态指示

---

### 模块三：Agent 自发现与协作教学

**目标**：让 Agent 知道自己在 Amux 中的身份，以及如何与其他 Agent 协作。

#### Phase 3.1：环境变量注入

在 Amux 创建终端 pane 时，注入环境变量：

```
AMUX_WORKSPACE=myapp
AMUX_PANE_ID=pane-3
AMUX_PANE_TITLE=claude
AMUX_VERSION=0.1.0
```

当检测到 Agent 启动后，追加：
```
AMUX_AGENT_KIND=claude
```

**实现要点**：
- 在 `AlacrittyTerminal::new()` / `with_scrollback()` 创建 PTY 时设置环境变量
- `AMUX_PANE_ID` 在创建时已知
- `AMUX_AGENT_KIND` 需要在检测到 Agent 后通过 shell 命令注入（`export AMUX_AGENT_KIND=claude`），或在 Agent 启动命令前预设

**涉及文件**：
- `crates/amux-platform/src/terminal/alacritty_view.rs` — PTY 环境变量
- `crates/amux-platform/src/terminal/manager.rs` — Agent 启动时的环境设置

#### Phase 3.2：AGENTS.md / System Prompt 模板

提供标准化的 Agent 指引模板，用户可以添加到各 Agent 的配置中：

```markdown
## Amux Inter-Agent Communication

你运行在 Amux 终端多路复用器中。可以通过 `amux` 命令与其他 Agent 协作：

- **发现**: `amux pane list` — JSON 列出所有 pane 及 Agent 状态
- **观察**: `amux pane read <pane-id> --lines 20` — 读取其他 pane 的输出
- **通信**: `amux pane message <pane-id> "消息内容"` — 向其他 Agent 发送消息
- **身份**: `amux pane id` — 显示你所在的 pane 信息

收到的消息格式为：
`[amux-bridge workspace:<w> pane:<id> agent:<kind>] <text>`

看到此格式的输入时，这是来自其他 Agent 的消息，请阅读并响应。
```

**实现方式**：
- 在 `~/.amux/` 目录下生成 `agent-prompt.md` 模板
- `amux pane teach` 命令：输出该模板到终端，Agent 可直接读取

**涉及文件**：
- `apps/desktop/src/gpui_entry.rs` — `amux pane teach` 命令
- 可选：`gpui_config.rs` — 配置是否自动注入 prompt

---

## 开发顺序与优先级

```
Phase 1.1  进程内 Bridge API          ■■□□□  基础，必须先做
Phase 1.2  消息信封格式               ■□□□□  简单，与 1.1 并行
Phase 2.1  Sidebar Agents 视图        ■■■□□  用户可见价值最高
Phase 1.3  终端内命令扩展             ■■□□□  让 Agent 能使用 Bridge
Phase 3.1  环境变量注入               ■□□□□  简单改动
Phase 2.2  Hover Peek                 ■■□□□  体验提升
Phase 2.3  Quick Reply                ■■□□□  体验提升
Phase 2.4  Agent 通知增强             ■■□□□  体验提升
Phase 3.2  Agent 教学模板             ■□□□□  文档工作
Phase 1.4  外部 CLI (后续)            ■■■■□  复杂度高，非首期
```

**建议开发批次**：

| 批次 | 内容 | 预期效果 |
|---|---|---|
| **第一批** | 1.1 + 1.2 + 1.3 + 3.1 | Agent 可在终端内执行 `amux pane list/read/message/id`，实现基本通信 |
| **第二批** | 2.1 + 2.2 | 用户在 sidebar 看到所有 Agent 状态，悬停预览输出 |
| **第三批** | 2.3 + 2.4 + 3.2 | Quick Reply、通知、教学模板，完善体验 |
| **后续** | 1.4 | 外部 CLI，支持 Amux 外部脚本调用 |

---

## 与 Mori 的关键差异

| 方面 | Mori | Amux |
|---|---|---|
| 通信基础 | tmux send-keys（跨进程） | 进程内直接调用（更快更可靠） |
| 身份发现 | 环境变量 MORI_* | 环境变量 AMUX_* + 进程内直接查询 |
| 输出读取 | tmux capture-pane（有延迟） | AlacrittyTerminal::last_lines()（实时） |
| 状态检测 | tmux pane option 轮询 5s | poll_activity() 每帧检测（已实现） |
| 消息信封 | `[mori-bridge project:X worktree:Y ...]` | `[amux-bridge workspace:X pane:Y ...]` |
| IPC 机制 | Unix Domain Socket | 首期不需要（进程内）；后续 Named Pipe/UDS |
| 外部 CLI | 独立 `mori` 二进制 | 首期终端内命令；后续 `amux-cli` |

**Amux 的优势**：单进程架构意味着 Bridge 实现更简单、延迟更低、可靠性更高。不需要处理进程间序列化、socket 连接断开、权限等问题。
