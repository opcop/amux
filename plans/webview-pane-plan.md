# WebView2 嵌入式浏览器 Pane 实现计划

## 目标

在 Amux 中新增 Browser Pane 类型，嵌入 WebView2 浏览器。开发者可以左边上下 split 跑前后端 dev server，右边 pane 实时查看网页，一屏内完成全栈开发调试。

## 技术方案

### 核心原理

GPUI 直接用 GPU 渲染，没有原生子窗口概念。WebView2 是一个 Win32 窗口。解决方案是**浮动子窗口**：

1. GPUI layout 阶段计算 Browser Pane 的屏幕区域 (x, y, w, h)
2. 用 Win32 `SetWindowPos` 把 WebView2 窗口精确定位到该区域
3. 每帧检查位置变化，有变化时重新定位

这和 VS Code（Electron）嵌入终端、BrowserView 的原理完全一样。

### 依赖

```toml
# Cargo.toml (amux-platform 或新建 amux-browser crate)
[target.'cfg(windows)'.dependencies]
webview2-com = "0.33"       # WebView2 COM API Rust binding
windows = { version = "0.58", features = [
    "Win32_Foundation",
    "Win32_UI_WindowsAndMessaging",
    "Win32_Graphics_Gdi",
]}
```

`webview2-com` 是微软官方维护的 Rust binding，封装了 ICoreWebView2 COM 接口。

### Linux 方案（后续）

Linux 上用 `webkit2gtk`。API 不同但概念一致。通过 `#[cfg(target_os)]` 隔离平台差异，对上层暴露统一接口。首期只做 Windows。

---

## 模块设计

### 1. `BrowserPane` 数据结构

```
crates/amux-platform/src/browser/
├── mod.rs          // pub mod, re-exports
├── webview.rs      // WebView2 创建、导航、生命周期
└── controller.rs   // URL 栏状态、历史、事件处理
```

```rust
pub struct BrowserPane {
    pub url: String,
    pub title: String,
    pub loading: bool,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    // WebView2 COM 对象（Windows only）
    webview: Option<WebView2Handle>,
    // 当前屏幕位置，用于检测是否需要 SetWindowPos
    last_bounds: (i32, i32, i32, i32),  // x, y, w, h
}
```

### 2. `PaneContent` 枚举扩展

当前 `TerminalPane` 只持有终端 tab。需要扩展为支持不同类型的 pane content：

```rust
pub enum PaneContent {
    Terminal(TerminalTabs),    // 现有的终端 tab 集合
    Browser(BrowserPane),      // 新增：嵌入式浏览器
}
```

或者更简单的方案：在 `TerminalManager` 的 pane tree 之外，独立管理 browser pane，用 `pane_id` 关联。这样改动更小，不需要重构现有 pane 系统。

**推荐后者**——侵入性最小。

### 3. WebView2 生命周期

```
创建 Browser Pane
  ↓
获取 GPUI 窗口的 HWND（父窗口）
  ↓
CreateCoreWebView2EnvironmentAsync
  ↓
CreateCoreWebView2ControllerAsync(parent_hwnd)
  ↓
controller.put_Bounds(pane_rect)  ← 每帧更新
  ↓
webview.Navigate(url)
```

关键 API：
- `ICoreWebView2Controller::put_Bounds` — 设置 WebView2 在父窗口内的位置和大小
- `ICoreWebView2Controller::put_IsVisible` — 显示/隐藏（pane 被遮挡或 zoom 其他 pane 时）
- `ICoreWebView2::Navigate` — 导航到 URL
- `ICoreWebView2::add_NavigationCompleted` — 导航完成回调
- `ICoreWebView2::add_DocumentTitleChanged` — 标题变化回调

### 4. GPUI 渲染集成

在 `gpui_layout_renderer.rs` 中，Browser Pane 的渲染区域用一个**空白 div**占位（背景透明或与 WebView2 重叠）。实际内容由 WebView2 子窗口渲染。

```rust
// 在 render_layout 中，遇到 Browser Pane 时：
if let Some(browser) = self.browser_panes.get(&pane_id) {
    // 1. 记录 pane 的屏幕坐标
    let bounds = (px_x, px_y, pw, ph);
    
    // 2. 渲染 URL 栏 + 控制按钮（GPUI 原生渲染）
    //    ┌─[←] [→] [↻] [http://localhost:3000_________]─┐
    //    │                                                │
    //    │         (WebView2 子窗口覆盖此区域)             │
    //    │                                                │
    //    └────────────────────────────────────────────────┘
    
    // 3. 在 paint 阶段，调用 SetWindowPos 同步位置
    browser.sync_bounds(bounds);
}
```

