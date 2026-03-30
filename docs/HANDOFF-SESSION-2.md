# AMUX 开发交接文档 — Session 2

## 本次会话完成的工作

### Canvas 渲染引擎重构
- div-based → GPUI canvas（paint_quad + ShapedLine）
- 动态字体测量（20字符平均法，消除 cell_w 偏差）
- 宽字符（CJK）独立 shape，防止光标漂移
- 文本装饰：underline、italic、strikethrough
- Block/box drawing 字符像素级渲染

### 鼠标事件完整支持
- 左键点击/拖动/释放 → 转发 PTY（TUI 应用可交互）
- 右键 → mouse mode 时转发 PTY，否则弹菜单
- 滚轮 → SGR + legacy 编码，正确坐标
- `intersects` 替代 `contains` 检测 MOUSE_MODE
- 中键粘贴
- 文本选择 + 自动复制（alacritty selection API）

### Vibe Coding 工具集成
- 自动检测 Claude/OpenCode/Codex/Aider/Gemini/Copilot
- WSL 双环境检测（native + WSL）
- 一键 split pane 启动，自动命名 tab
- Login shell（bash -ilc）确保 PATH 完整
- 检测结果启动时缓存，右键菜单瞬开

### Tab & Pane 管理
- Tab 拖拽移动到其他 pane（move_tab_to_pane）
- Tab close 按钮
- Tab 双击改名（custom_title 优先于 terminal title）
- Tab 标题自适应宽度（min/max/shrink/overflow）
- Workspace 拖拽排序（持久化顺序）
- Workspace 删除
- Zoom pane（Ctrl+Shift+F）+ ZOOMED 指示器
- 均分布局（Ctrl+Shift+E）

### Workspace Startup Commands
- `~/.amux/workspaces/<name>.startup` 配置文件
- `[pane:N title=xxx]` 语法，自定义 tab 标题
- 空 workspace 自动执行 startup
- 右键菜单 "Edit Startup" / "Run Startup"

### 活动通知
- Tab 级：绿色圆点标记有新输出的非活跃 tab
- Workspace 级：侧栏 workspace 名字旁的绿色圆点
- 通过 cursor line 变化检测活动

### 截图粘贴
- Ctrl+Shift+V：剪贴板图片 → 保存文件 → 插入路径
- Windows → WSL 路径自动转换

### 终端搜索
- Ctrl+F 打开搜索栏
- 增量搜索，Enter/Shift+Enter 跳转
- alacritty RegexSearch API

### 启动优化
- PTY spawn 延迟到第一帧后
- 工具检测延迟到第三帧
- CREATE_NO_WINDOW 消除子进程窗口闪烁
- windows_subsystem = "windows" 隐藏 console

### 稳定性修复
- 6 个隐藏崩溃 bug 修复（除零、unwrap、index OOB 等）
- restore_layout 验证 active_pane
- terminal_manager_mut 自动创建缺失的 manager

### 构建系统
- `scripts/build.sh`（Linux/macOS）
- `scripts/build.ps1`（Windows）
- `.github/workflows/build.yml`（CI/CD 三平台）
- `gpui-linux` feature 分离 Wayland/X11

---

## 待解决的已知问题

### 1. 光标位置与文字的累积偏差（优先级：高）
**现象**：beam 光标跟前面的文字有空格间距，字符越多偏差越大。
**根因**：光标位置 = `cursor_col × cell_w`，但文本渲染用字体 ShapedLine 的实际 advance 宽度。cell_w 是 20 字符平均值，跟单字符 advance 有微小差异。
**修复方向**：光标定位不用 `col × cell_w`，而是 shape 光标行文本到 cursor_col，用实际 shaped width 定位 beam。

### 2. Block cursor 渲染路径 bug（优先级：中）
**现象**：修改 `data.grid[row][col].bg` 后 bg_rects 循环没有正确读取新颜色。
**临时方案**：在 bg_rects 循环里 inline split rect，直接覆盖光标位置的颜色。
**根因未明**：grid cell 的 Rgba `==` 比较可能有浮点精度问题，或者有其他代码覆盖了修改。

### 3. TUI 应用（Claude/Codex）光标显示（优先级：中）
**现状**：active pane 强制显示 beam 光标（即使 TUI hide cursor），位置基本正确但有偏差（#1）。
**风险**：某些 TUI 应用可能在非输入区域也有光标位置变化，强制显示 beam 可能在错误位置出现光标。

### 4. 浏览器面板集成（优先级：低，未开始）
**方案**：用 `wry` crate 嵌入系统 WebView。需要获取 GPUI 原生窗口句柄。
**参考**：limux 用 WebKitGTK（`webkit6` crate），但 AMUX 用 GPUI 不是 GTK。

---

## 关键架构信息

### 文件结构
```
apps/desktop/src/
  gpui_entry.rs         主窗口、输入、布局、所有交互逻辑（~2800行）
  gpui_terminal.rs      Canvas 终端渲染器（~900行）
  gpui_status_bar.rs    状态栏
  gpui_workspace_sidebar.rs  侧栏（大部分是死代码，实际 UI 在 gpui_entry.rs）

crates/amux-platform/src/terminal/
  manager.rs            Pane/Tab/Layout 管理、activity 检测
  alacritty_view.rs     AlacrittyTerminal 封装

crates/amux-ui/src/
  commands.rs           Command Palette 命令目录
```

### 渲染流程
```
render() → collect_render_data() → canvas(prepaint, paint)
  prepaint: shape text runs, collect bg_rects/cursor_rects/selection_rects
  paint: paint_quad(bg) → paint_quad(selection) → paint_quad(special) → ShapedLine.paint(text) → paint_quad(cursor)
```

### 光标渲染
- Block cursor (shape=0)：在 bg_rects 循环里 inline split，不走 cursor_rects
- Beam cursor (shape=1)：cursor_rects，2px 宽竖线
- Underline cursor (shape=2)：cursor_rects，底部横线
- Active pane 强制 beam（cursor_visible=true, cursor_shape=1）

### 快捷键
```
Ctrl+D          Split Right
Ctrl+Shift+D    Split Down
Ctrl+T          New Tab
Ctrl+W          Close Pane
Ctrl+Shift+F    Zoom/Restore
Ctrl+Shift+E    Equalize Splits
Ctrl+F          Terminal Search
Ctrl+Shift+V    Smart Paste (image → path)
Ctrl+M          Toggle Sidebar
Ctrl+K          Clear
Ctrl+Q          Quit
```

### 编译
```bash
# Linux（当前开发环境）
cargo build -p amux-desktop --features gpui-linux --release

# Windows / macOS
cargo build -p amux-desktop --features gpui --release
```
