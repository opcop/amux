# AMUX 架构地图

## 1. 文档目的

这份文档不是产品说明，也不是任务清单，而是：

`一份按调用链组织的代码地图。`

它用来帮助接手开发者快速回答这些问题：

- 一个用户动作是如何穿过系统的
- 哪些模块负责状态，哪些模块负责编排，哪些模块只负责渲染
- 新需求应该落在哪一层
- 当前哪些地方是稳定边界，哪些地方仍可继续演进

如果你已经看过：

- [developer-handoff.md](/mnt/d/repository/arden/Ai/ide/amux/plans/developer-handoff.md)
- [next-tasks.md](/mnt/d/repository/arden/Ai/ide/amux/plans/next-tasks.md)

那这份文档就是第三块拼图：帮助你真正顺着代码走一遍。

---

## 2. 系统总图

当前项目可以用下面这张逻辑图理解：

```text
User Action
  -> apps/desktop (gpui/text entry)
  -> amux-ui::DesktopApp
  -> amux-ui::AppController
  -> amux-ui::UiState
  -> amux-core::SessionState / WorkspaceState / Layout / Surface
  -> amux-platform / amux-agent / amux-workspace / amux-session
  -> AppSnapshot
  -> GpuiWindowModel / TextRenderer output
  -> Window
```

再换一种按职责分层的看法：

```text
UI Entry Layer
  apps/desktop

UI Orchestration Layer
  amux-ui::root
  amux-ui::controller
  amux-ui::state
  amux-ui::commands

Domain Layer
  amux-core

Infrastructure / Service Layer
  amux-platform
  amux-agent
  amux-workspace
  amux-session

Rendering Layer
  amux-ui::render::text
  amux-ui::render::gpui
  apps/desktop gpui view modules
```

---

## 3. 顶层调用关系

### 3.1 桌面入口

桌面程序入口在：

- [apps/desktop/src/main.rs](/mnt/d/repository/arden/Ai/ide/amux/apps/desktop/src/main.rs)

这里按 feature 分两条路径：

- 默认路径：text entry
- `gpui` feature：gpui entry

也就是说：

- `apps/desktop` 只负责选择入口和装配桌面宿主
- 它不负责业务状态

### 3.2 text 路径

text 路径主要用于：

- 验证 snapshot
- 验证命令路由
- 无图形依赖的快速 smoke test

入口文件：

- [apps/desktop/src/text_entry.rs](/mnt/d/repository/arden/Ai/ide/amux/apps/desktop/src/text_entry.rs)

### 3.3 gpui 路径

图形路径入口：

- [apps/desktop/src/gpui_entry.rs](/mnt/d/repository/arden/Ai/ide/amux/apps/desktop/src/gpui_entry.rs)

当前这个文件的职责应该理解为：

- 窗口壳装配器
- 交互事件绑定点
- 各面板视图的组合点

不要把它理解成业务层。

---

## 4. UI 主链路

这里是最重要的一段。

### 4.1 `DesktopApp`

入口：

- [crates/amux-ui/src/root.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-ui/src/root.rs)

`DesktopApp` 是 UI 层对外的统一门面。你可以把它理解成：

- 对 `UiState` 的持有者
- 对 `AppController` 的持有者
- 对外暴露有限操作接口的 facade

当前它大致负责：

- `bootstrap_demo()`
- `run_command(...)`
- `dispatch(...)`
- `activate_workspace(...)`
- `split_active_pane(...)`
- `focus_pane(...)`
- `activate_tab(...)`
- `close_tab(...)`
- `render_with(...)`
- `snapshot()`

这层不要承载复杂业务逻辑，它应该继续保持“薄门面”。

### 4.2 `AppController`

核心编排入口：

- [crates/amux-ui/src/controller.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-ui/src/controller.rs)

这是当前最值得先读的文件之一。

它负责的事情包括：

- session 恢复与持久化
- demo bootstrap
- command router 执行
- agent 启动
- 文件打开
- 工作区切换
- pane/tab 操作编排
- snapshot 富化