URL 栏在上方，由 GPUI 渲染（和 tab strip 类似）。下方区域完全交给 WebView2。

### 5. 焦点管理

- 点击 Browser Pane 区域 → WebView2 获取焦点（可以在网页内交互）
- 点击终端 Pane → GPUI 获取焦点，WebView2 失焦
- 快捷键 Ctrl+数字 切换 pane 时也要同步焦点
- WebView2 获取焦点时，Amux 的全局快捷键（Ctrl+P 等）需要仍然可用

焦点切换通过 `ICoreWebView2Controller::MoveFocus` 和 GPUI 的 `focus_handle` 配合。

### 6. 获取 GPUI 窗口 HWND

WebView2 需要一个父 HWND。GPUI 的窗口是原生 Win32 窗口，需要获取其 HWND：

```rust
// GPUI 内部有 platform::windows::Window，持有 HWND
// 需要通过 window.raw_handle() 或类似 API 获取
// 如果 GPUI 没有直接暴露，可以用 FindWindow 或 GetForegroundWindow
```

这是潜在的技术风险点——需要确认 GPUI 是否暴露了 HWND。如果没有，可以用 `raw-window-handle` crate（GPUI 可能已经实现了 `HasRawWindowHandle`）。

---

## 用户交互设计

### 创建 Browser Pane

- 右键菜单 → **"Open Browser"** → 弹出 URL 输入框，默认 `http://localhost:3000`
- 快捷键 **Ctrl+Shift+B** → 同上
- 在终端里输入 `amux browser http://localhost:3000` → 在当前 pane 旁边打开浏览器

### URL 栏

```
┌─[←] [→] [↻] [🔒 localhost:3000/dashboard          ]──[✕]─┐
│                                                            │
│                   (网页内容)                                │
│                                                            │
└────────────────────────────────────────────────────────────┘
```

- 点击 URL 栏可编辑，Enter 导航
- 点击 ← → ↻ 前进/后退/刷新
- ✕ 关闭 Browser Pane
- 显示 loading spinner 和页面标题

### DevTools

- **F12** 或右键 → Inspect → 打开 WebView2 内置 DevTools
- WebView2 原生支持，不需要额外实现

### 与终端联动

- 终端检测到 `npm run dev` / `cargo run` 等输出了 `http://localhost:XXXX` 时，在状态栏显示可点击链接
- 点击链接 → 自动在 Browser Pane 中打开（如果没有 Browser Pane 则创建一个）

---

## 开发阶段

| 阶段 | 内容 | 复杂度 |
|---|---|---|
| **Phase 1** | WebView2 初始化 + 基础导航 + 固定位置显示 | ■■□□□ |
| **Phase 2** | GPUI 坐标同步 + pane resize/split 跟随 | ■■■□□ |
| **Phase 3** | URL 栏 UI + 前进/后退/刷新/标题 | ■■□□□ |
| **Phase 4** | 焦点管理 + 快捷键 + zoom 支持 | ■■□□□ |
| **Phase 5** | 终端联动（自动检测 dev server URL） | ■□□□□ |
| **Phase 6** | Linux webkit2gtk 支持 | ■■■□□ |

**Phase 1-2 是核心**，完成后就有可用的嵌入式浏览器。Phase 3-5 是体验打磨。

## 风险点

1. **HWND 获取**：需确认 GPUI 暴露了原生窗口句柄。如未暴露需 patch GPUI 或用 hack 方式获取
2. **坐标精度**：GPUI 用浮点坐标（px），Win32 用整数像素。DPI 缩放下可能有 1px 对齐问题
3. **渲染层叠**：WebView2 子窗口始终在 GPUI 内容之上。弹出的 context menu、file picker 等 overlay 会被 WebView2 遮挡。需要在显示 overlay 时临时隐藏 WebView2，或使用 `WS_EX_TRANSPARENT`
4. **多实例**：WebView2 环境初始化是异步的。多个 Browser Pane 需要共享同一个 environment 实例
