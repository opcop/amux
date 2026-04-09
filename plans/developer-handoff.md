# AMUX 开发交接文档

## 1. 文档目的

这份文档给下一位接手 `amux` 的开发者使用，目标是把下面几件事一次讲清楚：

- 当前应该在哪个分支继续开发
- 这条跨平台改造线已经做到了什么
- 哪些模块是真正在线上的实现，哪些只是历史遗留或占位
- 后续开发该从哪里继续，哪些地方不要乱动
- 最低验证命令和回归红线是什么

如果你是第一次接手这条线，建议顺序如下：

1. 先看本文
2. 再看 [cross-platform-architecture-plan.md](/Users/arden/data/repository/ai/arden/amux/plans/cross-platform-architecture-plan.md)
3. 再看 [cross-platform-implementation-tasks.md](/Users/arden/data/repository/ai/arden/amux/plans/cross-platform-implementation-tasks.md)
4. 最后再开始改代码

---

## 2. 当前工作分支

当前开发分支：

`cross-platform-foundation`

这不是一个“另起炉灶”的实验分支，而是在保护现有 Windows 能力的前提下，为 `Windows / macOS / Linux` 三平台打通基础模型和平台层的正式改造分支。

当前工作区还有一个**已有的未提交改动**：

- `scripts/build.sh`

这不是本轮跨平台改造的核心内容。除非你明确知道自己为什么要改它，否则不要顺手处理、回滚、整理或覆盖它。

---

## 3. 当前整体判断

### 3.1 这条线已经完成了什么

跨平台改造已经不是“只写了文档”，而是完成了第一轮可运行骨架落地 + 一轮收敛/产品化：

**Phase 1 骨架（CP1–CP11）**

- 平台抽象层已经建立
- Windows 现有能力已经适配到平台抽象
- UI/controller 已支持 `HostPlatform` 注入
- `WorkspaceTarget` 已从 Windows-first 改成支持 `LocalPath`
- `ShellKind` 已扩成可承载三平台
- `current_host_platform()` 已可按宿主自动注入 Windows/macOS/Linux
- `PlatformCapabilities` 已贯穿到 command/help/palette/runtime gating
- `Ctrl+Shift+N` 已接上真正的 workspace 打开链
- folder picker 已在平台层落地
- 桌面层多个入口已统一走同一条“打开 workspace”链
- 一批未接线的旧桌面模块已经从编译路径移出

**Phase A 收敛 + Phase B/CP6 产品化（详见 §4.8–§4.12）**

- 核心活动路径 warning 全部清零（amux-platform / amux-ui / amux-desktop）
- `terminal/` 子模块从 5067 行收敛到 2638 行（少 48%），三套并行的旧 terminal 抽象（emulator.rs / view.rs / session.rs）明确归档
- view.rs 中的活代码 `keys` 模块被抢救到独立 `terminal/keys.rs`
- macOS/Linux 第一次有真实剪贴板（`arboard` 三平台共用）
- 三平台 folder picker 升级到原生（`rfd`，macOS NSOpenPanel / Win IFileDialog / Linux xdg-portal），不再 fork 子进程
- `ClipboardImage` 重设计为 raw RGBA8，编码决策回归到 desktop shell
- demo bootstrap 删除，`DesktopApp::startup(StartupOptions)` 是新的产品入口；`main.rs` 支持 `--workspace <path>` / `-w` / 位置参数
- 启动 banner 按真实 `StartupMode = OpenedWorkspace | Restored | Empty` 打印
- scrollback 选区修复：`gpui_entry.rs` 的 selection 创建点接通 `display_offset`，用户滚动到历史区域拖选 → 复制能拿到正确内容
- amux-ui 14/14 + amux-platform 27/27 测试全绿（pre-existing 测试 debt 也清掉了）

### 3.2 还没有完成什么

跨平台基础设施和产品化收口已经走到一个稳定基线，剩下的主要分两类：

**架构债（暂未动，需独立迭代专门处理）**

- `WorkspaceTarget` 仍同时存在 `LocalPath` 和 `WindowsPath` 两个 variant —— `WindowsPath` 是 CP4 之前留下的过渡品，应该折叠进 `LocalPath`，让 Windows 路径只是 LocalPath 的合法写法。这是 §10.2 高风险区，需要写 session migration。
- `DefaultPathMapper` 三平台共用，但 WSL 路径转换逻辑可能仍藏在里面。应该把 `PathService` 真正按平台特化，让 Windows 实现独享 WSL 逻辑。前置依赖 WorkspaceTarget 折叠。

**剩余的产品化收口**

