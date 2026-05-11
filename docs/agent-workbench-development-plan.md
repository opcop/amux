# Agent Workbench 开发计划

## 背景

Amux 已经具备多 pane、AI 工具启动、Agent 状态识别、Sidebar Agents、Agent Bridge、浏览器/预览等基础能力。现在的问题不是“能不能同时跑多个 Agent”，而是多个 Agent 跑起来之后缺少统一的任务、状态、证据和协作协议。

`third_party/tmux-ide` 值得借鉴的核心不是 tmux 布局，而是它把终端 pane 抽象成了一个可调度的 Agent 工作台：

- 有 Mission / Goal / Task 的任务账本。
- 有 Lead Agent 和 Worker Agent 的角色分工。
- 有任务依赖、优先级、proof、validation contract。
- 有 Mission Control 面板观察 Agent、任务、目标和事件。
- 有自动拼装任务上下文并分派到空闲 Agent 的 orchestrator。

Amux 的优势是 GUI、跨平台桌面体验、真实 pane 状态、Agent Bridge、浏览器和本地预览能力。开发方向应该是把这些能力组合成 **Agent Workbench**：让用户把多个 AI CLI 当成一个有任务、有状态、有协作协议的团队使用。

## 目标

第一阶段目标不是全自动项目经理，而是做一个可控、可解释、可逐步自动化的 Agent 工作台。

### 用户目标

- 用户可以在 Amux 里创建一个 Mission，并拆成 Goal / Task。
- 用户可以把 Task 指派给某个 Agent，或者让 Amux 选择空闲 Agent。
- Agent 收到任务时，自动带上任务上下文、项目协作说明和完成协议。
- 用户能在一个面板里看到每个 Agent 正在做什么、哪些任务阻塞、哪些已完成、完成证据是什么。
- Agent 完成任务后可以通过命令或 Bridge 消息回写 proof。

### 产品目标

- 把 Amux 从“多 AI 终端管理器”推进到“多 Agent 协作工作台”。
- 复用现有 Agent Bridge 和 Sidebar Agents，不重造一套独立系统。
- 保持用户可控：第一版默认不自动无限派活，自动调度必须可开关。
- 所有任务状态本地保存，便于审计、恢复和后续 session replay。

## 非目标

第一版不做这些：

- 不做完整项目管理软件，不替代 Linear、GitHub Issues。
- 不做跨设备同步。
- 不强制用户使用固定工作流。
- 不直接依赖某个 AI CLI 的私有协议。
- 不一开始就做复杂自动调度、自动验收、自动 PR 合并。
- 不让 Agent 在无任务账本和无 proof 的情况下自动互相派生工作。

## 核心概念

### Mission

一次工作会话的总目标。

示例：

```text
为 Amux 增加 Agent Workbench MVP。
```

字段：

- `id`
- `title`
- `description`
- `status`: `planning | active | validating | complete | archived`
- `created_at`
- `updated_at`

### Goal

Mission 下的阶段性目标。Goal 应该有明确验收标准。

字段：

- `id`
- `mission_id`
- `title`
- `description`
- `acceptance`
- `priority`
- `status`: `todo | in_progress | done`
- `created_at`
- `updated_at`

### Task

可以被单个 Agent 独立执行的最小工作单元。

字段：

- `id`
- `mission_id`
- `goal_id`
- `title`
- `description`
- `status`: `todo | assigned | in_progress | review | done | blocked | failed`
- `assignee_pane_id`
- `assignee_agent_kind`
- `priority`
- `depends_on`
- `tags`
- `specialty`
- `proof`
- `created_at`
- `updated_at`

### Proof

Agent 完成任务时必须留下的完成证据。

字段：

- `notes`
- `tests`
- `files_changed`
- `commands_run`
- `errors`
- `pr`
- `ci`

第一版允许 proof 是自由文本，但内部结构要预留 JSON 字段，后续可以升级成强 schema。

### Event

Agent Workbench 的事件日志。

事件类型：

