# AMUX 跨平台实施任务表

## 1. 文档目标

这份文档是对 [cross-platform-architecture-plan.md](/Users/arden/data/repository/ai/arden/amux/plans/cross-platform-architecture-plan.md) 的执行拆分。

前一份文档回答的是：

- 为什么要做跨平台架构改造
- 应该如何分层
- 哪些能力应共享，哪些必须平台隔离
- 如何保护 Windows 现有稳定链路

这一份文档回答的是：

- 现在应该先做哪些任务
- 每个任务改动范围是什么
- 每个任务做完如何验收
- 哪些文件这轮可以动，哪些先不要碰
- Windows 回归要怎么守

目标是让后续开发可以直接按任务推进，而不是重新拆阶段。

---

## 2. 总体执行策略

这次跨平台改造不按“先把一切都抽象完”推进，而按下面的顺序做：

1. 先建立平台抽象骨架
2. 再把现有 Windows 能力接到骨架上
3. 再重构 core 模型承载三平台
4. 再接通 macOS
5. 最后接通 Linux

整个过程中遵守 4 条硬规则：

1. 不为抽象而重写 Windows 稳定实现
2. 不在 UI 层继续扩散平台条件分支
3. 每一阶段都必须可编译、可回归
4. 所有平台新增能力都必须有 capability gate

---

## 3. 当前阶段划分

建议将近期工作拆成 8 个核心任务。

按优先级和依赖顺序如下：

1. `CP1` 平台服务抽象落地
2. `CP2` Windows 现有实现接入平台抽象
3. `CP3` UI/controller 改为依赖注入平台服务
4. `CP4` Core 模型去 Windows-first 化
5. `CP5` Session schema 与 migration 重构
6. `CP6` 正式启动流替换 demo bootstrap
7. `CP7` macOS 最小可用链路接通
8. `CP8` Linux 最小可用链路接通

其中：

- `CP1` 到 `CP3` 是 Phase 1
- `CP4` 到 `CP6` 是 Phase 2
- `CP7` 是 Phase 3
- `CP8` 是 Phase 4

---

## 4. Windows 回归保护总表

在所有任务中，下列能力视为回归保护线。

### 4.1 必须保持稳定的功能

- Windows 本地 workspace 打开
- WSL workspace 打开
- WSL path mapping
- terminal 启动与输入输出
- AI tool 检测与启动
- startup commands
- pane/tab/split 行为
- session restore
- smart paste 图片路径注入

### 4.2 高风险文件

以下文件允许改，但每次改动都必须做 Windows 回归验证：

- [target.rs](/Users/arden/data/repository/ai/arden/amux/crates/amux-core/src/workspace/target.rs)
- [terminal.rs](/Users/arden/data/repository/ai/arden/amux/crates/amux-core/src/surface/terminal.rs)
- [path_mapper.rs](/Users/arden/data/repository/ai/arden/amux/crates/amux-platform/src/path_mapper.rs)
- [backend.rs](/Users/arden/data/repository/ai/arden/amux/crates/amux-platform/src/terminal/backend.rs)
- [controller.rs](/Users/arden/data/repository/ai/arden/amux/crates/amux-ui/src/controller.rs)
- [gpui_entry.rs](/Users/arden/data/repository/ai/arden/amux/apps/desktop/src/gpui_entry.rs)
- [gpui_vibe_tools.rs](/Users/arden/data/repository/ai/arden/amux/apps/desktop/src/gpui_vibe_tools.rs)
- [gpui_workspace_persistence.rs](/Users/arden/data/repository/ai/arden/amux/apps/desktop/src/gpui_workspace_persistence.rs)

### 4.3 每轮最低回归命令

```bash
cargo test -q
cargo check -q -p amux-desktop --features gpui
```

### 4.4 推荐手工 smoke

1. 打开 Windows 本地 workspace
2. 打开 WSL workspace
3. Split pane / new tab
4. 启动 Codex / Claude
5. 运行 startup 文件
6. 关闭并恢复 session
7. Smart paste 图片路径