- macOS/Linux `BrowserService` 仍是 Noop —— 三平台 capability 已经按 `browser_tabs: false` 诚实地降级，UI 已隐藏 browser 入口。要真正在 macOS/Linux 上做 browser host 需要接 `wry`/`webkit2gtk`，工作量较大，建议作为独立任务。
- `ClipboardService` 仍只有 `read_image`，没有 `write_image`。当前 desktop 不需要写图像，将来如果要做 screenshot 反向写回剪贴板再加。
- `seed_demo_workspace_files` 仍在 controller.rs 里被 `restore_session_if_present` 调用，production `--real` 模式下短路返回。建议未来从 controller.rs 抽进 test fixtures，避免读者疑惑。
- 启动流仍然没有"welcome screen" UI ——目前空状态依赖 GPUI 端的"empty workspace"渲染 + Ctrl+Shift+N folder picker 引导。如果产品上要做真正的 welcome 屏，是独立的 UI 工作。

### 3.3 当前最重要的约束

这条线的首要原则不是“抽象更优雅”，而是：

`不破坏 Windows 现有稳定能力`

只要你准备动下面这些链路，就必须把 Windows 当成回归基线：

- 本地 workspace 打开
- WSL workspace 打开
- terminal 启动与输入输出
- WSL path mapping
- AI tool 启动
- startup commands
- session restore

---

## 4. 已完成阶段总览

为了方便继续推进，这里按已经完成的阶段总结，而不是按文件罗列。

### 4.1 CP1: 平台抽象骨架

已完成：

- `PlatformId`
- `PlatformCapabilities`
- `HostPlatform`
- `TerminalService`
- `FsService`
- `PathService`
- `ClipboardService`
- `BrowserService`
- `MetricsService`
- `WorkspaceDialogService`

关键文件：

- [capabilities.rs](/Users/arden/data/repository/ai/arden/amux/crates/amux-platform/src/capabilities.rs)
- [services.rs](/Users/arden/data/repository/ai/arden/amux/crates/amux-platform/src/services.rs)

### 4.2 CP2: Windows 接入平台抽象

已完成：

- `WindowsPlatform`
- 现有 terminal/fs/path/metrics 的平台适配
- browser/clipboard/dialog 的注入位

关键文件：

- [windows/platform.rs](/Users/arden/data/repository/ai/arden/amux/crates/amux-platform/src/windows/platform.rs)

### 4.3 CP3: UI/controller 注入平台服务

已完成：

- `AppController::with_platform(...)`
- `DesktopApp::with_platform(...)`
- `main --real` 走平台注入

关键文件：

- [controller.rs](/Users/arden/data/repository/ai/arden/amux/crates/amux-ui/src/controller.rs)
- [root.rs](/Users/arden/data/repository/ai/arden/amux/crates/amux-ui/src/root.rs)
- [main.rs](/Users/arden/data/repository/ai/arden/amux/apps/desktop/src/main.rs)

### 4.4 CP4: Core 模型跨平台化

已完成：

- `WorkspaceTarget::LocalPath`
- `ShellKind::SystemDefault/Bash/Zsh/Fish/Custom`
- 本地 workspace 从 Windows 特化中抽离
- agent/provider/path/session/terminal 相关链路同步承载新模型

关键文件：

- [target.rs](/Users/arden/data/repository/ai/arden/amux/crates/amux-core/src/workspace/target.rs)
- [terminal.rs](/Users/arden/data/repository/ai/arden/amux/crates/amux-core/src/surface/terminal.rs)

### 4.5 CP5: 平台工厂与兼容性接线

已完成：

- `current_host_platform()`
- `MacosPlatform`
- `LinuxPlatform`
- `amux-session` 旧格式兼容读取测试

关键文件：

- [lib.rs](/Users/arden/data/repository/ai/arden/amux/crates/amux-platform/src/lib.rs)
- [macos/mod.rs](/Users/arden/data/repository/ai/arden/amux/crates/amux-platform/src/macos/mod.rs)
- [linux/mod.rs](/Users/arden/data/repository/ai/arden/amux/crates/amux-platform/src/linux/mod.rs)
- [codec.rs](/Users/arden/data/repository/ai/arden/amux/crates/amux-session/src/codec.rs)

### 4.6 CP6-CP11: capability 落地与 workspace 打开链收口

已完成：

- `PlatformCapabilities` 已进入 `AppSnapshot` / `GpuiWindowModel`
- WSL/browser 已按 capability 做 runtime gating
- command help / command palette 已按 capability 裁剪
- agent picker / context menu / browser 快捷键已按 capability 裁剪
- `Ctrl+Shift+N` 已从坏命令改成真实 workspace 打开链
- 三平台 folder picker 已在 platform 层接通
- workspace 入口文案已统一成 `Open Workspace`
- sidebar / context menu / 快捷键 已统一复用 `prompt_open_local_workspace()`

关键文件：

- [commands.rs](/Users/arden/data/repository/ai/arden/amux/crates/amux-ui/src/commands.rs)
- [state.rs](/Users/arden/data/repository/ai/arden/amux/crates/amux-ui/src/state.rs)
- [render/gpui.rs](/Users/arden/data/repository/ai/arden/amux/crates/amux-ui/src/render/gpui.rs)
- [common/dialogs.rs](/Users/arden/data/repository/ai/arden/amux/crates/amux-platform/src/common/dialogs.rs)
- [gpui_entry.rs](/Users/arden/data/repository/ai/arden/amux/apps/desktop/src/gpui_entry.rs)
- [gpui_input_handler.rs](/Users/arden/data/repository/ai/arden/amux/apps/desktop/src/gpui_input_handler.rs)

