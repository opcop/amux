# AMUX 研发设计方案

## 1. 文档目标

本文档用于指导 `AMUX` 的首阶段研发落地，目标是定义一套可执行的技术方案，支持以下产品方向：

- `Windows-first`
- `cross-platform-ready`
- 面向 `AI Coding CLI` 工作流
- 统一管理 `workspace / split panes / tabs / terminals / agents / file tree / editor / markdown preview`

本文档优先面向研发落地，而不是市场介绍。重点包括：

- 产品边界
- 模块拆分
- 核心数据结构
- 平台抽象
- Windows / WSL 技术路线
- 开发阶段与验收标准
- 首批技术 spike

***

## 2. 产品定义

### 2.1 产品定位

`AMUX` 是一个 `Windows-first` 的 AI Coding Workspace 桌面应用。它不是单纯的终端复用器，也不是完整 IDE，而是围绕 AI Coding 命令行工具构建的统一工作台。

核心价值：

- 统一管理多个项目工作区
- 统一管理多个终端和多个 Agent 会话
- 为 Windows + WSL 开发场景提供原生工作流
- 提供轻量文件树、轻量编辑器和 Markdown 预览能力

### 2.2 目标用户

优先服务以下用户群：

- 在 Windows 上开发、并依赖 WSL2 的开发者
- 已经使用 `codex`、`claude code`、`opencode`、`aider` 等 CLI 工具的用户
- 希望保留 terminal-native 工作方式，但又希望获得多面板桌面工作台的用户

### 2.3 首版范围

首版功能范围：

- Workspace 管理
- Split panes + tabs 布局系统
- 普通 shell 终端
- AI Coding CLI 探测与一键启动
- 文件树浏览与过滤
- 文本编辑
- Markdown 预览
- Session 持久化与恢复

首版不做：

- 完整 LSP
- Git GUI
- 插件系统
- 高级 diff / merge
- 复杂浏览器集成
- 远程容器或 SSH workspace

***

## 3. 设计原则

### 3.1 架构原则

- `core` 与 `ui` 强隔离
- 平台能力通过 trait 抽象，不直接渗透到 UI
- Windows 首版优先，但不写死平台分支
- session 与布局模型从第一天稳定化
- Agent 作为一等公民，不与普通 terminal 混用抽象

### 3.2 交互原则

- 当前焦点 pane 是默认操作目标
- 新建 terminal / agent / file tree / editor 都默认落到当前 pane
- workspace 是第一层上下文，pane/tab 是第二层上下文
- 首版先做稳定和清晰，不抢先做复杂拖拽和花式交互

### 3.3 工程原则

- 优先验证高风险基础能力，再堆 UI
- 不先做浏览器
- 不先做复杂 IDE 能力
- 对 `Windows + WSL` 的路径和 cwd 从第一天建模

***

## 4. 现有参考与约束

当前仓库内的第三方参考：

- `third_party/limux`
- `third_party/gpui-component`

### 4.1 limux 的参考价值

`limux` 已有以下可参考思路：

- workspace 状态与持久化模型
- pane / split / tab 结构
- terminal + browser tab 组合方式
- session restore 思路

适合参考的文件：

- `third_party/limux/rust/limux-host-linux/src/layout_state.rs`
- `third_party/limux/rust/limux-host-linux/src/pane.rs`
- `third_party/limux/rust/limux-host-linux/src/window.rs`

### 4.2 limux 的限制

`limux` 当前明显偏 Linux GTK 方案，不适合作为 Windows 首版的技术底座。它更适合提供：

- 产品交互参考
- 状态建模参考
- terminal workspace 产品边界参考

### 4.3 gpui-component 的参考价值

`gpui-component` 可为上层桌面 UI 提供：

- `tree`
- `tabs`
- `resizable`
- `editor`
- `markdown`
- `webview`

适合承担：

- 桌面 UI 壳
- 文件树面板
- 编辑器视图
- Markdown 预览
- pane/tab 组织

### 4.4 当前结论

- 参考 `limux` 的交互模型和 session 思路
- 使用 `gpui-component` 作为候选 UI 组件基础
- 不沿用 `limux` 的 GTK/Ghostty/Linux 技术实现

***

## 5. 总体架构

建议拆成 7 个 crate / 模块。

```text
apps/desktop
crates/amux-core
crates/amux-platform
crates/amux-agent
crates/amux-workspace
crates/amux-session
crates/amux-ui
```