你可以把它理解成：

`UI use-case layer`

也就是：

- 不直接定义 domain 结构
- 不直接渲染 UI
- 负责串联服务层和状态层

### 4.3 `UiState`

状态入口：

- [crates/amux-ui/src/state.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-ui/src/state.rs)

`UiState` 当前包含：

- `session`
- `command_palette_open`
- `last_error`
- `activity_log`

这层主要负责两件事：

1. 承接 UI action 并转交给 `amux-core`
2. 将当前状态投影为 `AppSnapshot`

它并不负责调 agent、fs、session store，这些都在 controller 里。

### 4.4 `AppSnapshot`

`AppSnapshot` 是当前 UI 架构里非常关键的桥梁。

它的意义是：

- controller 可以产出一个对渲染友好的视图模型
- text/gpui 两条渲染路径共享这份中间态

当前它包含：

- workspace list
- agent list
- file list
- open files
- active surface
- active workspace layout snapshot
- command palette state
- error
- activity log

这意味着：

如果一个新 UI 需求要更多数据，优先考虑先补 `AppSnapshot`，再补渲染层。

---

## 5. Domain 主链路

### 5.1 `amux-core` 的角色

`amux-core` 是项目的稳定中心。

其关键职责：

- 定义核心模型
- 定义 command/event
- 定义 session/workspace/layout 的变更语义

关键入口：

- [crates/amux-core/src/lib.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-core/src/lib.rs)
- [crates/amux-core/src/command.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-core/src/command.rs)
- [crates/amux-core/src/session/ops.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-core/src/session/ops.rs)
- [crates/amux-core/src/workspace/ops.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-core/src/workspace/ops.rs)
- [crates/amux-core/src/layout/ops.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-core/src/layout/ops.rs)

### 5.2 Command -> SessionState

`UiState::dispatch(...)` 的本质是：

```text
UiAction
  -> to_core_command()
  -> SessionState::apply(command)
  -> Vec<Event>
```

也就是：

- UI action 是 UI 层的动作格式
- core command 是 domain 层的动作格式
- session apply 是真正执行状态变更的地方

### 5.3 Workspace / Layout / Surface

当前最重要的 domain 模型关系是：

```text
SessionState
  -> WorkspaceState
    -> LayoutNode
      -> PaneNode
        -> TabState
          -> SurfaceState
```

这条链路直接决定了：

- split 是作用在 layout/pane 上
- close/activate tab 是作用在 pane/tab 上
- 内容类型是由 surface 决定的

### 5.4 为什么这个模型不能随便改

因为当前 UI、session、controller、renderer 都已经默认这套结构存在。

如果你要新增功能，优先想的是：

- 是不是新增一种 surface
- 是不是在 workspace/layout ops 上加一种命令

而不是推翻层次关系。

---

## 6. 服务层调用关系

当前 controller 会用到四个服务型 crate：

### 6.1 `amux-agent`

入口：

- [crates/amux-agent/src/registry.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-agent/src/registry.rs)
- [crates/amux-agent/src/launch.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-agent/src/launch.rs)

用途：

- 列出 provider
- 获取 detection 状态
- 根据 workspace target 生成 launch plan

controller 中的使用场景：

- `launch_agent(...)`
- `snapshot()` 里构建 agent list

### 6.2 `amux-workspace`

入口：

- [crates/amux-workspace/src/manager.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-workspace/src/manager.rs)

用途：

- list files
- open file
- save file

controller 中的使用场景：

- `open_file_in_active_workspace(...)`
- `snapshot()` 里构建 file list

### 6.3 `amux-session`

入口：

- [crates/amux-session/src/store.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-session/src/store.rs)

用途：

- load session
- save session

controller 中的使用场景：

- `restore_session(...)`
- `persist_session(...)`

### 6.4 `amux-platform`

入口：

