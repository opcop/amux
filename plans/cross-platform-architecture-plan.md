# AMUX 跨平台架构改造蓝图

## 1. 文档目标

这份文档用于指导 `cross-platform-foundation` 分支上的架构改造工作，目标不是“把项目改成理论上的跨平台”，而是：

- 在不破坏现有 `Windows / WSL` 稳定能力的前提下
- 补齐 `macOS / Linux` 作为一等公民的基础模型
- 让可共享的逻辑真正共享
- 让必须平台隔离的能力明确隔离
- 为后续性能、稳定性、兼容性优化建立清晰边界

这份文档优先回答 5 个问题：

1. 哪些代码必须共享
2. 哪些代码必须按平台拆开
3. 现有架构中哪些地方最容易导致三平台互相污染
4. 改造顺序应该是什么
5. 如何保证这次改造不把 Windows 现有稳定能力带崩

---

## 2. 当前问题总结

结合当前代码现状，跨平台的主要问题不是“缺几个 `cfg`”，而是核心抽象仍然偏 `Windows-first`。

### 2.1 当前结构的优点

- crate 边界已经初步存在：
  - `amux-core`
  - `amux-platform`
  - `amux-agent`
  - `amux-workspace`
  - `amux-session`
  - `amux-ui`
- UI 门面、状态、控制器已经形成基础分层
- Windows / WSL 工作流已经具备一定稳定性
- `apps/desktop` 已经承担了桌面装配层的一部分职责

### 2.2 当前结构的主要问题

#### 2.2.1 Core 层混入平台语义

当前 `WorkspaceTarget` 只有：

- `WindowsPath`
- `WslPath`

当前 `ShellKind` 只有：

- `PowerShell`
- `Cmd`
- `WslDefault`
- `WslDistro(String)`

这意味着：

- 平台差异没有被封装在 platform 层
- session schema 被 Windows/WSL 语义锁定
- macOS/Linux 只能走“兼容分支”，不是正式模型

#### 2.2.2 UI 层知道太多平台细节

当前 GPUI 桌面层中直接包含了大量：

- WSL 路径转换
- 平台 shell 决策
- 平台工具探测
- startup 文件环境分支

这会导致：

- UI 代码越来越难维护
- 平台 bug 和交互 bug 混在一起
- 三平台行为难以做一致性校验

#### 2.2.3 持久化存在双状态源

当前同时存在：

- `session.json`
- `layouts.json`

前者更接近业务状态，后者更接近桌面运行态。

这在单平台阶段还能凑合，但在三平台扩展后会产生：

- 恢复流程不一致
- 某平台 layout 可恢复，另一平台业务状态不可恢复
- 数据迁移复杂度上升

#### 2.2.4 启动与能力探测未完全产品化

当前仍有：

- demo bootstrap 默认执行
- agent 检测存在硬编码 installed
- 桌面层轮询承担了过多平台运行时职责

这些问题在多平台下会放大。

---

## 3. 跨平台总原则

本次改造遵守以下原则。

### 3.1 只增不炸

第一阶段尽量通过“新增抽象层、适配旧实现”的方式改造，不主动重写 Windows 已稳定的业务链路。

### 3.2 Core 只保留平台无关语义

`amux-core` 只保留：

- workspace
- session
- layout
- pane/tab/surface
- command/event
- capability 描述

不再保留宿主平台细节。

### 3.3 Platform 层负责系统差异

所有涉及系统差异的能力，统一沉到 `amux-platform`：

- PTY / process
- shell
- path
- clipboard
- browser host
- metrics
- workspace picker / open panel
- watcher

### 3.4 UI 层只消费抽象接口

`amux-ui` 和 `apps/desktop` 应尽量依赖 trait / service 接口，而不是平台枚举分支。

### 3.5 Windows 是迁移过程中的回归基线

因为当前 Windows 是最接近可上线的平台，所以：

- Windows 现有行为是回归保护线
- macOS/Linux 支持应该“向上兼容”现有 Windows 能力
- 不能为了抽象美观重写 Windows 稳定链路

---

## 4. 目标架构

目标结构仍然基于现有 workspace，但会重新明确职责。