- `mission_created`
- `goal_created`
- `task_created`
- `task_assigned`
- `task_started`
- `task_completed`
- `task_failed`
- `task_blocked`
- `agent_message_sent`
- `proof_recorded`

事件日志用于 Mission Control、审计、session replay 和调试。

## 存储方案

第一版使用本地文件存储，避免引入数据库和迁移复杂度。

建议路径：

```text
~/.amux/workspaces/<workspace-id>/workbench/
  mission.json
  goals/
    01-api-foundation.json
  tasks/
    001-add-task-store.json
  events.jsonl
  dispatch/
    001-add-task-store.md
```

实现要求：

- 写 JSON 文件时使用临时文件 + rename，避免崩溃时写坏状态。
- `events.jsonl` 只追加，不在热路径里重写。
- 文件名带 ID 和 slug，便于用户直接查看。
- 所有读写逻辑集中在一个 store 模块里，不散落在 UI 和 bridge 命令中。

建议新增模块：

```text
apps/desktop/src/workbench/
  mod.rs
  model.rs
  store.rs
  events.rs
  prompt.rs
  dispatch.rs
```

## CLI / Bridge 命令

Amux 需要提供给用户和 Agent 都能调用的命令。第一版可以先走现有 Agent Bridge / 命令拦截路径，后续再补完整外部 CLI。

### Mission

```bash
amux mission create "Agent Workbench MVP" --description "..."
amux mission show
amux mission status
amux mission complete
```

### Goal

```bash
amux goal create "任务账本" --acceptance "可创建、展示、持久化任务"
amux goal list
amux goal show 01
amux goal done 01
```

### Task

```bash
amux task create "实现 TaskStore" --goal 01 --priority 1 --tags "rust,state"
amux task list
amux task show 001
amux task assign 001 --pane pane-3
amux task send 001 --pane pane-3
amux task done 001 --proof "实现了 store，cargo test 通过"
amux task block 001 --reason "等待 UI 决策"
```

### Pane / Agent

复用现有 Agent Bridge 能力：

```bash
amux pane list
amux pane read pane-3 --lines 40
amux pane message pane-3 "..."
```

新增或扩展：

```bash
amux agent list
amux agent idle
amux agent assign 001
```

第一版可以不暴露全部 CLI，只要内部 command palette / command bar 能调用同一套 action。

## 任务上下文拼装

把 Task 发给 Agent 时，Amux 不应该只发送一句标题，而是生成一个 dispatch prompt。

Prompt 结构：

```text
You are working inside Amux Agent Workbench.

Mission:
<mission title + description>

Goal:
<goal title + acceptance>

Task:
<task title + description>

Project Guidelines:
<AGENTS.md / CLAUDE.md 摘要>

Recent Context:
<同一 goal 下最近完成的任务摘要>

Available Amux Commands:
- amux pane list
- amux pane read <pane> --lines N
- amux pane message <pane> "..."
- amux task done <id> --proof "..."
- amux task block <id> --reason "..."

Completion Protocol:
When done, run:
amux task done <id> --proof "<what changed, tests run, files touched>"
```

实现要求：

- Prompt 生成逻辑必须是纯函数，方便测试。
- AGENTS.md / CLAUDE.md 只取摘要，避免把长文直接塞进每个任务。
- recent context 第一版只取最近 3 到 5 个完成任务。
- prompt 写入 `dispatch/<task-id>.md`，便于用户检查和复现。

## UI 设计

### Sidebar 扩展

当前 Sidebar 有 Workspaces / Agents。新增 Workbench 模式，或在 Agents 模式里增加任务区域。

第一版建议做独立 Workbench 模式：

```text
[WS] [Agents] [Workbench]
```

Workbench 视图结构：

```text
Mission: Agent Workbench MVP

Progress
  Goals: 1/3 done
  Tasks: 4/12 done, 2 running, 1 blocked

Running
  003 TaskStore persistence       Claude   12m
  004 Mission Control UI          Codex     5m

Todo
  P1 005 Add task command actions
  P2 006 Persist event log

Blocked
  002 Decide proof schema

Recently Done
  001 Define model
```