- [crates/amux-platform/src/terminal.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-platform/src/terminal.rs)
- [crates/amux-platform/src/fs.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-platform/src/fs.rs)
- [crates/amux-platform/src/path_mapper.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-platform/src/path_mapper.rs)

用途：

- terminal backend
- fs backend
- path mapper

controller 中的使用场景：

- agent 启动时注入 terminal backend
- 文件打开/内容富化时使用 fs backend
- workspace target 到显示/运行路径的映射

---

## 7. 渲染层调用关系

### 7.1 渲染抽象

入口：

- [crates/amux-ui/src/render/mod.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-ui/src/render/mod.rs)

当前有两条实现：

- [crates/amux-ui/src/render/text.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-ui/src/render/text.rs)
- [crates/amux-ui/src/render/gpui.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-ui/src/render/gpui.rs)

它们都吃同一份 `AppSnapshot`，但产物不同：

- text renderer -> 文本 UI 结构
- gpui renderer -> `GpuiWindowModel`

### 7.2 `GpuiWindowModel`

`GpuiWindowModel` 是桌面图形路径的中间模型。

它当前提供：

- workspace items
- agent items
- file items
- open file items
- tab items
- pane items
- active surface
- active workspace name
- last activity
- command palette state
- 若干 section summary

它的意义是：

- `apps/desktop` 无需直接理解 domain 结构
- `apps/desktop` 只和窗口模型打交道

### 7.3 apps/desktop 的视图模块

当前 GPUI 相关视图拆分为：

- [apps/desktop/src/gpui_entry.rs](/mnt/d/repository/arden/Ai/ide/amux/apps/desktop/src/gpui_entry.rs)
- [apps/desktop/src/gpui_surface_views.rs](/mnt/d/repository/arden/Ai/ide/amux/apps/desktop/src/gpui_surface_views.rs)
- [apps/desktop/src/gpui_status_bar.rs](/mnt/d/repository/arden/Ai/ide/amux/apps/desktop/src/gpui_status_bar.rs)
- [apps/desktop/src/gpui_command_bar.rs](/mnt/d/repository/arden/Ai/ide/amux/apps/desktop/src/gpui_command_bar.rs)
- [apps/desktop/src/gpui_command_palette.rs](/mnt/d/repository/arden/Ai/ide/amux/apps/desktop/src/gpui_command_palette.rs)

建议理解方式：

- `gpui_entry.rs`：窗口装配 + 事件绑定
- `gpui_surface_views.rs`：active surface 内容视图
- `gpui_status_bar.rs`：底栏
- `gpui_command_bar.rs`：快捷命令区
- `gpui_command_palette.rs`：弹层命令入口

---

## 8. 三条最重要的运行链路

这一节是“如何顺着系统真正走一遍”的关键。

### 8.1 链路 A：命令执行链

以用户点击 command bar 中某个动作按钮为例：

```text
gpui_entry.rs
  -> on_command_click(...)
  -> DesktopApp::run_command(...)
  -> AppController::run_command(...)
  -> parse_command(...)
  -> UiAction / LaunchAgent / OpenFile / ShowHelp
  -> state mutation or service call
  -> persist_session(...)
  -> refresh_model()
  -> render_with(GpuiRenderer)
  -> new GpuiWindowModel
  -> cx.notify()
```

看这个链路时，建议重点看：

- [crates/amux-ui/src/commands.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-ui/src/commands.rs)
- [crates/amux-ui/src/controller.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-ui/src/controller.rs)
- [apps/desktop/src/gpui_entry.rs](/mnt/d/repository/arden/Ai/ide/amux/apps/desktop/src/gpui_entry.rs)

### 8.2 链路 B：UI action -> domain action 链

以 pane split 为例：

```text
UI click
  -> DesktopApp::split_active_pane(...)
  -> AppController::split_active_pane(...)
  -> UiAction::SplitPane
  -> UiState::dispatch(...)
  -> to_core_command()
  -> SessionState::apply(...)
  -> WorkspaceState / Layout ops
  -> Event list
  -> activity log
  -> persist_session(...)
```