### 4.7 CP12-CP16: 桌面层维护面清理

已完成：

- `gpui_workspace_sidebar.rs` 已瘦身，只保留共享状态和数据模型
- 一批完全未接线的旧 UI 模块已经移出编译路径：
  - `gpui_command_bar.rs`
  - `gpui_command_palette.rs`
  - `gpui_components.rs`
  - `gpui_keyboard_shortcuts.rs`
  - `gpui_surface_views.rs`
- desktop 侧一批低风险 warning 已清掉

注意：

这些文件大多**仍在仓库中**，但已经不参与当前桌面构建。不要把“文件还在”误解成“运行时还在用”。

### 4.8 Phase A 收敛：terminal 子模块、warning、归档

已完成：

- 核心活动路径 warning 全部清零（amux-platform / amux-ui / amux-desktop）
- `crates/amux-platform/src/terminal/` 子模块从 5067 行收敛到 2638 行（少 48%），三套并行的旧 terminal 抽象明确归档：
  - `terminal/emulator.rs`（1697 行，自研 ANSI 解析器，由 alacritty_view::AlacrittyTerminal 取代）
  - `terminal/view.rs`（454 行，旧 TerminalView 包装）
  - `terminal/session.rs`（379 行，旧 TerminalSessionManager + keyboard_to_pty）
- 三个文件都通过注释 `pub mod` 移出编译路径，保留磁盘以备查阅
- view.rs 中嵌着的 `pub mod keys` 是仍在使用的活代码（keyboard→PTY 编码器），在收敛过程中**抽到独立文件** `crates/amux-platform/src/terminal/keys.rs`，调用方路径 `amux_platform::terminal::keys` 不变
- handoff §6.1 增补了 `gpui_terminal_component.rs`（之前漏列的第 6 个未编译桌面旧模块）和 `amux-platform` 内部的死代码归档段落

### 4.9 Phase B：macOS/Linux 真实剪贴板与原生 folder picker

已完成：

- `crates/amux-platform` 引入两个标准 crate：
  - `arboard = "3.6"` —— 跨平台剪贴板（文本 + 图像）
  - `rfd = "0.17"` —— 跨平台原生 folder picker，Linux 默认走 `xdg-portal + wayland`，无 GTK3 C 编译依赖
- 新增 `crates/amux-platform/src/common/clipboard.rs::ArboardClipboardService`，三平台共用，懒打开 native handle，对 `ContentNotAvailable` 静默回退
- 重写 `crates/amux-platform/src/common/dialogs.rs::RfdWorkspaceDialogService` 替换之前 powershell.exe / osascript / zenity 三套子进程方案：
  - macOS folder picker 启动延迟从 ~200ms 降到几乎瞬时（原生 NSOpenPanel）
  - Windows folder picker 也升级到原生 IFileDialog
- `services::ClipboardImage` 重新设计：从"假定 PNG/JPEG 编码后字节"改成 raw RGBA8 (`width + height + rgba`)，避免在平台层引入 image / png 编码器；编码决策交给 desktop shell
- 三平台 `PlatformCapabilities::image_clipboard` 全部从 `false` 升到 `true`
- 新增 macos platform smoke test：构造 + capability 默认值

### 4.10 Phase CP6：删 demo bootstrap，正式 startup 流

已完成：

- `UiAction::OpenWindowsWorkspace` → `OpenLocalWorkspace`（消除 Windows-first 命名遗留，内部行为不变）
- `controller::bootstrap_demo` 拆解：
  - `restore_session_if_present(&mut UiState) -> bool`：产品级，零 opinion
  - `open_local_workspace(&mut UiState, PathBuf)`：持久化封装
  - `seed_demo_state(&mut UiState)`：test/dev 专用，`#[cfg(test)]` 锁死，production 编译路径里根本不存在
- `DesktopApp::startup(StartupOptions) -> StartupResult` 是新的产品入口；`StartupMode = OpenedWorkspace { path } | Restored | Empty`
- `apps/desktop/src/main.rs` 改写：
  - 新的 `parse_cli` 支持 `--workspace <path>` / `-w <path>` / 位置参数（自动识别目录）
  - `bootstrap_demo` 调用删除，改为 `app.startup(...)`
  - 启动 banner 按真实 `StartupMode` 打印不同行
- 新增 2 个回归测试守红线：
  - `startup_with_no_session_lands_in_empty_state`：empty 启动不能 auto-open cwd / mock README / launch codex
  - `startup_with_explicit_workspace_opens_it`：`--workspace` 路径必须落到 `OpenedWorkspace` 模式
- GPUI 端在无 active workspace 时**已经能优雅降级**（fallback 到 "default" id + 空 TerminalManager），所以本轮没有引入新 welcome UI

