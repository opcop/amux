# AMUX 开发交接文档

## 1. 文档目的

这份文档是给下一位接手开发者的快速上手材料。目标不是重复产品设想，而是把以下信息一次讲清楚：

- 这个项目为什么这样设计
- 当前仓库已经做到什么程度
- 现在哪些模块是真正可用的，哪些仍是骨架/占位
- 下一步应该从哪里下手，优先级如何排
- 之前踩过哪些坑，后续开发时应该避免什么

如果你是第一次接手本项目，建议按本文顺序阅读，然后先运行文末的验证命令，再开始改代码。

---

## 2. 项目背景与定位

`amux` 的目标不是做一个普通终端管理器，也不是马上做一个完整 IDE，而是做一个：

`Windows-first 的 AI Coding Workspace`

它的核心价值在于统一管理：

- Workspace
- 多 Pane / 多 Tab 布局
- 普通 Terminal
- AI Agent CLI 会话
- 文件树
- 轻量 Editor
- Markdown / Preview

当前产品定位已经明确收敛为：

- 首版优先服务 `Windows + WSL` 场景
- 架构保持 `cross-platform-ready`
- 优先做通工作流主链，不优先做复杂浏览器/LSP/插件系统

对应的设计原则是：

1. `Workspace` 是一级对象，不是一个普通路径字符串。
2. `Agent` 是一级对象，不是“只是往终端里敲一个命令”。
3. `Pane/Tab/Surface` 是稳定抽象，后续能力都应在这个模型上扩展。
4. UI 与平台实现必须解耦，不能把 ConPTY/WSL/GPUI 细节直接写进 domain 层。

---

## 3. 当前实现状态总览

截至本次交接，项目已经从“方案设计阶段”进入“可运行原型骨架阶段”。

当前已经打通的主链：

1. `workspace -> pane/tab/layout`
2. `workspace -> agent -> terminal backend -> agent tab`
3. `workspace -> file tree -> open file -> editor tab`
4. `session save/restore`
5. `text renderer + gpui renderer feature gate`
6. `gpui 最小桌面工作台壳 + 基础点击交互`

当前还没有真正完成的部分：

1. 真实 ConPTY / WSL 子进程会话
2. 真实终端渲染
3. 真实文本编辑
4. 真正的 Markdown 渲染引擎
5. command palette 输入/过滤
6. 真实 GPUI 组件级交互体系

换句话说：

- 当前仓库已经有比较稳的架构和用例流
- 也有可运行的 UI 原型
- 但大量能力仍是“正确抽象 + mock 后端 + 只读视图”

不要误判为“功能已经完成”，它更准确的状态是：

`架构骨架已成型，主链已验证，接下来进入真实能力替换阶段。`

---

## 4. 当前目录与模块职责

项目是一个 Rust workspace，核心目录如下：

```text
apps/desktop
crates/amux-core
crates/amux-platform
crates/amux-agent
crates/amux-workspace
crates/amux-session
crates/amux-ui
plans
```

### 4.1 `crates/amux-core`

职责：纯 domain 层。

这里定义了最重要的数据模型和状态操作：

- `WorkspaceState`
- `WorkspaceTarget`
- `LayoutNode / PaneNode / SplitNode`
- `TabState`
- `SurfaceState`
- `SessionState`
- `Command / Event`

关键文件：

- [crates/amux-core/src/command.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-core/src/command.rs)
- [crates/amux-core/src/layout/model.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-core/src/layout/model.rs)
- [crates/amux-core/src/layout/ops.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-core/src/layout/ops.rs)
- [crates/amux-core/src/session/model.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-core/src/session/model.rs)
- [crates/amux-core/src/session/ops.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-core/src/session/ops.rs)
- [crates/amux-core/src/workspace/ops.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-core/src/workspace/ops.rs)
- [crates/amux-core/src/workspace/target.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-core/src/workspace/target.rs)

当前状态：

- 数据模型稳定
- 基础布局命令已经可用
- session 级 apply 流程已经可用
- serde 序列化已接通

这层是整个项目最不能随便破坏的部分。后续开发如果需要新增功能，优先在这里扩展模型和操作语义，而不是在 UI 层打补丁。

### 4.2 `crates/amux-platform`

职责：平台抽象层。

主要包括：

- terminal backend trait
- in-memory terminal backend
- fs backend trait
- in-memory fs backend
- path mapper
- Windows/WSL 启动策略骨架