交互：

- 点击任务：打开任务详情面板。
- 双击任务：发送到选中的空闲 Agent。
- 右键任务：Assign / Send / Mark Done / Block / Delete。
- 点击 Agent 名：聚焦对应 pane。

### Mission Control 面板

中期做一个更完整的 Mission Control，可以作为可停靠面板或普通 pane。

Tabs：

- `Agents`
- `Tasks`
- `Goals`
- `Activity`

第一版不必一次做完整面板，先把 Workbench Sidebar 跑通。

### Command Palette 集成

新增命令：

- `Workbench: Create Mission`
- `Workbench: Create Goal`
- `Workbench: Create Task`
- `Workbench: Assign Task to Active Agent`
- `Workbench: Send Task to Agent...`
- `Workbench: Mark Task Done`
- `Workbench: Open Mission Control`

## Agent 状态和任务绑定

Amux 已经有 AgentSidebarItem 和 Agent 状态识别。需要把任务状态和 Agent 状态关联起来。

建议扩展内存状态：

```rust
pub struct AgentWorkbenchBinding {
    pub pane_id: String,
    pub current_task_id: Option<String>,
    pub assigned_at: Option<SystemTime>,
    pub last_activity_at: Option<SystemTime>,
}
```

绑定规则：

- `task send` 成功后，task 进入 `assigned` 或 `in_progress`。
- 如果目标 pane 是 Agent，记录 `assignee_pane_id`。
- 如果 Agent 输出或 Bridge 命令表明完成，进入 `done` 或 `review`。
- 如果 pane 被关闭，未完成任务进入 `todo` 或 `blocked`，并追加事件。

第一版不要强依赖“精确判断 Agent 是否真的开始工作”。发送成功即可视为 assigned，后续再优化。

## 自动调度策略

自动调度放到第二阶段，不作为 MVP 的硬依赖。

MVP 只做手动或半自动：

- 用户选 task，点 Send。
- 用户选 task，点 Assign to idle agent。
- Amux 可以推荐空闲 Agent，但不持续自动派活。

第二阶段再做：

- 根据 priority 和 depends_on 找下一个 unblocked task。
- 找 idle agent。
- 匹配 specialty。
- 发送 dispatch prompt。
- 记录 claim。
- 超时未活动时提示用户，而不是直接抢占任务。

第三阶段再做：

- stall detection。
- failed retry。
- validator agent。
- research agent。
- milestone gate。

## 开发阶段

### 阶段 0：基础整理

目标：明确现有能力落点，避免重复造轮子。

工作项：

- 梳理现有 Agent Bridge 命令入口。
- 梳理 Sidebar Agents 数据来源。
- 梳理 command palette / command bar action 分发路径。
- 确认 workspace id 和 workspace state 的持久化路径。

交付物：

- 一份实现备注，列出需要接入的现有模块。
- 确定 `workbench/` 模块边界。

验收：

- 能说明 task store、UI、bridge、pane manager 之间的调用关系。

### 阶段 1：任务账本 MVP

目标：能创建、读取、更新 Mission / Goal / Task。

工作项：

- 新增 `workbench/model.rs`。
- 新增 `workbench/store.rs`。
- 新增 atomic JSON 写入。
- 新增 event append。
- 支持 mission / goal / task 基础 CRUD。
- 增加单元测试覆盖序列化、反序列化、ID 生成、状态迁移。

交付物：

- 本地文件可持久化 mission、goal、task、events。
- 内部 action 可调用 store。

验收：

- 创建 mission 后重启 Amux 仍能读到。
- 创建 task 后文件落在 workspace workbench 目录。
- task done 后 proof 被保存。
- 损坏 JSON 不导致整个 Amux 崩溃，UI 显示可恢复错误。

### 阶段 2：Command Palette / Command Bar 接入

目标：用户不用碰文件，也能创建和管理任务。