重点看：

- [crates/amux-ui/src/state.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-ui/src/state.rs)
- [crates/amux-core/src/session/ops.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-core/src/session/ops.rs)
- [crates/amux-core/src/workspace/ops.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-core/src/workspace/ops.rs)

### 8.3 链路 C：文件打开链

以点击文件列表中的文件为例：

```text
gpui_entry.rs
  -> on_file_click(relative_path)
  -> DesktopApp::run_command("file open ...")
  -> AppController::open_file_in_active_workspace(...)
  -> WorkspaceService::open_file(...)
  -> FsBackend::read_to_string(...)
  -> create EditorSurfaceState
  -> UiAction::OpenSurface
  -> SessionState::apply(...)
  -> snapshot() enrich active surface
  -> GpuiWindowModel refresh
```

重点看：

- [crates/amux-workspace/src/manager.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-workspace/src/manager.rs)
- [crates/amux-ui/src/controller.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-ui/src/controller.rs)

### 8.4 链路 D：Agent 启动链

以点击 agent 列表中的 `claude` 为例：

```text
gpui_entry.rs
  -> on_agent_click("claude")
  -> DesktopApp::run_command("agent claude")
  -> AppController::launch_agent(...)
  -> AgentRegistry / AgentLauncher
  -> TerminalBackend::create_session(...)
  -> write bootstrap input
  -> create AgentSurfaceState
  -> OpenSurface
  -> SessionState::apply(...)
  -> snapshot() enrich active surface
  -> GpuiWindowModel refresh
```

重点看：

- [crates/amux-agent/src/launch.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-agent/src/launch.rs)
- [crates/amux-platform/src/terminal.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-platform/src/terminal.rs)
- [crates/amux-ui/src/controller.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-ui/src/controller.rs)

---

## 9. Snapshot 富化机制

这是当前架构里比较容易被忽视的一点。

`UiState::snapshot()` 并不负责构建完整 UI 视图，它只负责：

- 基础 workspace/layout/active surface 投影

真正的“可展示 UI 数据”是在 controller 里进一步富化的：

- 加 agent list
- 加文件列表
- 加 open files
- 给 active surface 加内容预览
- 给 terminal/agent 加 recent IO 预览

这一层在：

- [crates/amux-ui/src/controller.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-ui/src/controller.rs)

设计意义是：

- `UiState` 保持轻
- 服务访问留在 controller
- render 层只看完成后的 snapshot

后续如果需要：

- palette query 结果
- 更丰富的 preview 数据
- editor metadata

优先也走这条“snapshot 富化”路径。

---

## 10. 视图层的真实边界

当前 `gpui` 路径虽然已经能点击、能刷新，但它仍然应该视为：

`桌面壳层 + 专用只读 surface 视图`

还不是：

- 完整交互式 editor
- 完整 terminal renderer
- 完整 palette 系统

这意味着后续开发要遵守两个原则：

### 10.1 视图层负责显示和触发，不负责真正业务决策

视图可以做：

- on_click
- 调 controller 暴露的方法
- refresh model

视图不应该做：

- 直接处理 domain 树
- 自己拼装业务对象
- 自己绕过 controller 持久化 session

### 10.2 新视图先挂在 `GpuiWindowModel` 上

如果你新增一个新面板，例如：

- recent commands
- launch profiles
- workspace metadata

优先先问自己：

`这个数据是不是应该先进入 AppSnapshot / GpuiWindowModel？`

通常答案是“是”。

---

## 11. 常见改动该落在哪一层

这是最实用的一节。

### 11.1 新增一个按钮去 split pane

落点：

- `apps/desktop` 视图层
- `DesktopApp` / `AppController`

通常不需要改 `amux-core`，因为 split 语义已经存在。

### 11.2 新增一种 Surface

落点：

- `amux-core::SurfaceState`
- snapshot 映射
- render 模型
- 视图模块