关键文件：

- [crates/amux-platform/src/terminal.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-platform/src/terminal.rs)
- [crates/amux-platform/src/fs.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-platform/src/fs.rs)
- [crates/amux-platform/src/path_mapper.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-platform/src/path_mapper.rs)
- [crates/amux-platform/src/windows/conpty.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-platform/src/windows/conpty.rs)
- [crates/amux-platform/src/windows/wsl.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-platform/src/windows/wsl.rs)
- [crates/amux-platform/src/windows/paths.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-platform/src/windows/paths.rs)

当前状态：

- 抽象已经立住
- mock backend 已可驱动 UI / workspace / agent 用例
- Windows/WSL 目前还是命令规划和路径映射骨架，不是真实会话实现

### 4.3 `crates/amux-agent`

职责：AI Agent CLI 的发现、注册、启动规划。

关键文件：

- [crates/amux-agent/src/provider.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-agent/src/provider.rs)
- [crates/amux-agent/src/registry.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-agent/src/registry.rs)
- [crates/amux-agent/src/launch.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-agent/src/launch.rs)

当前状态：

- 已内置 `codex` / `claude` / `opencode` / `aider`
- 可以根据 workspace target 生成 launch plan
- 可以通过 terminal backend 写入 bootstrap command
- 目前依赖 mock terminal backend 验证

### 4.4 `crates/amux-workspace`

职责：工作区文件服务。

关键文件：

- [crates/amux-workspace/src/manager.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-workspace/src/manager.rs)
- [crates/amux-workspace/src/file_tree.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-workspace/src/file_tree.rs)
- [crates/amux-workspace/src/filter.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-workspace/src/filter.rs)

当前状态：

- 已支持 list/open/save 的服务接口
- 文件树过滤有基础逻辑
- 当前主要配合 in-memory fs backend 使用

### 4.5 `crates/amux-session`

职责：session 编码和落盘。

关键文件：

- [crates/amux-session/src/codec.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-session/src/codec.rs)
- [crates/amux-session/src/store.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-session/src/store.rs)

当前状态：

- `SessionState <-> JSON` 已打通
- 文件存储已接通
- 缺失文件时可回退到默认空 session

### 4.6 `crates/amux-ui`

职责：UI 状态、控制器、快照、渲染抽象。

关键文件：

- [crates/amux-ui/src/controller.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-ui/src/controller.rs)
- [crates/amux-ui/src/state.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-ui/src/state.rs)
- [crates/amux-ui/src/root.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-ui/src/root.rs)
- [crates/amux-ui/src/commands.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-ui/src/commands.rs)
- [crates/amux-ui/src/render/text.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-ui/src/render/text.rs)
- [crates/amux-ui/src/render/gpui.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-ui/src/render/gpui.rs)

当前状态：

- `AppController` 已经成为用例编排中心
- `UiState` 管理 palette 开关、activity log、session 等前端状态
- `AppSnapshot` 可以为 text/gpui 两条渲染路径提供统一视图模型
- `GpuiWindowModel` 已经是结构化窗口模型，而不是纯文本拼接

### 4.7 `apps/desktop`

职责：桌面入口和 GPUI 视图装配。

关键文件：

- [apps/desktop/src/main.rs](/mnt/d/repository/arden/Ai/ide/amux/apps/desktop/src/main.rs)
- [apps/desktop/src/text_entry.rs](/mnt/d/repository/arden/Ai/ide/amux/apps/desktop/src/text_entry.rs)
- [apps/desktop/src/gpui_entry.rs](/mnt/d/repository/arden/Ai/ide/amux/apps/desktop/src/gpui_entry.rs)
- [apps/desktop/src/gpui_surface_views.rs](/mnt/d/repository/arden/Ai/ide/amux/apps/desktop/src/gpui_surface_views.rs)
- [apps/desktop/src/gpui_status_bar.rs](/mnt/d/repository/arden/Ai/ide/amux/apps/desktop/src/gpui_status_bar.rs)
- [apps/desktop/src/gpui_command_bar.rs](/mnt/d/repository/arden/Ai/ide/amux/apps/desktop/src/gpui_command_bar.rs)
- [apps/desktop/src/gpui_command_palette.rs](/mnt/d/repository/arden/Ai/ide/amux/apps/desktop/src/gpui_command_palette.rs)

当前状态：