工作项：

- 增加 Workbench command actions。
- 提供最小输入弹窗：title、description、goal、priority。
- 支持 `Send Task to Agent...` 的 pane picker。
- 支持 `Mark Done` 和 `Block`。

交付物：

- 用户可以通过 UI 创建任务并发送给 Agent。

验收：

- 新建 task 后 Sidebar/状态能刷新。
- Send 后目标 pane 收到完整 dispatch prompt。
- 发送事件写入 `events.jsonl`。

### 阶段 3：任务上下文拼装和发送

目标：Agent 收到的是可执行任务，不是裸标题。

工作项：

- 新增 `workbench/prompt.rs`。
- 实现 dispatch prompt 纯函数。
- 读取 AGENTS.md / CLAUDE.md 摘要。
- 加入 recent completions。
- 生成 `dispatch/<task-id>.md`。
- 调用 Agent Bridge 发送到目标 pane。

交付物：

- `task send` 产生可复查的 dispatch markdown。
- Agent prompt 中包含完成协议。

验收：

- 给 Claude / Codex 发送同一个 task，prompt 内容稳定一致。
- 没有 AGENTS.md 时仍能生成可用 prompt。
- prompt 不超过配置的最大长度。

### 阶段 4：Workbench Sidebar

目标：让用户看得见任务系统。

工作项：

- 扩展 `SidebarMode`，增加 Workbench。
- 新增 Workbench sidebar state。
- 渲染 mission、progress、running、todo、blocked、recent done。
- 点击任务显示详情或聚焦相关 Agent。
- 右键任务提供常用操作。

交付物：

- 一个可用的 Workbench Sidebar。

验收：

- 任务状态变化后 UI 自动刷新。
- running task 能显示 assignee 和耗时。
- blocked task 有明显标识。
- 点击 assignee 能聚焦 pane。

### 阶段 5：Agent 完成协议

目标：Agent 能把任务完成状态回写到 Workbench。

工作项：

- 在 Bridge 命令解析中支持 `amux task done <id> --proof ...`。
- 支持 `amux task block <id> --reason ...`。
- 支持 proof 结构化解析：自由文本优先，JSON 可选。
- 完成时追加 event。
- 完成时给用户 toast 或 Sidebar 高亮。

交付物：

- Agent 可以自助完成任务。

验收：

- 目标 Agent 输入完成命令后，task 状态变成 done。
- proof 被记录并在 UI 可见。
- 最近完成列表更新。

### 阶段 6：半自动分派

目标：减少用户手动选择 Agent 的成本，但保留控制权。

工作项：

- 实现 idle agent 查询。
- 实现 `Assign to idle agent`。
- 根据 specialty 做简单匹配。
- 任务有未完成依赖时禁止发送。
- blocked 原因显示在 UI。

交付物：

- 用户一键把下一个可执行 task 发给空闲 Agent。

验收：

- 有依赖未完成的 task 不会被发送。
- 无空闲 Agent 时给出清楚提示。
- 发送失败时 task 状态回滚。

## 技术风险

### Agent 状态识别不稳定

不同 AI CLI 的 busy / waiting / done 信号不一致。第一版不要把状态识别作为强一致来源，任务状态以 Workbench 命令和用户操作为准。

### 自动发送可能打断 Agent

发送 prompt 前必须检查 pane 是否是 Agent，且最好显示确认。第一版只有用户主动 Send，不做后台自动插入。

### 文件存储并发写入

多个 Agent 可能同时执行 `task done`。store 写入必须集中串行化，或者至少使用 atomic write 和读后合并策略。

### Prompt 过长

AGENTS.md、recent completions、任务描述可能很长。需要配置最大字符数，默认截断并提示“see full file”。

### UI 复杂度膨胀

不要一开始做完整项目管理 UI。Sidebar 先覆盖 80% 场景，Mission Control 面板后置。

## 验收场景

### 场景 1：手动创建并发送任务