### 5.1 模块职责

#### apps/desktop

桌面应用入口。

负责：

- 启动 GPUI 应用
- 依赖装配
- 创建主窗口
- 主题与资源初始化

#### amux-core

纯业务状态层，不依赖平台 API，不依赖 UI。

负责：

- id 定义
- workspace 状态
- layout tree
- pane / tab / surface 状态
- commands
- domain events

#### amux-platform

平台抽象层，负责与系统交互。

负责：

- terminal backend
- process spawn
- path mapping
- filesystem backend
- Windows ConPTY
- Windows WSL 集成
- 未来 Unix PTY

#### amux-agent

AI Coding CLI 管理层。

负责：

- provider 模型
- agent discovery
- agent status
- launch profiles

#### amux-workspace

工作区和文件系统组织层。

负责：

- workspace 管理
- recent workspaces
- file tree
- filter
- watcher

#### amux-session

session 与布局持久化层。

负责：

- session schema
- 序列化与反序列化
- migration
- session store

#### amux-ui

UI 渲染与交互层。

负责：

- workspace sidebar
- pane grid
- tab strip
- command palette
- surface 视图渲染
- activity / notifications

### 5.2 依赖方向

必须保持单向依赖：

- `apps/desktop -> amux-ui`
- `amux-ui -> amux-core`
- `amux-ui -> amux-agent`
- `amux-ui -> amux-workspace`
- `amux-ui -> amux-session`
- `amux-agent -> amux-core`
- `amux-agent -> amux-platform`
- `amux-workspace -> amux-core`
- `amux-workspace -> amux-platform`
- `amux-session -> amux-core`
- `amux-platform` 尽量独立
- `amux-core` 不依赖其他业务 crate

***

## 6. 信息架构与交互模型

### 6.1 结构层次

AMUX 采用如下结构：

```text
Workspace
  -> LayoutTree
    -> Pane
      -> Tabs
        -> Surface
```

### 6.2 Workspace

一个 workspace 表示一个项目上下文，包含：

- 显示名称
- 工作区目标路径
- 当前布局树
- 当前激活 pane
- 最近文件
- 默认 Agent 配置
- 环境配置

### 6.3 LayoutTree

布局树只允许两种节点：

- `Split`
- `Pane`

`Split` 提供：

- 横向切分
- 纵向切分
- 比例

`Pane` 提供：

- 多 tab 容器
- 当前激活 tab
- 焦点目标

### 6.4 Surface

首版支持的 surface 类型：

- `TerminalSurface`
- `AgentSurface`
- `FileTreeSurface`
- `EditorSurface`
- `PreviewSurface`
- `WelcomeSurface`

二期可扩展：

- `BrowserSurface`
- `SearchSurface`
- `TaskSurface`
- `DiffSurface`

### 6.5 首版关键交互

必须优先支持：

- 新建 workspace
- 打开 Windows workspace
- 打开 WSL workspace
- 向右 split
- 向下 split
- 新建 terminal tab
- 新建 agent tab
- 新建 file tree tab
- 打开 editor
- 打开 markdown preview
- 切换 pane 焦点
- 切换 tab
- 保存与恢复 session

***

## 7. 核心数据结构

以下结构应优先在 `amux-core` 中稳定。

### 7.1 IDs

建议统一封装 id 类型，避免业务层直接乱用裸字符串。

```rust
pub struct WorkspaceId(pub String);
pub struct PaneId(pub String);
pub struct TabId(pub String);
pub struct SurfaceId(pub String);
pub struct TerminalSessionId(pub String);
pub struct AgentInstanceId(pub String);
```

### 7.2 WorkspaceTarget

Windows 版必须把路径目标建模成一等类型。

```rust
pub enum WorkspaceTarget {
    WindowsPath { path: std::path::PathBuf },
    WslPath { distro: String, path: String },
}
```

设计要求：

- 不要在系统内四处传绝对路径字符串
- editor / file tree / terminal 各自通过 `WorkspaceTarget + relative path` 协作

### 7.3 WorkspaceState

```rust
pub struct WorkspaceState {
    pub id: WorkspaceId,
    pub name: String,
    pub target: WorkspaceTarget,
    pub layout: LayoutNode,
    pub active_pane_id: PaneId,
    pub env_profile_id: Option<String>,
    pub default_agent_provider_id: Option<String>,
    pub recent_files: Vec<String>,
}
```