如果这类 surface 需要服务支持，再扩展 platform/workspace/agent。

### 11.3 修改工作区路径模型

落点：

- `amux-core::WorkspaceTarget`
- `amux-platform::PathMapper`
- session serde/migration

这是高风险改动，不能只改 UI。

### 11.4 新增一个 palette 命令

落点：

- [crates/amux-ui/src/commands.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-ui/src/commands.rs)
- 如需新行为，再补 controller 和可能的 core action

### 11.5 新增真实后端能力

落点：

- `amux-platform`
- 适当扩展 controller 注入和调用

不要在 `apps/desktop` 直接接系统 API。

---

## 12. 当前最脆弱的几个点

### 12.1 `gpui_entry.rs` 仍然偏大

虽然已经拆掉一部分，但它仍然包含不少面板和交互绑定。

后续如果新增更多 UI，优先继续拆模块。

### 12.2 `AppController::new(...)` 目前仍然是固定注入

现在 controller 内部直接构造：

- static agent registry
- in-memory terminal backend
- in-memory fs backend
- default path mapper
- file session store

这使得：

- demo 很方便
- 但真实 backend 替换时需要继续演进注入方式

后续如果上真实 fs / terminal，建议优先改造这里。

### 12.3 snapshot 富化正在变重

controller 的 `snapshot()` 已经开始承担越来越多的视图准备逻辑。

这本身没错，但要留意：

- 不要在这里塞入过多 UI 专属格式化
- 如果某部分逻辑明显是 GPUI 专用，应下沉到 render/gpui

---

## 13. 推荐阅读顺序

如果你只想花 20 到 30 分钟建立完整心智模型，建议这样看代码：

### 第一轮：理解业务骨架

1. [crates/amux-core/src/command.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-core/src/command.rs)
2. [crates/amux-core/src/session/ops.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-core/src/session/ops.rs)
3. [crates/amux-ui/src/state.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-ui/src/state.rs)
4. [crates/amux-ui/src/controller.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-ui/src/controller.rs)

### 第二轮：理解桌面路径

1. [crates/amux-ui/src/render/gpui.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-ui/src/render/gpui.rs)
2. [apps/desktop/src/gpui_entry.rs](/mnt/d/repository/arden/Ai/ide/amux/apps/desktop/src/gpui_entry.rs)
3. [apps/desktop/src/gpui_surface_views.rs](/mnt/d/repository/arden/Ai/ide/amux/apps/desktop/src/gpui_surface_views.rs)

### 第三轮：理解服务层

1. [crates/amux-agent/src/launch.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-agent/src/launch.rs)
2. [crates/amux-workspace/src/manager.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-workspace/src/manager.rs)
3. [crates/amux-platform/src/terminal.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-platform/src/terminal.rs)
4. [crates/amux-session/src/store.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-session/src/store.rs)

---

## 14. 当前最推荐的理解方式

一句话总结当前项目的结构：

`amux-core 定义真相，amux-ui/controller 组织用例，amux-platform 等服务层提供能力，renderer 和 apps/desktop 只负责把结果显示出来。`

如果你始终按这个思路做改动，就不容易破坏架构。

如果你开始出现这些想法，就应该停一下：

- “这个数据我直接在 gpui 视图里算一下吧”
- “这个命令我直接在 entry 里改 session 吧”
- “这个 workspace 路径就先传字符串吧”

这些通常都是偏离当前设计的信号。

---

## 15. 当前结论

这套架构已经形成了比较清晰的主干：

- command/action 主干
- snapshot/render 主干
- workspace/agent/file 三条服务主链
- gpui/text 双渲染路径

接下来所有新增工作，都建议沿着现有调用链往前推进，而不是绕开它。

最稳的开发方式是：

1. 先判断改动属于哪一层
2. 只在那一层开口
3. 通过 snapshot 或 controller 把变化往上游送
4. 最后才改视图

这样后续接入真实终端、真实文件系统、真实 editor，成本都会低很多。