```text
apps/desktop
  -> Desktop shell / app wiring / window host / renderer bootstrap

crates/amux-ui
  -> UI state, controller, view model, interaction orchestration

crates/amux-core
  -> Platform-agnostic domain model and commands

crates/amux-platform
  -> Platform service traits + per-platform implementations

crates/amux-agent
  -> Agent provider model + launch planning + capability bridge

crates/amux-workspace
  -> File tree, watcher orchestration, workspace discovery helpers

crates/amux-session
  -> Session schema, migration, serialization, persistence
```

---

## 5. 代码边界设计

### 5.1 `amux-core`

#### 职责

- session/workspace/layout/pane/tab/surface 的纯模型
- command/event
- UI 不可见但业务关键的 invariant
- 平台 capability 的抽象表达

#### 不应该包含

- `windows / wsl / macos / linux` 直接语义
- 原生路径转换逻辑
- shell 可执行程序选择
- PTY / process / clipboard / browser 细节

#### 建议新增抽象

```rust
pub enum WorkspaceLocation {
    Local { path: PathBuf },
    Virtual { scheme: String, authority: Option<String>, path: String },
}
```

或更偏运行时表达：

```rust
pub enum WorkspaceTarget {
    LocalPath { path: PathBuf },
    RemotePath { kind: RemoteKind, path: String },
}
```

这里的关键不是名字，而是：

- `LocalPath` 必须成为一等公民
- `WSL` 不再作为全局 workspace 模型的唯一非本地类型

#### Shell 抽象建议

```rust
pub enum ShellKind {
    SystemDefault,
    PowerShell,
    Cmd,
    Bash,
    Zsh,
    Fish,
    WslDefault,
    WslDistro(String),
    Custom(String),
}
```

### 5.2 `amux-platform`

这是本次改造的重点。

#### 结构建议

```text
crates/amux-platform/src/
  lib.rs
  services.rs
  capabilities.rs
  common/
  windows/
  macos/
  linux/
  unix/
```

#### 建议的 trait 边界

##### HostPlatform

```rust
pub trait HostPlatform: Send + Sync {
    fn id(&self) -> PlatformId;
    fn capabilities(&self) -> PlatformCapabilities;
    fn terminal(&self) -> Arc<dyn TerminalService>;
    fn filesystem(&self) -> Arc<dyn FsService>;
    fn paths(&self) -> Arc<dyn PathService>;
    fn clipboard(&self) -> Arc<dyn ClipboardService>;
    fn browser(&self) -> Arc<dyn BrowserService>;
    fn metrics(&self) -> Arc<dyn MetricsService>;
    fn workspace_dialogs(&self) -> Arc<dyn WorkspaceDialogService>;
}
```

##### TerminalService

```rust
pub trait TerminalService: Send + Sync {
    fn create_session(&self, spec: TerminalLaunchSpec) -> Result<TerminalSessionId, String>;
    fn write_input(&self, id: &TerminalSessionId, data: &[u8]) -> Result<(), String>;
    fn resize(&self, id: &TerminalSessionId, cols: u16, rows: u16) -> Result<(), String>;
    fn kill(&self, id: &TerminalSessionId) -> Result<(), String>;
    fn metadata(&self, id: &TerminalSessionId) -> Result<TerminalSessionMetadata, String>;
}
```

##### PathService

```rust
pub trait PathService: Send + Sync {
    fn display_path(&self, target: &WorkspaceTarget) -> String;
    fn runtime_cwd(&self, target: &WorkspaceTarget) -> Result<String, String>;
    fn map_editor_file(&self, target: &WorkspaceTarget, relative: &str) -> Result<MappedFile, String>;
}
```

##### WorkspaceDialogService

```rust
pub trait WorkspaceDialogService: Send + Sync {
    fn pick_folder(&self) -> Result<Option<PathBuf>, String>;
}
```

#### 平台实现原则

##### Windows

- 保留现有 ConPTY / WSL 路径能力
- 不改现有稳定命令行为
- 先适配到 trait，不先重写实现

##### macOS

- 提供原生本地路径 workspace 能力
- 使用 Unix PTY / system shell
- 提供原生 folder picker / clipboard / browser host 能力

##### Linux

- Unix PTY 与 shell 可最大化复用
- X11/Wayland 差异仅留在 window / browser / clipboard 细节层

### 5.3 `amux-ui`

#### 职责

- `DesktopApp`
- `AppController`
- `UiState`
- `AppSnapshot`
- 视图与交互状态