### 4.11 Scrollback 选区修复

已完成：

- `gpui_entry.rs` 两个 selection 创建点（mouse down + mouse move）以前直接用 `Line(row as i32)` 构造 alacritty `Point`，未减 `display_offset`
- 现象：用户滚动到 scrollback 拖选 → 复制出来的不是看到的内容，是 live 区域同坐标位置的内容
- 修复：在 `with_term_mut` 闭包内读 `t.grid().display_offset()`，按 `grid_line = row - display_offset` 转换。这是 `gpui_terminal.rs:451` 已存在的反向转换 `viewport_line = grid_line + display_offset` 的逆运算
- 第三个 selection 调用点（`find next` 高亮，行 918）已经正确，因为它用的是 `RegexSearch` 返回的真正 grid `Point`

### 4.12 杂项

已完成：

- 修复 `palette_selection_wraps_around_filtered_commands` pre-existing 测试失败：硬编码的 wrap 目标改成从实际 `filtered_palette_commands_for("agent", caps)` 动态算
- amux-ui 测试套件 14/14 全绿，amux-platform 27/27 全绿

### 4.13 macOS smoke 阶段：5 个 P0 修复

这一段是把 amux-desktop 第一次在 macOS Tahoe (Darwin 25.4) 上真正跑起来的过程中找到的 5 个 P0 bug 的修复总结。**每一项的 root cause 都不是表面上看到的样子**，所以记得是必要的：

#### 4.13.1 GPUI folder picker 同步调用导致 RefCell 重入崩溃

**症状**：在 sidebar 点 `+ Open Workspace` 或按 Ctrl+Shift+N → 立即 panic：`thread 'main' panicked at gpui/src/app/async_context.rs:65: RefCell already borrowed`

**Root cause**：B 阶段把 osascript / powershell 子进程方案换成 `rfd::FileDialog::pick_folder()` 后，rfd 在 macOS 上是**同步阻塞调用 `NSOpenPanel.runModal()`，跑一个嵌套 NSApp event loop**。这个嵌套 loop 重入 GPUI 的事件分发，命中已经被外层 render listener 借走的 RefCell。

之前 osascript 是 fork 子进程，没有重入；rfd 走原生 NSOpenPanel **更快但反而把 GPUI 拽进重入**。这是用同步原生对话框时最经典的坑。

**修复**：`apps/desktop/src/gpui_entry.rs::prompt_open_local_workspace` 改成接收 `cx: &mut Context<Self>`，用 `rfd::AsyncFileDialog::pick_folder()` + `cx.spawn` 把对话框 future 交给 GPUI executor。listener 立即返回、所有 borrow 释放，然后 dialog 在主 run loop 上跑（不在 listener stack frame 里），结果回来后再 `this.update(cx, ...)` 应用。这个模式跟 `gpui_entry.rs` 里 `open_browser` 的 WebView2 deferral 完全一致。

`rfd = "0.17"` 加为 desktop crate 的直接依赖。3 个调用点（context menu / sidebar 按钮 / Ctrl+Shift+N 快捷键）全部加 `cx` 参数。

#### 4.13.2 macOS 文字完全不渲染（NoopTextSystem 被错误启用）

**症状**：amux 启动后窗口里所有文字都看不见——sidebar 标签、tab 标签、终端 prompt、status bar 全空。但矩形（cell 背景、selection 高亮、active tab 蓝线）都正常。Title bar "AMUX" 因为是 macOS 原生 NSTitlebar 渲染所以正常。

**误诊路径**（记下来，下次能省时间）：
1. 怀疑 Cascadia Code 字体没装 → 改 config 用 Menlo → 没改善
2. 怀疑 GPUI 的 `.SystemUIFont` 默认在 Darwin 25.4 不可用 → 给 root div 加 `font_family + fallbacks` 防御 → 没改善
3. 怀疑 `paint_glyph` 的 `raster_bounds.is_zero()` short-circuit → 加 paint 错误日志 → 没有错误
4. 怀疑 font-kit 的 `glyph_for_char` 在 macOS Tahoe 上 `CTFontGetGlyphsForCharacters(count: 2)` 硬编码导致返回 .notdef → 用 `[patch.crates-io]` 把 font-kit fork 到 `/tmp` 改 count → **patch 编译进 rlib 但不在 final binary**
5. `nm` binary 发现：**只有 `gpui::platform::NoopTextSystem::glyph_for_char` 被链接进来**，没有任何 `MacTextSystem` 或 font-kit 符号

**Root cause**：`gpui_macos/src/platform.rs::MacPlatform::new()` 有这两行：
```rust
#[cfg(feature = "font-kit")]
let text_system = Arc::new(crate::MacTextSystem::new());
#[cfg(not(feature = "font-kit"))]
let text_system = Arc::new(gpui::NoopTextSystem::new());
```