- 默认 text 路径可运行
- `--features gpui` 可编译通过
- GPUI 路径已经有最小桌面壳和点击交互

---

## 5. 当前真实完成度

这一节很重要。不要把“概念支持”误判为“真实功能支持”。

### 5.1 已真实完成的部分

- Rust workspace / crate 结构已经稳定
- domain 模型与 session apply 逻辑已经可用
- pane/tab/split/close/focus/activate 等基础操作已可用
- session 可以真实落盘和恢复
- text renderer 可用
- gpui feature 下可以真实编译出窗口壳
- gpui 原型里已经有这些交互：
  - workspace 切换
  - agent 启动
  - 文件打开
  - split right / split down
  - active pane 切换
  - active tab 激活
  - active tab 关闭
  - command bar 点击执行
  - command palette 弹层点击执行
  - activity log 和 status bar

### 5.2 仍是 mock / 只读 / 骨架的部分

- terminal transcript 是从 `InMemoryTerminalBackend` 中取最近写入记录，不是真终端输出
- editor surface 当前是只读内容预览，不是真编辑器
- preview surface 当前是轻量文本/markdown 样式展示，不是真预览引擎
- ConPTY/WSL 仍然只是策略层，没有真实长连接子进程管理
- agent “启动”目前主要是 launch plan + bootstrap command 的演示链路
- command palette 还没有输入框和过滤逻辑

---

## 6. 当前 UI 原型状态

### 6.1 默认 text 路径

text renderer 主要用于：

- 快速验证快照结构
- 测试 split 层级
- 验证 command router

它不是最终产品方向，但在当前阶段很有用，不要轻易删。

### 6.2 GPUI 路径

`gpui` feature 是当前更接近产品形态的入口。

当前窗口结构大致是：

- 顶部 header
- 左侧 sidebar
  - workspaces
  - agents
  - files
- 主区
  - command bar
  - command palette
  - toolbar
  - active surface panel
  - open files panel
  - active tabs panel
  - panes panel
  - layout summary
  - metric cards
- 底部 activity panel
- 最底部 status bar

当前 active surface 视图已经分化为几种：

- `editor`
- `preview`
- `terminal / agent`
- `generic fallback`

这一步非常关键，说明项目已经不是“纯导航壳”，而是开始有 surface 内容语义。

---

## 7. 设计理念与不要破坏的边界

这是接手开发时最重要的部分。

### 7.1 不要把 domain 逻辑塞回 UI

现在 `AppController` 已经接住了大部分编排逻辑。后续新增能力时：

- domain 语义进 `amux-core`
- 平台能力进 `amux-platform`
- 编排和用例流进 `amux-ui::controller`
- 视图层只消费 snapshot / window model

不要在 `gpui_entry.rs` 里直接写大量业务状态操作。

### 7.2 不要把 workspace 当普通路径字符串

必须坚持 `WorkspaceTarget` 抽象：

- `WindowsPath`
- `WslPath`

所有文件打开、cwd、显示路径、运行路径都应通过 path mapper 或 workspace target 传递。

### 7.3 不要把 Agent 和 Terminal 混成一种 surface

虽然当前 agent surface 仍依赖 terminal-like 预览，但数据模型上已经分开了。后续做真实终端/agent 时必须保持分离。

### 7.4 不要过早上浏览器、LSP、插件系统

当前最缺的是：

- 真实 terminal backend
- 真实 surface 内容能力
- 真实 palette 交互

这些没完成前，不要把精力投入浏览器等次要能力。

### 7.5 不要直接推翻当前 render 分层

现在已经形成了：

- `AppSnapshot`
- `GpuiWindowModel`
- `TextRenderer`
- `GpuiRenderer`
- `apps/desktop` 入口装配

后续接真实 GPUI 组件时，应在这个边界内演进，而不是把 snapshot 模型绕过。

---

## 8. 已踩过的坑

### 8.1 `limux` 只能参考交互，不适合直接复用技术实现

最初产品参考了 `third_party/limux`，但它更偏 Linux GTK 宿主路线，不适合直接做 Windows-first 桌面壳。

结论：

- 可以参考它的 workspace/pane/tab/session 思路
- 不要试图直接沿用它的宿主实现

### 8.2 `gpui-component` 不适合当前就硬接

早期已经判断过，当前环境和依赖条件下直接接 `gpui-component` 容易阻塞。

所以现在的策略是：