#### 改造方向

当前 controller 不应再直接 new 平台 backend，而应依赖注入：

```rust
pub struct AppController {
    platform: Arc<dyn HostPlatform>,
    registry: Arc<dyn AgentRegistry>,
    session_store: Arc<dyn SessionStore>,
}
```

#### controller 只做编排

controller 应负责：

- 业务动作编排
- session/load/save
- workspace/open/activate
- agent/file/browser/open action

controller 不应负责：

- 判断当前是 Windows 还是 macOS
- 决定 shell 可执行程序路径
- 直接处理 WSL path conversion

### 5.4 `apps/desktop`

#### 职责

- 创建 `HostPlatform`
- 创建 `DesktopApp`
- 注入 renderer / window integration
- 绑定 GPUI / platform runtime

#### 建议

把当前一些直接散落在 GPUI 层的系统逻辑往 `HostPlatform` 装配收口，例如：

- 默认 shell 选择
- vibe tool detection
- browser host 初始化策略
- 打开文件夹对话框

---

## 6. 哪些逻辑应共享，哪些应独立

### 6.1 必须共享的部分

这些逻辑如果按平台重复实现，后期一定会烂掉：

- session 模型
- workspace 模型
- layout tree
- pane/tab/surface 模型
- command/event
- snapshot/view-model
- agent provider catalog
- autosave 策略
- command palette/filter logic
- activity/notification 规则

### 6.2 必须平台隔离的部分

- PTY / process spawn
- shell 启动命令构造
- 路径映射
- clipboard image/text 读写
- browser/webview 宿主
- folder picker / open dialogs
- system metrics
- watcher 实现细节
- OS capability 探测

### 6.3 可共享但需要分层的部分

- tool detection
  - catalog 共享
  - 查找方式平台化
- startup commands
  - 文件格式共享
  - shell/path 注入平台化
- workspace restore
  - schema 共享
  - runtime rehydrate 平台化

---

## 7. Session 与持久化重构原则

### 7.1 目标

把当前两套状态源收敛到“单一业务真相 + 可重建运行态”。

### 7.2 建议方案

#### 方案 A：以 session 为唯一真相

- `session.json` 保存：
  - workspace 列表
  - active workspace
  - layout model
  - tab/surface state
  - UI prefs
- desktop runtime 中的 terminal/browser/webview 是运行态
- 应用启动时从 session 重建 runtime

这是长期推荐方案。

#### 方案 B：短期过渡兼容

第一阶段保留 `layouts.json`，但明确：

- 它只是运行态缓存
- 不是权威业务状态
- controller/session 恢复失败时可丢弃 runtime cache

### 7.3 迁移要求

- 必须支持旧 `session.json` 自动迁移
- 必须保证旧 Windows session 可恢复
- 不能因为新增 macOS/Linux variant 导致旧版本数据不可读

---

## 8. 启动流程目标

### 8.1 当前问题

当前默认 demo bootstrap 不适合生产，也不适合跨平台。

### 8.2 目标启动流

```text
App Launch
  -> Detect host platform
  -> Build HostPlatform services
  -> Load session
  -> Migrate session if needed
  -> Restore workspaces/layout
  -> If empty session:
       show welcome / open-folder flow
  -> Start background services
```

### 8.3 禁止事项

- 不再默认打开 demo workspace
- 不再默认拉起 agent
- 不再默认打开 README

---

## 9. 性能与稳定性原则

### 9.1 不再让 frame loop 承担过多职责

当前 16ms 轮询主循环承担了：

- PTY dirty 检测
- browser bounds sync
- terminal activity polling
- toast 过期
- autosave

跨平台后这会更难控。

### 9.2 目标策略

#### 高频事件驱动

- terminal output
- focus change
- pane resize
- browser visible change

#### 低频定时器

- autosave
- metrics refresh
- activity aggregation

#### 显式重绘触发

- layout change
- tab switch
- selection / input state change

### 9.3 稳定性原则

- 所有平台能力都应暴露 capability
- UI 不能假设 browser 一定可用
- UI 不能假设 clipboard image 一定可用
- UI 不能假设 folder picker 一定可用
- 所有失败都要回到用户可见状态，而不是只写 stderr

---

## 10. Windows 回归保护点

本次改造中，以下能力视为 Windows 回归保护线。