**当 `gpui_macos` 的 `font-kit` feature 没启用时，整个 macOS text system 是 `NoopTextSystem`**——一个测试桩，它的 `typographic_bounds` 对所有字符返回常量 `(54, 0) size (392, 528)`，`glyph_raster_bounds` 返回全 0，于是 `paint_glyph` 因为 `raster_bounds.is_zero()` 直接 silent skip。所有文字 glyph 都不画，但矩形 (`paint_quad`) 不受影响。

这个 feature 的传递路径是：
- `gpui_platform/Cargo.toml`: `font-kit = ["gpui_macos/font-kit"]`
- `gpui/Cargo.toml`: `gpui_platform = { workspace = true, features = ["font-kit"] }`
- `gpui` 自己的 `default = ["font-kit", ...]` 是 implicit feature，**只激活 `zed-font-kit` crate 但不会传递到 `gpui_platform/font-kit`**

amux 的 `apps/desktop/Cargo.toml` 同时直接依赖了 `gpui` 和 `gpui_platform`。`gpui_platform` 的 dep 行没显式启用 `features = ["font-kit"]`，cargo 的 feature unification 在这种"两个分别 dep + 其中一个 dep 的 dep 启用 feature"的拓扑下，没正确把 feature 传过来（具体规则我没完全验证，但症状摆在那里）。

**修复**（一行）：
```toml
- gpui_platform = { git = "...", optional = true }
+ gpui_platform = { git = "...", optional = true, features = ["font-kit"] }
```

**验证**：`nm target/debug/amux-desktop | grep glyph_for_char` 修复前只有 `NoopTextSystem`，修复后能看到 `gpui_macos::text_system::MacTextSystemState::glyph_for_char` 和 `zed_font_kit::loaders::core_text::Font::glyph_for_char`。

**教训**：`#[cfg(not(feature = "..."))]` 默认 fallback 到 noop 的写法非常危险——下游用户漏开 feature 不会有任何编译错误或运行警告，bug 只在最终行为里浮现。今后给三平台 capability 加 cfg 时要避免这种模式。

#### 4.13.3 Workspace folder picker 顺手发现的次要问题

`zed-font-kit/src/loaders/core_text.rs::glyph_for_char` 调用 `CTFontGetGlyphsForCharacters` 时硬编码 `count: 2`。对 BMP 字符（绝大多数），`encode_utf16` 只写 1 个 u16 单元，第 2 个是 NUL。在 macOS Tahoe (Darwin 25.4) 上，Core Text 拿到 NUL 时可能行为变化，存在潜在风险。

**这个 bug 不是 macOS 文字看不见的根因**（根因是 §4.13.2 的 NoopTextSystem），但它本身也是个 latent bug，建议向 zed-font-kit 上游单独提 PR：

```rust
// 把 hardcoded `2` 改成 `character.encode_utf16(&mut src).len()`
let utf16_len = character.encode_utf16(&mut src).len();
self.core_text_font.get_glyphs_for_characters(
    src.as_ptr(), dest.as_mut_ptr(),
    utf16_len as core_foundation::base::CFIndex,
);
```

**当前不在 amux 仓库里 patch**——已经撤掉 `[patch.crates-io]`。

#### 4.13.4 Scrollback 选区接通 display_offset

参见 §4.11。

#### 4.13.5 PTY 子进程 HOME 隔离 → AMUX_HOME 抽象（生产级关键改动）

**症状**：在 amux 的终端里跑 `claude` 居然要求重新登录，但在系统 Terminal.app 里跑 `claude` 已经是登录状态。

**Root cause**：smoke 测试为了不污染真实 `~/.amux/`，启动 amux 时用 `HOME=/tmp/amux-smoke ./amux-desktop ...`。但**整个 amux 进程的 `HOME` 都被改成假目录**，PTY 子进程默认继承父 env，所以 `claude` 也看到假 HOME，去 `/tmp/amux-smoke/.claude/` 找授权 → 找不到 → 要求登录。

但**这个问题不只是 smoke 的副作用**——任何依赖 `HOME` 的下游 CLI（`gh`, `git`, `ssh`, `fish`/`zsh` 启动文件等）在 smoke / 多用户 / dotfiles repo / NAS 共享配置 等场景下都会撞墙。这是一个真正的产品级缺陷：amux 把"自己的配置目录"和"用户的真实 home"两个概念绑死成同一个 `HOME`。

**修复**（生产级重构）：

1. 新增 `crates/amux-platform/src/dirs.rs`，把这两个概念彻底分开：
   - `amux_home_dir() -> PathBuf`：amux 自己的配置目录。优先 `AMUX_HOME` env var → fallback 到 `~/.amux` (Unix) / `%USERPROFILE%\.amux` (Windows) → 最后 fallback 到 temp dir
   - `real_user_home() -> Option<PathBuf>`：用户的真实 OS home，用于 `~` 展开和 PTY 子进程继承
   - 常量 `AMUX_HOME_ENV = "AMUX_HOME"`
   - 单元测试覆盖 resolution order