- 先立 `gpui` feature 和真实窗口边界
- 先把窗口模型和交互链走通
- 再考虑逐步接真实组件

这不是绕路，而是为了避免一开始就卡死在依赖和 API 兼容上。

### 8.3 如果没有 in-memory backend，UI 开发会被平台实现卡死

现在 mock terminal/fs backend 的存在非常关键。它让 UI、controller、workspace、agent 可以并行推进。

不要轻易删掉这些 mock backend。真实平台实现接入后，它们仍然适合作为测试与 demo 驱动。

### 8.4 `gpui_entry.rs` 很容易膨胀

这个问题已经出现过一次，所以才把 active surface 视图拆到了：

- [apps/desktop/src/gpui_surface_views.rs](/mnt/d/repository/arden/Ai/ide/amux/apps/desktop/src/gpui_surface_views.rs)
- [apps/desktop/src/gpui_status_bar.rs](/mnt/d/repository/arden/Ai/ide/amux/apps/desktop/src/gpui_status_bar.rs)
- [apps/desktop/src/gpui_command_bar.rs](/mnt/d/repository/arden/Ai/ide/amux/apps/desktop/src/gpui_command_bar.rs)
- [apps/desktop/src/gpui_command_palette.rs](/mnt/d/repository/arden/Ai/ide/amux/apps/desktop/src/gpui_command_palette.rs)

后续如果再加视图模块，优先继续拆，不要再把入口文件堆回巨型模块。

### 8.5 当前 gpui 视图里不要过度追求复杂样式 API

之前做内容区时，某些预期的滚动 API 并不稳定或不适用，所以当前实现偏保守，更多用定高、简单块布局、只读内容。

结论：

- 先做正确的结构和交互
- 不要为了一个局部视觉效果把整体编译稳定性打坏

---

## 9. 当前验证状态

本次交接前已经重新确认以下命令可通过：

```bash
cargo test -p amux-ui
cargo check -p amux-desktop --features gpui
```

本次验证结果：

- `amux-ui`：4 个测试通过
- `amux-desktop --features gpui`：编译通过

如果你接手后先做代码修改，建议优先至少复跑这两个命令。

推荐补充验证命令：

```bash
cargo check
cargo run -p amux-desktop
cargo run -p amux-desktop --features gpui
```

---

## 10. 当前代码中的关键入口

如果你只想快速理解系统主线，建议先按下面顺序看代码。

### 10.1 先看 domain 操作

1. [crates/amux-core/src/command.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-core/src/command.rs)
2. [crates/amux-core/src/session/ops.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-core/src/session/ops.rs)
3. [crates/amux-core/src/workspace/ops.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-core/src/workspace/ops.rs)
4. [crates/amux-core/src/layout/ops.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-core/src/layout/ops.rs)

### 10.2 再看 UI 用例编排

1. [crates/amux-ui/src/controller.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-ui/src/controller.rs)
2. [crates/amux-ui/src/state.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-ui/src/state.rs)
3. [crates/amux-ui/src/root.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-ui/src/root.rs)

### 10.3 再看渲染抽象

1. [crates/amux-ui/src/render/gpui.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-ui/src/render/gpui.rs)
2. [apps/desktop/src/gpui_entry.rs](/mnt/d/repository/arden/Ai/ide/amux/apps/desktop/src/gpui_entry.rs)
3. [apps/desktop/src/gpui_surface_views.rs](/mnt/d/repository/arden/Ai/ide/amux/apps/desktop/src/gpui_surface_views.rs)

### 10.4 再看平台 mock 和服务

1. [crates/amux-platform/src/terminal.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-platform/src/terminal.rs)
2. [crates/amux-platform/src/fs.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-platform/src/fs.rs)
3. [crates/amux-workspace/src/manager.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-workspace/src/manager.rs)
4. [crates/amux-agent/src/launch.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-agent/src/launch.rs)

---

## 11. 下一步开发建议

后续开发不要平均用力，建议按优先级推进。

### Phase A：把 command palette 做成真正可用的统一入口

当前 palette 只是：

- 弹层
- 静态命令列表
- 点击执行

建议下一步优先做：

1. palette query 状态
2. 输入框
3. 按 query 过滤命令
4. 高频命令 / 分组命令展示
5. 键盘导航

原因：

- 它能最快把当前“命令驱动”模型变成真实交互入口
- 不会破坏 domain 边界
- 能明显提升原型可用性