---

## 5. CP1 平台服务抽象落地

### 5.1 目标

在 `amux-platform` 中建立正式的平台抽象骨架，但不改变现有行为。

### 5.2 需要完成的内容

- 新增 `PlatformId`
- 新增 `PlatformCapabilities`
- 新增 `HostPlatform` trait
- 新增：
  - `TerminalService`
  - `FsService`
  - `PathService`
  - `ClipboardService`
  - `BrowserService`
  - `MetricsService`
  - `WorkspaceDialogService`
- 在 `amux-platform` 中建立目录骨架：
  - `services.rs`
  - `capabilities.rs`
  - `windows/`
  - `macos/`
  - `linux/`
  - `common/`

### 5.3 范围边界

这一任务里不要：

- 改 `amux-core` 的 `WorkspaceTarget`
- 改现有 Windows 行为
- 改 UI 调用链
- 改 session schema

先把抽象搭出来。

### 5.4 建议改动文件

- [lib.rs](/Users/arden/data/repository/ai/arden/amux/crates/amux-platform/src/lib.rs)
- `crates/amux-platform/src/services.rs`
- `crates/amux-platform/src/capabilities.rs`
- `crates/amux-platform/src/windows/mod.rs`

### 5.5 验收标准

- `amux-platform` 能编译
- 新 trait 边界明确
- 旧代码尚未切换也不影响现有行为

### 5.6 风险

- trait 边界定得太细，后面会导致接口膨胀
- trait 边界定得太粗，UI 仍会拿到平台细节

### 5.7 Windows 回归要求

- 本任务不应改变运行行为
- 仅允许新增抽象与 re-export

---

## 6. CP2 Windows 现有实现接入平台抽象

### 6.1 目标

把当前 Windows/WSL 稳定实现接入 `HostPlatform` 骨架，但保持现有功能语义不变。

### 6.2 需要完成的内容

- 新增 `WindowsPlatform`
- 将现有：
  - `RealTerminalBackend`
  - `RealFsBackend`
  - `DefaultPathMapper`
  - metrics
  - clipboard/browser 能力
  封装进 `WindowsPlatform`
- 把现有 WSL 能力保留在 Windows 适配器内部

### 6.3 范围边界

这一任务里不要：

- 重写 WSL path 逻辑
- 改 tool detection 行为
- 改 startup commands 行为
- 改 shell 默认值

只做“包一层，不改行为”。

### 6.4 建议改动文件

- `crates/amux-platform/src/windows/platform.rs`
- [backend.rs](/Users/arden/data/repository/ai/arden/amux/crates/amux-platform/src/terminal/backend.rs)
- [path_mapper.rs](/Users/arden/data/repository/ai/arden/amux/crates/amux-platform/src/path_mapper.rs)
- [fs.rs](/Users/arden/data/repository/ai/arden/amux/crates/amux-platform/src/fs.rs)

### 6.5 验收标准

- `WindowsPlatform` 可构造
- 能通过 trait 拿到 terminal/fs/path 等服务
- 原有 Windows 行为不变

### 6.6 Windows 回归要求

必须手工验证：

- Windows 本地 workspace
- WSL workspace
- agent launch

---

## 7. CP3 UI/controller 改为依赖注入平台服务

### 7.1 目标

让 `amux-ui` 不再内部直接 new backend，而是接收 `HostPlatform` 注入。

### 7.2 需要完成的内容

- 修改 `AppController`
- 修改 `DesktopApp`
- 调整 `apps/desktop` 的装配逻辑
- 让 controller 通过 `HostPlatform` 获取 terminal/fs/path 等能力

### 7.3 范围边界

这一任务里不要：

- 改业务状态模型
- 改命令语义
- 改 session schema
- 改 UI 行为

只改依赖方向。

### 7.4 建议改动文件