### 7.4 LayoutNode

```rust
pub enum LayoutNode {
    Split(SplitNode),
    Pane(PaneNode),
}
```

```rust
pub struct SplitNode {
    pub id: String,
    pub axis: SplitAxis,
    pub ratio: f32,
    pub first: Box<LayoutNode>,
    pub second: Box<LayoutNode>,
}
```

```rust
pub enum SplitAxis {
    Horizontal,
    Vertical,
}
```

```rust
pub struct PaneNode {
    pub pane_id: PaneId,
    pub tabs: Vec<TabState>,
    pub active_tab_id: TabId,
}
```

### 7.5 TabState

```rust
pub struct TabState {
    pub id: TabId,
    pub title: String,
    pub pinned: bool,
    pub surface: SurfaceState,
}
```

### 7.6 SurfaceState

```rust
pub enum SurfaceState {
    Terminal(TerminalSurfaceState),
    Agent(AgentSurfaceState),
    FileTree(FileTreeSurfaceState),
    Editor(EditorSurfaceState),
    Preview(PreviewSurfaceState),
    Welcome(WelcomeSurfaceState),
}
```

关键要求：

- `AgentSurfaceState` 和 `TerminalSurfaceState` 必须独立
- 即使首版 Agent tab 仍由 terminal 宿主承载，状态建模也不能混淆

### 7.7 TerminalSurfaceState

```rust
pub struct TerminalSurfaceState {
    pub surface_id: SurfaceId,
    pub session_id: Option<TerminalSessionId>,
    pub launch_profile: TerminalLaunchProfile,
    pub cwd: Option<String>,
    pub title_override: Option<String>,
}
```

### 7.8 AgentSurfaceState

```rust
pub struct AgentSurfaceState {
    pub surface_id: SurfaceId,
    pub session_id: Option<TerminalSessionId>,
    pub agent_instance_id: Option<AgentInstanceId>,
    pub provider_id: String,
    pub launch_mode: AgentLaunchMode,
    pub cwd: Option<String>,
}
```

### 7.9 FileTreeSurfaceState

```rust
pub struct FileTreeSurfaceState {
    pub surface_id: SurfaceId,
    pub root: WorkspaceTarget,
    pub filter: String,
    pub selected: Option<String>,
    pub expanded: std::collections::BTreeSet<String>,
    pub show_hidden: bool,
}
```

### 7.10 EditorSurfaceState

```rust
pub struct EditorSurfaceState {
    pub surface_id: SurfaceId,
    pub relative_path: String,
    pub language: Option<String>,
    pub dirty: bool,
    pub readonly: bool,
}
```

### 7.11 PreviewSurfaceState

```rust
pub struct PreviewSurfaceState {
    pub surface_id: SurfaceId,
    pub source_relative_path: String,
    pub kind: PreviewKind,
}
```

```rust
pub enum PreviewKind {
    Markdown,
    PlainText,
}
```

***

## 8. 平台抽象设计

平台抽象是首版最关键的工程边界。`amux-ui` 不应直接感知 ConPTY、WSL、Win32 细节。

### 8.1 TerminalBackend

```rust
pub trait TerminalBackend {
    fn create_session(
        &self,
        spec: TerminalLaunchSpec,
    ) -> anyhow::Result<TerminalSessionId>;

    fn write_input(
        &self,
        id: &TerminalSessionId,
        data: &[u8],
    ) -> anyhow::Result<()>;

    fn resize(
        &self,
        id: &TerminalSessionId,
        cols: u16,
        rows: u16,
    ) -> anyhow::Result<()>;

    fn kill(&self, id: &TerminalSessionId) -> anyhow::Result<()>;
}
```

### 8.2 TerminalLaunchSpec

```rust
pub struct TerminalLaunchSpec {
    pub target: WorkspaceTarget,
    pub shell: ShellKind,
    pub cwd: Option<String>,
    pub env: std::collections::BTreeMap<String, String>,
    pub title: Option<String>,
}
```

### 8.3 ShellKind

首版建议支持：

```rust
pub enum ShellKind {
    PowerShell,
    Cmd,
    WslDefault,
    WslDistro(String),
}
```

实际重点：

- `PowerShell`
- `WslDefault`
- `WslDistro`

`Cmd` 可保留，但不作为首版主要验证对象。