### Phase B：把 editor / preview 从只读摘要推进到更真实内容视图

当前 editor/preview 是只读内容区。

优先建议：

1. editor 真正支持选中文本或更接近代码窗口的布局
2. preview 对 markdown 做更稳定的内容分块
3. 将 surface 视图继续拆成更小模块

### Phase C：开始真实 terminal backend 替换

这是技术难点，也是产品成败关键。

建议顺序：

1. 先在 `amux-platform` 内实现真实 terminal backend，不要先碰 UI
2. 先完成最小 PowerShell / WSL 启动、写入、resize、关闭
3. 再把 `InMemoryTerminalBackend` 替换为真实 backend 注入到 `AppController`
4. 最后再做 terminal surface 真视图

注意：这一步风险最大，不要和大量 UI 变更并行推进。

### Phase D：真实文件系统与 workspace 打通

当前 workspace 服务虽然接口完整，但仍主要依赖 in-memory fs 进行 demo。

下一步可以逐步替换为：

1. 真实文件扫描
2. 真实读取
3. 真实保存
4. workspace 最近文件与持久化增强

---

## 12. 建议的近期任务拆分

如果你准备立刻开工，推荐按这个顺序拿任务。

### 任务 1

完善 command palette：

- 增加 query 状态
- 增加输入框
- 支持命令过滤
- 保持当前 command router 不变

### 任务 2

继续清理 GPUI 视图模块边界：

- 将 workspace/agent/file/open files/tabs/panes 这些面板也逐步从 `gpui_entry.rs` 抽离
- 让 `gpui_entry.rs` 更像“窗口装配器”

### 任务 3

补真实文件系统 backend 的接入路径：

- 先保留 mock
- 增加 real fs backend
- 在 controller/bootstrap 中切换注入

### 任务 4

做真实 terminal backend spike：

- 先只验证 Windows local shell
- 再验证 WSL
- 不要一开始就做多平台统一完善实现

---

## 13. 开发注意事项

### 13.1 保留 mock backend

真实 backend 进来后，mock backend 也不要删。

它们有三个价值：

- 测试
- demo
- UI 不依赖平台环境的开发

### 13.2 优先保持 feature gate 清晰

当前 `gpui` 路径已经通过 feature 单独收口，这很好。

后续新增真实能力时也尽量保持 feature 边界，不要让默认路径被桌面依赖污染。

### 13.3 修改 UI 前先看 snapshot 是否足够

如果一个 UI 新需求需要复杂数据，优先先补：

- `AppSnapshot`
- `GpuiWindowModel`

而不是在视图层直接向 controller 拉业务数据。

### 13.4 改 command 行为时先检查幂等逻辑

当前已经做了：

- 重复打开同一 agent tab 会复用
- 重复打开同一文件 editor tab 会复用

新增命令时，先考虑是否也需要复用或激活已有 tab。

### 13.5 session schema 变更要谨慎

现在 session 已真实落盘。如果你修改核心结构：

- 先检查 serde 兼容性
- 必要时在 `amux-core/src/session/migrate.rs` 增加迁移逻辑

不要随意破坏旧 session 的加载。

---

## 14. 推荐的接手流程

新开发者建议按下面步骤接手：

1. 先阅读：
   - [plans/amux-technical-design.md](/mnt/d/repository/arden/Ai/ide/amux/plans/amux-technical-design.md)
   - 本文档
2. 运行验证命令：
   - `cargo test -p amux-ui`
   - `cargo check -p amux-desktop --features gpui`
3. 按第 10 节顺序阅读关键入口代码
4. 从第 12 节的任务 1 开始做，而不是直接挑战真实终端

如果你想尽快做出“看得见的进展”，优先做 command palette。
如果你想尽快接近真实产品能力，优先做 terminal backend spike。

---

## 15. 当前结论

这个项目最难的阶段已经过去一半：

- 产品方向已经收敛
- 技术架构已经立住
- 可运行原型已经存在

接下来真正要做的是：

`把正确的抽象，逐步替换成真实能力。`

不要推翻当前设计，也不要被“已经有 UI 了”误导。最稳的推进方式是：

1. 保持 domain 边界稳定
2. 保持 controller 编排中心稳定
3. 优先把 command、surface、terminal 三条主线做实

只要沿着这个方向推进，后面无论接 `gpui` 真组件、ConPTY、WSL，还是更完整的 editor/preview，都不会发生大规模返工。