2. 所有 amux 自己的路径解析改走 `amux_platform::amux_home_dir()`：
   - `apps/desktop/src/gpui_workspace_persistence.rs::amux_base_dir`
   - `crates/amux-ui/src/controller.rs::default_session_dir`（保留 app_name slug 兼容路径，但 `slug == "amux"` 时短路到 `amux_home_dir()`）

3. **PTY 子进程 env 完全不动**——`RealTerminalBackend` 用 `portable_pty::CommandBuilder` 默认继承父 env，amux 自己**绝不显式 set `HOME`**。所以只要 amux 启动时 `HOME` 是真实的，子进程就看到真实的 `HOME`。

4. `~` 展开（`gpui_entry.rs:1285` 和 `expand_tilde`）保持读 `HOME`/`USERPROFILE` —— 用户写 `~/projects` 期望解析到自己真实 home，而不是 amux 配置目录。这两处的语义本来就是对的，没改。

**结果**：

- 启动 amux 用 `AMUX_HOME=/tmp/amux-smoke/.amux ./amux-desktop ...`，**`HOME` 保留真实值**
- amux 自己的 session/layouts/config/screenshots 都写到 `/tmp/amux-smoke/.amux/`
- PTY 子进程继承 `HOME=/Users/arden`
- `claude` 找到 `~/.claude/` 配置，正常使用

**长期价值**：

- 用户可以把 `~/.amux/` 放进 dotfiles repo，`AMUX_HOME=$DOTFILES/amux ./amux`
- 可以把 amux 装在多用户 / 服务器场景下，每个用户用 `AMUX_HOME` 隔离自己的状态
- 可以在 CI / smoke 测试里用临时目录隔离，不污染真实环境
- amux 不再是依赖系统 `HOME` 的"侵入式"工具，而是符合 XDG 风格的"自包含"应用

---

## 5. 当前真正该看的入口文件

如果你要继续开发，不要平均用力扫全仓库，优先看下面这些入口。

### 5.1 跨平台核心入口

- [crates/amux-platform/src/lib.rs](/Users/arden/data/repository/ai/arden/amux/crates/amux-platform/src/lib.rs)
- [crates/amux-platform/src/services.rs](/Users/arden/data/repository/ai/arden/amux/crates/amux-platform/src/services.rs)
- [crates/amux-platform/src/capabilities.rs](/Users/arden/data/repository/ai/arden/amux/crates/amux-platform/src/capabilities.rs)
- [crates/amux-platform/src/dirs.rs](/Users/arden/data/repository/ai/arden/amux/crates/amux-platform/src/dirs.rs) — `amux_home_dir()` / `real_user_home()` / `AMUX_HOME` 常量

### 5.2 平台实现入口

- [crates/amux-platform/src/windows/platform.rs](/Users/arden/data/repository/ai/arden/amux/crates/amux-platform/src/windows/platform.rs)
- [crates/amux-platform/src/macos/mod.rs](/Users/arden/data/repository/ai/arden/amux/crates/amux-platform/src/macos/mod.rs)
- [crates/amux-platform/src/linux/mod.rs](/Users/arden/data/repository/ai/arden/amux/crates/amux-platform/src/linux/mod.rs)
- [crates/amux-platform/src/common/clipboard.rs](/Users/arden/data/repository/ai/arden/amux/crates/amux-platform/src/common/clipboard.rs) — `ArboardClipboardService`，三平台共用
- [crates/amux-platform/src/common/dialogs.rs](/Users/arden/data/repository/ai/arden/amux/crates/amux-platform/src/common/dialogs.rs) — `RfdWorkspaceDialogService`
- [crates/amux-platform/src/terminal/keys.rs](/Users/arden/data/repository/ai/arden/amux/crates/amux-platform/src/terminal/keys.rs) — keyboard event → PTY 字节编码器（独立模块，desktop 路径 `amux_platform::terminal::keys`）

### 5.3 UI 装配入口

- [crates/amux-ui/src/controller.rs](/Users/arden/data/repository/ai/arden/amux/crates/amux-ui/src/controller.rs) — `restore_session_if_present` / `open_local_workspace` / `seed_demo_state`(test-only)
- [crates/amux-ui/src/root.rs](/Users/arden/data/repository/ai/arden/amux/crates/amux-ui/src/root.rs) — `DesktopApp::startup(StartupOptions) -> StartupResult`，新的产品启动入口
- [crates/amux-ui/src/commands.rs](/Users/arden/data/repository/ai/arden/amux/crates/amux-ui/src/commands.rs) — `UiAction::OpenLocalWorkspace`（已不再叫 OpenWindowsWorkspace）
- [crates/amux-ui/src/state.rs](/Users/arden/data/repository/ai/arden/amux/crates/amux-ui/src/state.rs)

### 5.4 Desktop 真正活跃入口