### 10.1 必须保持行为不变的链路

- Windows 本地 terminal 启动
- WSL workspace 打开
- WSL path mapping
- tool detection 与 launch
- startup commands
- session 恢复
- split/tab/pane 交互
- screenshot paste 到路径

### 10.2 高风险文件

以下文件可以改，但要视为高风险：

- `crates/amux-core/src/workspace/target.rs`
- `crates/amux-core/src/surface/terminal.rs`
- `crates/amux-platform/src/path_mapper.rs`
- `crates/amux-platform/src/terminal/backend.rs`
- `crates/amux-ui/src/controller.rs`
- `apps/desktop/src/gpui_entry.rs`
- `apps/desktop/src/gpui_vibe_tools.rs`
- `apps/desktop/src/gpui_workspace_persistence.rs`

### 10.3 回归策略

每次修改上述区域后至少验证：

1. `cargo test -q`
2. `cargo check -q -p amux-desktop --features gpui`
3. Windows smoke cases

建议 smoke cases：

- 打开本地 workspace
- 打开 WSL workspace
- split pane / new tab
- 启动 Codex / Claude
- 运行 startup 文件
- 关闭并恢复 session
- smart paste 图片路径

---

## 11. 分阶段迁移计划

### Phase 0：冻结目标与回归线

目标：

- 建立本分支
- 确认 Windows 回归保护线
- 明确迁移不要先碰的稳定能力

完成标准：

- 本文档确认
- 建立任务拆分基线

### Phase 1：抽象先行，不改行为

目标：

- 在 `amux-platform` 中引入 service traits
- 用现有 Windows 实现去适配 traits
- `apps/desktop` 改成注入 platform services

完成标准：

- Windows 行为与当前基本一致
- 代码不再到处 `cfg!(target_os)` 决策核心行为

### Phase 2：重构 core 模型为三平台可承载

目标：

- 引入 `LocalPath` 一等抽象
- 扩展 `ShellKind`
- 调整 session schema 与 migration

完成标准：

- 旧 session 可读
- Windows 路径与 WSL 路径兼容
- macOS/Linux 数据模型可落地

### Phase 3：接通 macOS 基础能力

目标：

- 本地 workspace open
- 本地 shell / PTY
- path mapper
- clipboard
- session restore

完成标准：

- macOS 能稳定完成“打开项目 -> 开终端 -> split/tab -> 恢复”

### Phase 4：接通 Linux 基础能力

目标：

- Unix terminal / local workspace / restore
- browser / clipboard capability gate
- 桌面环境差异兼容

完成标准：

- Linux 具备基础使用能力

### Phase 5：统一运行时和性能治理

目标：

- 把轮询改为事件驱动 + 低频定时器
- 合并或弱化 `layouts.json`
- 补齐统一错误展示

完成标准：

- 三平台运行时职责更清晰
- 主循环负担下降

---

## 12. 第一批建议任务

建议先拆成下面这些任务。

### Task 1

新增 `PlatformId / PlatformCapabilities / HostPlatform` 抽象。

### Task 2

把当前 `RealTerminalBackend / RealFsBackend / PathMapper` 包进 Windows platform adapter。

### Task 3

把 `AppController` 改成依赖注入 platform services，而不是内部自行 new backend。

### Task 4

设计新的 `WorkspaceTarget` 与 `ShellKind` 演进方案，并写 migration 草案。

### Task 5

去掉默认 demo 启动流，改成正式启动流。

### Task 6

增加 macOS 本地 workspace + shell 的最小实现。

### Task 7

增加 Linux 本地 workspace + shell 的最小实现。

### Task 8

建立 Windows 回归 smoke checklist 文档与脚本。

---

## 13. 结论

这次跨平台改造的正确方向不是：

- 到处补 `cfg`
- 为了抽象重写稳定实现
- 在 UI 层继续硬编码平台分支

正确方向是：

- `core` 去平台化
- `platform` 真正承担平台差异
- `ui` 只做编排和渲染
- `desktop` 只做装配
- Windows 作为回归基线
- macOS/Linux 通过新增实现接入

如果这个方向执行得当，AMUX 后续会得到三件非常重要的东西：

1. 三平台功能演进不会互相污染
2. 平台问题能在明确边界内定位
3. 后续做性能、稳定性、兼容性优化时有真正可维护的架构基础