### 8.4 ProcessSpawner

建议将进程拉起与 terminal 进一步隔离，为未来 managed process 预留。

```rust
pub trait ProcessSpawner {
    fn spawn(&self, spec: ProcessLaunchSpec) -> anyhow::Result<SpawnedProcess>;
}
```

### 8.5 PathMapper

```rust
pub trait PathMapper {
    fn to_display_path(&self, target: &WorkspaceTarget) -> String;

    fn to_runtime_cwd(&self, target: &WorkspaceTarget) -> anyhow::Result<String>;

    fn map_file_for_editor(
        &self,
        workspace: &WorkspaceTarget,
        relative_path: &str,
    ) -> anyhow::Result<MappedFile>;
}
```

PathMapper 负责解决三类路径：

- Windows UI 展示路径
- WSL 运行时 cwd
- 实际文件读取路径

### 8.6 FsBackend

```rust
pub trait FsBackend {
    fn read_to_string(&self, file: &MappedFile) -> anyhow::Result<String>;
    fn write_string(&self, file: &MappedFile, content: &str) -> anyhow::Result<()>;
    fn read_dir(&self, target: &WorkspaceTarget, relative_path: &str) -> anyhow::Result<Vec<FsEntry>>;
}
```

***

## 9. Windows / WSL 技术路线

### 9.1 Windows 首版总策略

策略明确为：

- UI 运行在 Windows 桌面环境
- 本地 shell 通过 ConPTY
- WSL shell 通过 `wsl.exe`
- 路径映射通过 `WorkspaceTarget + PathMapper`

### 9.2 本地 shell

Windows 本地 shell 使用 ConPTY，优先验证：

- PowerShell 启动
- 输入输出正确
- resize 正确
- 长输出稳定
- 中文与 ANSI 控制序列处理可用

### 9.3 WSL shell

WSL shell 建议使用：

```text
wsl.exe -d <distro> --cd <path> <shell>
```

需要重点验证：

- 启动指定 distro
- cwd 正确切换
- resize 正常
- 退出和清理行为正确
- 在 WSL 项目目录中启动 Agent CLI 正常

### 9.4 Workspace 路径策略

必须从第一天区分：

- `WindowsPath`
- `WslPath`

典型样例：

- `D:\repo\app`
- `\\wsl$\Ubuntu\home\user\app`
- `/home/user/app`

建议规则：

- UI 中保留 workspace target 语义
- 文件树和 editor 按 target 解析
- terminal runtime cwd 由 PathMapper 统一给出

### 9.5 为什么必须先做 WSL 一等支持

因为 AI Coding CLI 在 Windows 真实使用场景里，大量工作都发生在：

- WSL 项目目录
- Linux shell 环境
- Linux 包管理和工具链环境

如果首版只支持 Windows 本地 PowerShell，会大幅降低产品实用性。

***

## 10. Agent 设计

### 10.1 目标

让 AI Coding CLI 成为一等公民，而不是要求用户每次手敲命令。

### 10.2 Provider 模型

```rust
pub struct AgentProvider {
    pub id: String,
    pub display_name: String,
    pub command: String,
    pub args_template: Vec<String>,
    pub detection: DetectionRule,
    pub supported_targets: Vec<ExecutionTarget>,
}
```

### 10.3 ExecutionTarget

```rust
pub enum ExecutionTarget {
    WindowsLocal,
    Wsl,
}
```

### 10.4 AgentStatus

```rust
pub enum AgentStatus {
    Installed,
    NotFound,
    Broken(String),
    NeedsAuth,
}
```

### 10.5 首版建议支持的 provider

- `codex`
- `claude`
- `opencode`
- `aider`

### 10.6 探测原则

不要只判断命令是否存在，探测至少分三层：

- 命令是否存在
- 是否可执行 `--help` 或 `--version`
- 是否缺失登录或必要环境

### 10.7 启动模式

```rust
pub enum AgentLaunchMode {
    AttachedTerminal,
    ManagedProcess,
}
```

首版全部先走：

- `AttachedTerminal`

即：

- 为 agent 创建一个 terminal-hosted session
- 自动执行对应命令

但架构上必须保留：

- `ManagedProcess`

为后续任务面板、结构化日志、执行状态监控留接口。

***

## 11. Workspace 与文件树设计

### 11.1 WorkspaceStore

建议定义统一工作区存储接口。