- [apps/desktop/src/main.rs](/Users/arden/data/repository/ai/arden/amux/apps/desktop/src/main.rs)
- [apps/desktop/src/gpui_entry.rs](/Users/arden/data/repository/ai/arden/amux/apps/desktop/src/gpui_entry.rs)
- [apps/desktop/src/gpui_input_handler.rs](/Users/arden/data/repository/ai/arden/amux/apps/desktop/src/gpui_input_handler.rs)
- [apps/desktop/src/gpui_layout_renderer.rs](/Users/arden/data/repository/ai/arden/amux/apps/desktop/src/gpui_layout_renderer.rs)
- [apps/desktop/src/gpui_terminal.rs](/Users/arden/data/repository/ai/arden/amux/apps/desktop/src/gpui_terminal.rs)
- [apps/desktop/src/gpui_preview.rs](/Users/arden/data/repository/ai/arden/amux/apps/desktop/src/gpui_preview.rs)
- [apps/desktop/src/gpui_browser.rs](/Users/arden/data/repository/ai/arden/amux/apps/desktop/src/gpui_browser.rs)
- [apps/desktop/src/gpui_workspace_sidebar.rs](/Users/arden/data/repository/ai/arden/amux/apps/desktop/src/gpui_workspace_sidebar.rs)

---

## 6. 哪些文件现在不要误判

下面这些情况最容易坑到接手的人。

### 6.1 文件还在，不代表运行时还在用

目前下列文件仍保留在仓库里，但已不在 `main.rs` 的编译路径中：

- `apps/desktop/src/gpui_command_bar.rs`
- `apps/desktop/src/gpui_command_palette.rs`
- `apps/desktop/src/gpui_components.rs`
- `apps/desktop/src/gpui_keyboard_shortcuts.rs`
- `apps/desktop/src/gpui_surface_views.rs`
- `apps/desktop/src/gpui_terminal_component.rs`

这些文件可以参考，但不要优先在这里继续加功能。

同样地，`amux-platform` 内部也有一组已经移出编译路径的死代码模块：

- `crates/amux-platform/src/terminal/emulator.rs`
- `crates/amux-platform/src/terminal/view.rs`
- `crates/amux-platform/src/terminal/session.rs`

它们在 `crates/amux-platform/src/terminal/mod.rs` 中已经被注释为不参与编译。
这三个文件构成了早期一套自研 terminal 实现：

- `emulator.rs` 是自研 ANSI 解析器与 cell grid（`TerminalEmulator`）
- `view.rs` 是基于该 emulator 的 view 包装（`TerminalView`）
- `session.rs` 是与 `terminal/manager.rs` + `terminal/backend.rs` 并行的旧 PTY session 管理
  （`TerminalSessionManager` / `TerminalSession` / `keyboard_to_pty`）

它们已被 `terminal/alacritty_view.rs` 中的 `AlacrittyTerminal`
（基于 `alacritty_terminal::Term`）整体替代，是当前 desktop 唯一驱动的 emulator。
不要把这三个旧文件当成现行 terminal 抽象的一部分。

注意：原本 `view.rs` 中嵌着的 `pub mod keys` 是仍在被 `gpui_entry.rs` 使用的活代码
（keyboard event → PTY 字节编码器），跨平台清理时已经被提取到独立的
`crates/amux-platform/src/terminal/keys.rs`，路径仍然是
`amux_platform::terminal::keys`。

### 6.2 `gpui_workspace_sidebar.rs` 不是完整 sidebar 实现

当前真正渲染 sidebar 的地方在 [gpui_entry.rs](/Users/arden/data/repository/ai/arden/amux/apps/desktop/src/gpui_entry.rs)。

[gpui_workspace_sidebar.rs](/Users/arden/data/repository/ai/arden/amux/apps/desktop/src/gpui_workspace_sidebar.rs) 现在只是共享状态和数据模型，不再负责实际渲染。

### 6.3 `Ctrl+Shift+N` 已不是历史行为

历史上它会调用一个不存在的 `new workspace` 命令。

现在它已经改成：

- 优先走平台 folder picker
- 失败或不可用时回退到 command palette 的 `workspace open `

不要把它改回“直接打开当前目录”或“静默失败”。

### 6.4 Workspace 打开逻辑要复用统一入口

目前 desktop 里的 workspace 打开链应该优先复用：

- `DesktopApp::pick_workspace_folder()`
- `DesktopApp::open_local_workspace(...)`
- `GpuiShellView::prompt_open_local_workspace()`

不要在 sidebar、context menu、快捷键里各自复制一套。

---

## 7. 当前回归红线

后续开发必须守住的红线：

1. 不主动破坏 Windows 本地 workspace 打开
2. 不主动破坏 WSL workspace 打开
3. 不主动破坏 terminal 启动和输入输出
4. 不主动破坏 path mapping
5. 不主动破坏 session restore
6. 不主动破坏 agent 启动链

尤其下面这些文件是高风险区：

