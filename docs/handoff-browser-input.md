# 交接文档：浏览器 URL 输入框焦点问题

## 当前状态

浏览器面板（WebView2）基本可用：能打开网页、显示内容、拖拽缩放。但 URL 输入框（gpui-component 的 Input 组件）无法正常接收键盘输入。

## 核心问题

gpui-component 的 `Input` 组件需要通过 GPUI 的焦点系统接收键盘输入。但 Amux 的事件处理架构与之冲突，导致 Input 无法稳定持有焦点。

## 已确认的冲突点

### 1. `track_focus` 抢焦点

**文件**: `apps/desktop/src/gpui_entry.rs` render() 方法

```rust
div().track_focus(&self.focus_handle)
```

`.track_focus()` 是 GPUI 的内建行为——任何点击事件会自动调用 `self.focus_handle.focus()`。这包括点击 Input 组件所在区域。执行顺序：

1. 用户点击 Input
2. GPUI 的 `track_focus` 自动聚焦 root 的 focus_handle
3. 我们的 `on_mouse_down` 回调执行（在 track_focus 之后）
4. 回调中尝试把焦点还给 Input

**问题**：步骤 4 的 re-focus 尝试不稳定。有时有效，有时无效。原因未确定——可能是 GPUI 内部的焦点调度时序问题。

**尝试过的方案**：
- 去掉 `track_focus` → `on_key_down` 不再触发，终端键盘完全失效
- 在 `on_mouse_down` 中 re-focus Input → 不稳定
- 用 `contains_focused()` 替代 `is_focused()` → 导致所有输入都失效

### 2. `window.handle_input()` 覆盖

**文件**: `apps/desktop/src/gpui_entry.rs` render() 方法

```rust
window.handle_input(
    &focus_for_ime,
    gpui::ElementInputHandler::new(bounds, view_entity),
    cx,
);
```

每帧 render 时注册 Amux 的 `EntityInputHandler`（处理 IME/中文输入）。这会覆盖 Input 组件自己注册的 handler。

**当前方案**：用 `child_input_focused` 标志位条件跳过注册：
```rust
let register_ime = !child_input_focused;
.when(register_ime, |this| { ... handle_input ... })
```

### 3. `on_key_down` 全局拦截

**文件**: `apps/desktop/src/gpui_input_handler.rs` `on_global_key_down()`

root div 上的 `on_key_down` 拦截所有键盘事件并发送到终端 PTY。当 Input 有焦点时需要跳过：

```rust
if let Some(ref url_input) = self.browser_url_input {
    if url_input.read(cx).focus_handle(cx).is_focused(window) {
        if event.keystroke.key == "escape" {
            self.focus_handle.focus(window, cx);
        }
        return;
    }
}
```

### 4. `replace_text_in_range` 拦截

**文件**: `apps/desktop/src/gpui_input_handler.rs` `EntityInputHandler` impl

IME 文本输入处理器也需要在 Input 有焦点时跳过。

### 5. render() 每帧抢焦点

**文件**: `apps/desktop/src/gpui_entry.rs` render() 开头

```rust
if !self.focus_handle.is_focused(window) && !child_input_focused {
    self.focus_handle.focus(window, cx);
}
```

如果 `child_input_focused` 判断不准（`is_focused` 有时序问题），焦点会被每帧抢回。

## 涉及文件

| 文件 | 相关代码 |
|---|---|
| `apps/desktop/src/gpui_entry.rs` | GpuiShellView struct、render()、open_browser()、close_browser()、焦点管理 |
| `apps/desktop/src/gpui_input_handler.rs` | on_global_key_down()、EntityInputHandler impl（replace_text_in_range） |
| `apps/desktop/src/gpui_browser.rs` | BrowserPaneState、render_browser_pane()、URL bar 中的 Input 渲染 |
| `apps/desktop/src/gpui_config.rs` | AmuxConfig |

## 关键数据流

```
用户按键
  ↓
GPUI 焦点系统 → 派发到焦点链
  ↓
如果 root div 有焦点:
  on_key_down → on_global_key_down → handle_terminal_input → PTY
  replace_text_in_range → PTY (IME 文本)

如果 Input 有焦点:
  Input 组件 action handlers (Backspace, Delete, 箭头等)
  Input 组件 EntityInputHandler (文本输入)
  on_key_down 冒泡到 root → 检测 Input 焦点 → return (不发送到 PTY)
```

## 可能的解决方向

### 方向 A：分离事件处理区域

不在 root div 上注册 `on_key_down`，改在终端 content area 的 div 上注册。终端区域和浏览器区域各自独立处理键盘。

**优点**：从根本上消除冲突
**缺点**：需要重构 gpui_entry.rs 和 gpui_input_handler.rs 的大量代码

### 方向 B：使用 GPUI 的 Action 系统

参考 gpui-component Input 的做法——用 GPUI Action 系统替代 `on_key_down`。注册终端专用的 action context，让 GPUI 根据焦点自动路由。

**优点**：和 GPUI 的设计一致
**缺点**：工作量大，需要把所有快捷键迁移到 action 系统

### 方向 C：双 focus scope

在浏览器面板外层加一个独立的 `track_focus` scope。点击浏览器区域时，浏览器 scope 的 focus handle 获得焦点（不是 root 的）。然后浏览器内部的 Input 管理自己的焦点。

**优点**：改动较小
**缺点**：需要理解 GPUI 的 focus scope 嵌套行为

### 方向 D：放弃 gpui-component Input，用原生 Win32 EDIT 控件

用 `CreateWindowExW("EDIT", ...)` 创建一个 Win32 原生文本输入框，像 WebView2 一样作为子窗口定位到 URL 栏位置。原生控件自带完整文本编辑功能。

**优点**：完全绕过 GPUI 焦点冲突
**缺点**：仅 Windows，需要 Win32 API 调用，样式不统一

## 其他待解决的产品体验问题

详见本文档同目录下审计结果。按优先级：

1. **所有文本输入框**（workspace rename、tab rename、search、file picker）都缺少光标/选中功能——同样需要 gpui-component Input 或类似方案
2. **右键菜单**不能用方向键导航（任意按键直接关闭）
3. **右键菜单 Copy** 永远禁用（`has_selection = false // TODO`）
4. **文件选择器**最多 20 个结果无提示
5. **终端搜索**无匹配计数

## 环境信息

- Rust edition 2024
- GPUI: git dep from zed-industries/zed
- gpui-component: git dep from longbridge/gpui-component
- WebView2: via wry 0.53
- 目标平台: Windows (WSL 支持)
- `gpui_component::init(cx)` 已在 app 启动时调用
- `gpui_component::Root` 已包裹 GpuiShellView
- `gpui_component::ThemeMode::Dark` 已设置