```rust
pub trait WorkspaceStore {
    fn list_recent(&self) -> anyhow::Result<Vec<WorkspaceRef>>;
    fn open(&self, target: WorkspaceTarget) -> anyhow::Result<WorkspaceState>;
    fn save_metadata(&self, workspace: &WorkspaceState) -> anyhow::Result<()>;
}
```

### 11.2 File Tree 首版能力

必须支持：

- root directory 展示
- expand / collapse
- filter
- open file
- show hidden 开关

可作为增强项：

- 读取 `.gitignore`
- 忽略常见大目录

### 11.3 Editor 首版能力

只做轻编辑器：

- 打开文本文件
- 保存
- dirty 状态
- 搜索
- syntax highlight
- markdown 编辑

不做：

- LSP
- refactor
- breakpoint
- 高复杂度多光标

### 11.4 Preview 首版能力

重点做 Markdown：

- 纯预览
- 编辑 / 预览切换
- 分栏预览

建议优先级高于浏览器。

***

## 12. UI 结构设计

### 12.1 主界面结构

建议布局：

```text
Top Bar / Command Actions
------------------------------------------
Workspace Sidebar | Main Pane Grid
                  | 
                  | Bottom Activity Panel
```

### 12.2 Workspace Sidebar

功能：

- workspace 列表
- 最近项目
- 新建 / 打开工作区
- 当前 workspace 标记

### 12.3 Main Pane Grid

基于 split tree 渲染：

- pane header
- tab strip
- active surface

### 12.4 Pane Header

首版建议动作：

- `+ Terminal`
- `+ Agent`
- `+ File Tree`
- `Split Right`
- `Split Down`
- `Close Pane`

### 12.5 Command Palette

必须在首版规划中预留。

首批命令：

- Open Workspace
- New Terminal
- New Agent
- Launch Codex
- Launch Claude
- Split Right
- Split Down
- Open File Tree
- Open Markdown Preview

***

## 13. Session 设计

### 13.1 目标

用户关闭应用后，下次打开应能恢复：

- workspace 列表
- active workspace
- pane / tab 布局
- surface 配置

### 13.2 Session 内容

建议保存：

- version
- active workspace id
- workspace states
- layout tree
- tab states
- UI 基础状态

不要保存：

- 旧进程 PID 的强绑定
- 无法可靠恢复的临时句柄

### 13.3 恢复策略

terminal / agent surface 的恢复策略应为：

- 恢复 tab 结构
- 尝试按 launch profile 重建会话
- 失败时以可见错误态占位，而不是静默丢失

### 13.4 Session 示例

```json
{
  "version": 1,
  "active_workspace_id": "ws-1",
  "workspaces": [
    {
      "id": "ws-1",
      "name": "demo",
      "target": {
        "kind": "wsl_path",
        "distro": "Ubuntu",
        "path": "/home/user/demo"
      },
      "layout": {}
    }
  ]
}
```

***

## 14. 推荐目录骨架

建议项目目录按以下方式初始化：

```text
amux/
├─ Cargo.toml
├─ rust-toolchain.toml
├─ README.md
├─ docs/
│  ├─ architecture.md
│  ├─ roadmap.md
│  ├─ session-schema.md
│  └─ spikes/
├─ apps/
│  └─ desktop/
├─ crates/
│  ├─ amux-core/
│  ├─ amux-platform/
│  ├─ amux-agent/
│  ├─ amux-workspace/
│  ├─ amux-session/
│  └─ amux-ui/
└─ assets/
```

### 14.1 `amux-core` 建议结构

```text
crates/amux-core/src/
├─ lib.rs
├─ ids.rs
├─ command.rs
├─ event.rs
├─ session/
├─ workspace/
├─ layout/
└─ surface/
```

### 14.2 `amux-platform` 建议结构

```text
crates/amux-platform/src/
├─ lib.rs
├─ shell.rs
├─ process.rs
├─ terminal.rs
├─ path_mapper.rs
├─ fs.rs
├─ windows/
│  ├─ mod.rs
│  ├─ conpty.rs
│  ├─ wsl.rs
│  └─ paths.rs
└─ unix/
   ├─ mod.rs
   └─ pty.rs
```

### 14.3 `amux-ui` 建议结构

```text
crates/amux-ui/src/
├─ lib.rs
├─ root.rs
├─ state.rs
├─ commands.rs
├─ components/
├─ surfaces/
└─ panels/
```