- [crates/amux-core/src/workspace/target.rs](/Users/arden/data/repository/ai/arden/amux/crates/amux-core/src/workspace/target.rs)
- [crates/amux-core/src/surface/terminal.rs](/Users/arden/data/repository/ai/arden/amux/crates/amux-core/src/surface/terminal.rs)
- [crates/amux-platform/src/path_mapper.rs](/Users/arden/data/repository/ai/arden/amux/crates/amux-platform/src/path_mapper.rs)
- [crates/amux-platform/src/terminal/backend.rs](/Users/arden/data/repository/ai/arden/amux/crates/amux-platform/src/terminal/backend.rs)
- [crates/amux-ui/src/controller.rs](/Users/arden/data/repository/ai/arden/amux/crates/amux-ui/src/controller.rs)
- [apps/desktop/src/gpui_entry.rs](/Users/arden/data/repository/ai/arden/amux/apps/desktop/src/gpui_entry.rs)

---

## 8. 建议的继续开发顺序

如果你接下来要继续推进，建议按下面顺序做。

### 8.1 第一优先级

继续压 warning，但只清低风险路径：

- `gpui_browser.rs`
- `gpui_entry.rs`
- `gpui_preview.rs`
- `amux-platform` terminal 子模块

目标不是“warning 全清”，而是先把**核心活动路径**上的噪音降下来。

### 8.2 第二优先级

补平台能力的产品化收口：

- browser capability 的更明确降级
- clipboard 的真实平台实现
- macOS/Linux 更完整的 folder picker / browser host / clipboard

### 8.3 第三优先级

继续做真正还没闭环的能力：

- 正式启动流替换 demo bootstrap
- editor 的编辑/dirty/save 闭环
- browser 生命周期和平台兜底
- macOS/Linux 更完整的 smoke 流程

---

## 9. 不建议现在做的事

下面这些事现在看起来“很想做”，但并不建议当前阶段先做：

1. 不要为了抽象美观重写 Windows 稳定链路
2. 不要把 desktop 壳层整个重写成新 UI 系统
3. 不要先去大规模整理所有历史文件
4. 不要把旧模块直接物理删除到不可恢复
5. 不要先改 repo 里所有文档

当前最重要的是：

`继续沿着现有跨平台骨架收口，而不是重开一条新的整理线`

---

## 10. 最低验证命令

每次提交前，至少跑：

```bash
cargo check -q -p amux-desktop --features gpui
```

如果你动了 platform / ui / session 关键链路，再补：

```bash
cargo check -q -p amux-platform
cargo check -q -p amux-ui
```

如果你动了更底层的跨平台模型，再看情况补：

```bash
cargo test -q -p amux-platform
cargo test -q -p amux-session
```

注意：

- 当前 repo-wide `cargo test -q` 不应默认作为每轮都跑的最低门槛
- 历史上它出现过与当前工作无关的失败项
- 除非你正在处理那条测试线，否则不要被它带偏

### 10.1 macOS / Linux 真实窗口烟雾测试

要在 macOS 或 Linux 上真跑一次 GUI（不只是 cargo check），用 `AMUX_HOME` 隔离配置：

```bash
mkdir -p /tmp/amux-smoke/.amux /tmp/amux-smoke-ws
AMUX_HOME=/tmp/amux-smoke/.amux \
  ./target/debug/amux-desktop --real --workspace /tmp/amux-smoke-ws
```

**重点**：用 `AMUX_HOME` 而不是 `HOME=...`。前者只隔离 amux 自己的配置目录，**保留真实 `HOME`**；后者会污染 PTY 子进程，导致 `claude` / `gh` / `git` / `ssh` 等下游 CLI 找不到自己的 `~/.{config}` 失效。详见 §4.13.5。

如果窗口起来后所有文字看不见，先 `nm target/debug/amux-desktop | grep glyph_for_char`：
- 只有 `NoopTextSystem::glyph_for_char` → 漏开 `gpui_platform/font-kit` feature，详见 §4.13.2
- 有 `MacTextSystemState::glyph_for_char` → 真的字体问题，看 config.toml 的 `font_family`

---

## 11. 当前已知事实

截至这次交接，最近确认过的事实是：

- `cargo check -q -p amux-desktop --features gpui` 通过
- desktop 侧多平台 capability gating 已接通
- folder picker 已接入平台层
- workspace 打开入口已统一
- 一批未接线旧 UI 模块已移出编译路径

如果你接手后发现这些结论不再成立，优先检查：

1. 你是否还在 `cross-platform-foundation`
2. 工作区里是否有新的未提交改动
3. 是否有人把旧模块重新接回了 `main.rs`

---

## 12. 接手建议

真正开始动手前，建议先做这 4 件事：

1. `git branch --show-current`
2. `git status --short`
3. `cargo check -q -p amux-desktop --features gpui`
4. 先读一遍 [controller.rs](/Users/arden/data/repository/ai/arden/amux/crates/amux-ui/src/controller.rs) 和 [gpui_entry.rs](/Users/arden/data/repository/ai/arden/amux/apps/desktop/src/gpui_entry.rs)

如果这 4 步都没异常，再继续开发。

这份文档的目标不是完整替代读代码，而是让你避免从错误入口开始。