1. 用户创建 Mission。
2. 用户创建 Goal。
3. 用户创建 Task。
4. 用户选择一个 Claude pane。
5. 用户执行 Send。
6. Claude pane 收到完整任务上下文。
7. task 状态变成 `in_progress`。

### 场景 2：Agent 完成任务

1. Agent 执行 `amux task done 001 --proof "..."`。
2. task 状态变成 `done`。
3. proof 被保存。
4. Workbench Sidebar 的 recently done 更新。
5. event log 有 `task_completed`。

### 场景 3：阻塞任务不会被误派

1. Task 002 depends_on Task 001。
2. Task 001 未完成。
3. 用户尝试发送 Task 002。
4. Amux 阻止发送并显示 blocked by 001。

### 场景 4：Pane 关闭后的恢复

1. Task 003 已指派给 pane-3。
2. pane-3 被关闭。
3. Amux 检测到 pane 不存在。
4. Task 003 显示为 `blocked` 或回到 `todo`。
5. event log 记录 pane lost。

## 建议开发顺序

推荐按这个顺序落地：

1. `workbench/model.rs`
2. `workbench/store.rs`
3. `workbench/events.rs`
4. Command action：create/list/update task
5. `workbench/prompt.rs`
6. Send task to pane
7. Sidebar Workbench 模式
8. Agent completion command
9. Idle agent picker
10. Mission Control 面板
11. 自动调度

## MVP 边界

MVP 完成后，用户应该可以完成这条主路径：

```text
创建 Mission
  -> 创建 Goal
  -> 创建 Task
  -> 选择 Agent
  -> 发送任务上下文
  -> Agent 执行
  -> Agent 回写 proof
  -> Sidebar 看到进度和完成记录
```

只要这条路径稳定，Amux 就已经从“多 AI 终端”进入“多 Agent 工作台”。自动调度、validator、research、成本统计、session replay 都可以在这个基础上继续加。

## 并行能力：Diff Panel

详见 [docs/diff-panel-design.md](./diff-panel-design.md)。

### 为什么放在这里

Agent Workbench 解决的是“任务派出去了”的问题；Diff Panel 解决的是“任务派出去后改了什么”的问题。两者是**互补但解耦**的能力：

- Workbench 的 proof 字段记录的是文字描述。
- Diff Panel 让用户在不离开 Amux 的情况下直接看到 agent 实际改动的代码。
- 当一个 task 被标记 `done` 时，用户可以一键打开对应 workspace 的 Diff Panel 验证 proof，再决定是否 commit / push。

这条 review 闭环命中了"review context 经常断裂"的痛点，也是 Amux 相对 Cursor / 2code 类竞品的差异点之一。

### 与 Workbench 阶段的关系

Diff Panel 是**独立 track**，可以与 Workbench 阶段并行推进，**不阻塞 MVP 主路径**：

| Workbench 阶段 | Diff Panel 对应里程碑 |
|---|---|
| 阶段 1（任务账本 MVP） | Diff Panel Day 1-3：`git status` 轮询 + sidebar 徽标 |
| 阶段 2（Command Palette 接入） | Diff Panel Day 4-7：preview panel `DiffMode` 渲染 |
| 阶段 3（任务上下文拼装） | Diff Panel Day 8-10：stage / commit |
| 阶段 4（Workbench Sidebar） | Diff Panel Day 11-12：push + 错误显示 |
| 阶段 5（Agent 完成协议） | Diff Panel Day 13-14：快捷键 + 跨平台测试 |

### 与 Workbench 的集成点（V1 之后再做）

- task `done` 事件触发 Diff Panel 的"未 commit 改动"提醒
- proof 里自动附上 `git diff --stat` 摘要
- commit message 自动带 `task-id: <id>` trailer，便于 audit 时把 commit 反查回 task

### 不在本计划讨论

完整规格（功能边界、架构、UX 草图、风险缓解、验收）见 `docs/diff-panel-design.md`。本节只声明 Diff Panel 是 Workbench 计划之外、可并行推进的兄弟交付物。