- [controller.rs](/Users/arden/data/repository/ai/arden/amux/crates/amux-ui/src/controller.rs)
- [root.rs](/Users/arden/data/repository/ai/arden/amux/crates/amux-ui/src/root.rs)
- [main.rs](/Users/arden/data/repository/ai/arden/amux/apps/desktop/src/main.rs)

### 7.5 验收标准

- `DesktopApp::new(...)` 或新的 builder 能接收平台服务
- controller 不再自行决定平台 backend 实现
- UI 行为无明显变化

### 7.6 Windows 回归要求

- `--real` 模式仍可工作
- session restore、agent launch、open file 不应回归

---

## 8. CP4 Core 模型去 Windows-first 化

### 8.1 目标

把当前 `WorkspaceTarget` 和 `ShellKind` 改造成真正可承载三平台的模型。

### 8.2 需要完成的内容

- 重构 `WorkspaceTarget`
- 扩展 `ShellKind`
- 为 WSL 保留兼容承载方式
- 更新相关 command、session、path 映射入口

### 8.3 范围边界

这一任务里不要先追求最完美的抽象。

优先目标是：

- 支持 `LocalPath`
- 保留 Windows/WSL 兼容
- 给 macOS/Linux 留足空间

### 8.4 建议改动文件

- [target.rs](/Users/arden/data/repository/ai/arden/amux/crates/amux-core/src/workspace/target.rs)
- [terminal.rs](/Users/arden/data/repository/ai/arden/amux/crates/amux-core/src/surface/terminal.rs)
- [session/ops.rs](/Users/arden/data/repository/ai/arden/amux/crates/amux-core/src/session/ops.rs)
- [path_mapper.rs](/Users/arden/data/repository/ai/arden/amux/crates/amux-platform/src/path_mapper.rs)

### 8.5 验收标准

- core 模型可表示 Windows/macOS/Linux 本地 workspace
- 现有 WSL 能力仍可承载
- 现有代码可编译迁移

### 8.6 风险

- 旧 session 兼容性
- controller 和 path mapper 上的连锁修改

### 8.7 Windows 回归要求

- 旧 Windows/WSL workspace 仍可打开
- 旧 session 必须能迁移加载

---

## 9. CP5 Session schema 与 migration 重构

### 9.1 目标

让 session 能稳定承载新模型，并尽量朝“单一业务真相”方向演进。

### 9.2 需要完成的内容

- 为新 target/shell schema 写 migration
- 兼容旧 `session.json`
- 评估 `layouts.json` 的过渡策略
- 明确 session 与 runtime layout 的主次关系

### 9.3 范围边界

这一任务里可以先不彻底删掉 `layouts.json`。

短期允许：

- `session.json` 是业务真相
- `layouts.json` 是运行态缓存

但要把边界写清楚。

### 9.4 建议改动文件

- [store.rs](/Users/arden/data/repository/ai/arden/amux/crates/amux-session/src/store.rs)
- `crates/amux-core/src/session/migrate.rs`
- [gpui_workspace_persistence.rs](/Users/arden/data/repository/ai/arden/amux/apps/desktop/src/gpui_workspace_persistence.rs)

### 9.5 验收标准

- 旧 session 自动迁移成功
- 新 schema 可序列化 / 反序列化
- session 恢复流程可工作

### 9.6 Windows 回归要求

- 现有用户 session 不应失效
- 启动后 workspace/layout 恢复不应明显退化

---

## 10. CP6 正式启动流替换 demo bootstrap

### 10.1 目标

去掉默认 demo 启动流，建立正式首启 / 恢复 / 空状态启动逻辑。

### 10.2 需要完成的内容

- 移除默认 `bootstrap_demo()` 启动路径
- 区分：
  - 首次启动
  - 有 session 恢复
  - 命令行指定 workspace
- 引入 welcome / empty state 流程

### 10.3 范围边界

这一任务里不要顺便重做 command palette 或工作区侧栏。

只做启动行为产品化。

### 10.4 建议改动文件