***

## 15. 研发阶段计划

## Phase 0: 技术 Spike

目标：优先验证高风险能力。

### P0-1 GPUI Windows 基础壳

验证点：

- 窗口创建
- 快捷键
- 中文输入
- 分栏拖拽

验收：

- 能稳定创建基础窗口并渲染基本组件

### P0-2 ConPTY Terminal Spike

验证点：

- 启动 PowerShell
- 输入输出
- resize
- 长输出
- 会话关闭

验收：

- 能稳定承载基础 shell 交互

### P0-3 WSL Terminal Spike

验证点：

- 指定 distro 启动
- 指定 cwd
- resize
- 中文输出

验收：

- 能从 Windows UI 稳定拉起 WSL shell

### P0-4 Agent TTY Spike

验证点：

- 在 terminal session 中运行一个目标 agent CLI
- 确认交互式 TTY 不异常

验收：

- 至少一个目标 agent 可稳定运行

### P0-5 Workspace Path Spike

验证点：

- WindowsPath 与 WslPath 均可驱动文件树
- editor 可读写
- terminal cwd 对应正确

验收：

- path mapping 方案成立

## Phase 1: Core + Session

目标：

- 建立基础状态模型
- 建立 session schema

交付：

- `WorkspaceState`
- `LayoutNode`
- `TabState`
- `SurfaceState`
- session save/load

## Phase 2: 最小 UI 壳

目标：

- workspace sidebar
- pane grid
- tab strip
- split 操作

交付：

- 可视化创建 pane / tab
- 布局可保存

## Phase 3: Windows Terminal

目标：

- ConPTY backend 接入
- PowerShell terminal surface

交付：

- 稳定可用的 Windows terminal pane

## Phase 4: WSL 支持

目标：

- WslPath workspace
- WSL terminal launch
- path mapping 打通

交付：

- WSL 项目可完整工作

## Phase 5: Agent 管理

目标：

- provider registry
- discovery
- 一键启动 agent

交付：

- 至少支持 `codex` 和 `claude` 两个 provider

## Phase 6: File Tree + Editor + Markdown Preview

目标：

- file tree
- editor
- markdown preview

交付：

- 围绕 workspace 的完整最小工作流

***

## 16. MVP 验收标准

满足以下条件时，定义为首版 MVP 可用：

- 用户可打开 Windows 或 WSL 项目作为 workspace
- 用户可创建多个 pane 和多个 tab
- 用户可在 pane 中启动普通 shell
- 用户可检测并启动已安装的 AI Coding CLI
- 用户可从文件树打开文件并保存
- 用户可预览 Markdown
- 用户重启应用后可恢复 workspace 和布局

***

## 17. 风险清单

### 17.1 ConPTY 稳定性风险

风险等级：高

说明：

- Windows terminal 承载是首版最核心基础设施

应对：

- Phase 0 优先 spike
- 在通过前不投入复杂上层功能

### 17.2 WSL 路径映射风险

风险等级：高

说明：

- WSL 路径、Windows UI 路径、文件读写路径可能混乱

应对：

- 建立 `WorkspaceTarget + PathMapper`
- 不全局传绝对字符串

### 17.3 GPUI Windows 稳定性风险

风险等级：中高

说明：

- 若窗口/输入/渲染稳定性不足，会影响整体方案

应对：

- 先做最小壳验证
- 控制 UI 复杂度

### 17.4 Agent CLI 兼容性风险

风险等级：中

说明：

- 不同 CLI 对 TTY、cwd、env 的要求不同

应对：

- provider 模型统一
- 先支持少量主流 CLI

### 17.5 过早扩展浏览器和 IDE 功能

风险等级：高

说明：

- 会稀释终端与 Agent 主线研发资源

应对：

- 浏览器后置到 `V1.1+`
- 首版只围绕 terminal-native workflow

***

## 18. 当前结论

从当前仓库条件和目标产品判断，最合理路线是：

- 交互层参考 `limux`
- UI 组件层优先评估 `gpui-component`
- 终端与平台层独立设计
- 以 `Windows + WSL + Agent` 为首版核心
- 先做稳定的 terminal workspace，再扩展更重功能

如果后续正式开工，建议下一步产出：

1. `crate 初始化骨架`
2. `amux-core` 数据结构草案代码
3. `Phase 0 spike 任务清单`
4. `session schema v1`