- [main.rs](/Users/arden/data/repository/ai/arden/amux/apps/desktop/src/main.rs)
- [controller.rs](/Users/arden/data/repository/ai/arden/amux/crates/amux-ui/src/controller.rs)
- [root.rs](/Users/arden/data/repository/ai/arden/amux/crates/amux-ui/src/root.rs)

### 10.5 验收标准

- 首启不再自动拉 demo
- 有 session 时能恢复
- 无 session 时进入空状态

### 10.6 Windows 回归要求

- Windows 首启流程可用
- session 恢复和 startup 不应互相踩踏

---

## 11. CP7 macOS 最小可用链路接通

### 11.1 目标

让 macOS 达到最小可用：

- 打开本地 workspace
- 启动 shell
- split/tab
- session restore
- 文件浏览/预览基本可用

### 11.2 需要完成的内容

- 新增 `MacosPlatform`
- 本地路径 workspace 支持
- system shell / PTY 支持
- path mapping
- folder picker / clipboard 基础接通

### 11.3 范围边界

这一任务先不追求：

- 内置 browser 完整体验
- 高级编辑器能力
- 所有 AI tool detection 细节完全一致

先把主链路跑通。

### 11.4 建议改动文件

- `crates/amux-platform/src/macos/*`
- [path_mapper.rs](/Users/arden/data/repository/ai/arden/amux/crates/amux-platform/src/path_mapper.rs)
- [backend.rs](/Users/arden/data/repository/ai/arden/amux/crates/amux-platform/src/terminal/backend.rs)
- [main.rs](/Users/arden/data/repository/ai/arden/amux/apps/desktop/src/main.rs)

### 11.5 验收标准

- macOS 可以打开本地项目
- 可以打开终端并执行命令
- 可以 split / new tab
- 可以保存和恢复 session

### 11.6 Windows 回归要求

- 不改 Windows 适配器语义
- 所有新增行为必须通过 capability gate 暴露给 UI

---

## 12. CP8 Linux 最小可用链路接通

### 12.1 目标

让 Linux 具备与 macOS 同级别的最小可用能力。

### 12.2 需要完成的内容

- 新增 `LinuxPlatform`
- Unix shell / PTY 支持
- 本地 workspace
- path mapping
- restore / clipboard / capability gate

### 12.3 范围边界

优先支持主流桌面环境，不在这一阶段追求全发行版完美兼容。

### 12.4 建议改动文件

- `crates/amux-platform/src/linux/*`
- `crates/amux-platform/src/unix/*`

### 12.5 验收标准

- Linux 能稳定打开 workspace
- shell / split / tab / restore 可用
- 不支持的能力可降级，不 silent fail

---

## 13. 不建议这轮先做的事项

以下事项先不要穿插到本轮跨平台改造里：

- 大规模 UI 重设计
- 浏览器完整平台统一
- 高级编辑器重做
- 插件系统
- Git GUI
- 命令面板大重构
- 通知系统重写

这些事情很容易把“平台抽象改造”演化成“全项目重构”，风险过高。

---

## 14. 推荐执行顺序

建议按下面的短迭代推进：

1. `CP1`
2. `CP2`
3. `CP3`
4. `CP4`
5. `CP5`
6. `CP6`
7. `CP7`
8. `CP8`

每完成一个任务都应：

1. 跑编译与测试
2. 做最小回归
3. 再进入下一个任务

---

## 15. 结论

这份任务表的核心目的不是把跨平台工作拆得“看起来很多”，而是把风险拆开。

真正应该避免的是两种做法：

1. 直接大重构，把 Windows 稳定线一起带进手术台
2. 到处补平台分支，最后继续让 UI、core、platform 三层互相污染

正确做法是：

- 先搭平台服务抽象
- 再让 Windows 先接上
- 再改 core 模型
- 再逐个平台接入
- 每一步都守住 Windows 回归线

如果后续严格按这个任务表推进，AMUX 才有机会在不牺牲现有 Windows 稳定性的情况下，真正演进成可维护的三平台产品。
