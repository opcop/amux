# AMUX 完整开发计划

## 文档信息

- **版本**: v1.0
- **目标**: 对标 limux，实现跨平台 (Windows/Linux/macOS) 的 AI Coding Workspace
- **参考项目**: third_party/limux
- **最后更新**: 2026-03-28

---

## 1. 产品目标与范围

### 1.1 产品定位

AMUX 是一个跨平台 (Windows/Linux/macOS) 的 GPU 加速终端工作区管理器，专注于 AI Coding 工作流。

核心价值：
- 统一管理多个项目工作区
- 多 Pane/多 Tab 布局系统
- 集成终端和 AI Agent
- 轻量文件浏览和编辑
- Session 持久化与恢复

### 1.2 首版功能范围

| 类别 | 功能 | 优先级 |
|------|------|--------|
| 工作区 | 创建/关闭/切换/重命名/拖拽排序 | P0 |
| 布局 | Pane 分割/关闭/焦点移动/比例调整 | ✅ 已实现 |
| 标签页 | 创建/关闭/切换/拖拽移动 | ✅ 已实现 |
| 终端 | PTY 会话/渲染/复制粘贴 | ✅ 已实现 |
| AI Agent | 探测/启动/管理 | ✅ 已实现 |
| 文件树 | 目录浏览/过滤/文件操作 | ✅ 已实现 |
| 编辑器 | 轻量编辑/语法高亮 | ✅ 已实现 |
| 预览 | Markdown/图片预览 | ✅ 已实现 |
| 会话 | 自动保存/恢复 | ✅ 已实现 |
| 命令面板 | 快速命令执行/切换器 | ✅ 已实现 |
| 快捷键 | 完整键盘导航 | ✅ 已实现 |
| 上下文菜单 | 右键菜单操作 | ✅ 已实现 |
| 浏览器 | 内置 WebView 标签 | P2 |
| 通知 | 系统通知/未读标记 | P2 |
| 查找 | 终端/面板内搜索 | P2 |
| 外部控制 | Unix Socket IPC | P3 |

### 1.3 首版不做

- 完整 LSP 支持
- Git GUI
- 插件系统
- 高级 Diff/Merge
- 远程容器/SSH Workspace

---

## 2. 架构设计

### 2.1 模块划分

```
apps/desktop           # 桌面应用入口 (GPUI)
├── gpui_entry.rs      # GPUI 应用主入口
├── gpui_*.rs          # GPUI 组件实现
crates/amux-core       # 纯业务状态层 (平台无关)
├── workspace/         # 工作区状态与操作
├── layout/            # 布局树状态与操作
├── surface/           # 表面状态 (Terminal/Agent/Editor/...)
├── session/           # 会话状态与持久化
├── command.rs         # 命令定义
└── event.rs           # 领域事件
crates/amux-platform   # 平台抽象层
├── terminal/          # 终端后端 (PTY)
├── process.rs         # 进程管理
├── shell.rs           # Shell 检测
├── fs.rs              # 文件系统抽象
└── windows/           # Windows 平台实现
    ├── conpty.rs      # Windows ConPTY
    ├── wsl.rs         # WSL 集成
    └── wsl_fs.rs      # WSL 文件系统
crates/amux-agent      # AI Agent 集成
├── provider.rs        # Agent 提供商定义
├── launch.rs          # Agent 启动逻辑
├── registry.rs        # Agent 注册表
└── profiles.rs        # Agent 配置
crates/amux-workspace  # 工作区服务
├── manager.rs         # 工作区管理
├── file_tree.rs       # 文件树服务
├── recent.rs          # 最近文件
└── watcher.rs         # 文件监视
crates/amux-session    # 会话持久化
├── store.rs           # 会话存储
├── codec.rs           # 序列化编解码
└── paths.rs           # 路径管理
crates/amux-ui         # UI 层 (GPUI + Text)
├── components/        # UI 组件
│   ├── workspace_sidebar.rs
│   ├── pane_grid.rs
│   ├── tab_strip.rs
│   ├── command_palette.rs
│   ├── title_bar.rs
│   └── ...
├── controller.rs      # UI 控制器
├── state.rs           # UI 状态快照
├── commands.rs        # 命令解析
└── render/            # 渲染器
```

### 2.2 数据模型

```
SessionState
├── version: u32
├── active_workspace_id: Option<WorkspaceId>
├── workspaces: Vec<WorkspaceState>
├── recent_workspaces: Vec<RecentWorkspace>
└── ui_preferences: UiPreferences

WorkspaceState
├── id: WorkspaceId
├── name: String
├── target: WorkspaceTarget (WindowsPath | WslPath)
├── layout: LayoutNode
├── active_pane_id: PaneId
└── env_profile_id: Option<String>

LayoutNode
├── Split(SplitNode)  # 水平/垂直分割
└── Pane(PaneNode)   # 面板叶子节点

PaneNode
├── pane_id: PaneId
├── tabs: Vec<TabState>
└── active_tab_id: TabId

TabState
├── id: TabId
├── title: String
├── pinned: bool
└── surface: SurfaceState

SurfaceState
├── Terminal(TerminalSurfaceState)
├── Agent(AgentSurfaceState)
├── FileTree(FileTreeSurfaceState)
├── Editor(EditorSurfaceState)
├── Preview(PreviewSurfaceState)
├── Welcome(WelcomeSurfaceState)
└── Settings(SettingsSurfaceState)
```

### 2.3 平台差异处理

| 功能 | Windows | Linux | macOS |
|------|---------|-------|-------|
| 终端后端 | ConPTY | pty | pty |
| Shell | PowerShell/CMD | bash/zsh | bash/zsh |
| WSL 集成 | 原生支持 | N/A | N/A |
| GUI 框架 | GPUI | GPUI | GPUI |
| 系统托盘 | 可选 | 可选 | 可选 |
| 通知 | Windows Toast | libnotify | NSUserNotification |

---

## 3. 功能实现计划

### Phase 1: 核心布局系统 (P0)

#### 1.1 Workspace 管理

**目标**: 实现完整的工作区生命周期管理

| 功能 | 描述 | 实现位置 | 状态 |
|------|------|----------|------|
| 创建工作区 | 基于文件夹路径创建工作区 | `workspace/ops.rs` | ✅ 已实现 |
| 关闭工作区 | 关闭并清理工作区 | `workspace/ops.rs` | ✅ 已实现 |
| 切换工作区 | 通过快捷键/点击切换 | `window.rs` | ⚠️ 需完善 |
| 重命名工作区 | 双击侧边栏重命名 | `workspace_sidebar.rs` | ❌ 需实现 |
| 工作区拖拽排序 | 侧边栏拖拽重排 | `workspace_sidebar.rs` | ❌ 需实现 |
| 工作区固定 | 收藏/固定到顶部 | `workspace_sidebar.rs` | ❌ 需实现 |
| 最近工作区 | 快速访问历史 | `session/model.rs` | ✅ 已实现 |

**关键数据结构**:
```rust
// workspace/model.rs
pub struct WorkspaceState {
    pub id: WorkspaceId,
    pub name: String,
    pub target: WorkspaceTarget,
    pub layout: LayoutNode,
    pub active_pane_id: PaneId,
    pub env_profile_id: Option<String>,
    pub recent_files: Vec<String>,
}
```

#### 1.2 Pane 布局

**目标**: 实现完整的分屏布局系统

| 功能 | 描述 | 实现位置 | 状态 |
|------|------|----------|------|
| 水平分割 | 左右分屏 | `layout/ops.rs` | ✅ 已实现 |
| 垂直分割 | 上下分屏 | `layout/ops.rs` | ✅ 已实现 |
| 关闭 Pane | 关闭当前面板 | `layout/ops.rs` | ✅ 已实现 |
| 焦点移动 | 方向键切换焦点 | `layout/ops.rs` | ✅ 已实现 |
| 比例调整 | 鼠标拖拽调整 | `layout/model.rs` | ✅ 已实现 |
| 比例重置 | 重置为 50/50 | `layout/model.rs` | ✅ 已实现 |

**关键实现**:
```rust
// layout/ops.rs
pub fn split_pane(
    layout: &mut LayoutNode,
    pane_id: &PaneId,
    axis: SplitAxis,
    split_id: impl Into<String>,
    new_pane: PaneNode,
) -> bool

pub fn close_pane(
    layout: &mut LayoutNode,
    pane_id: &PaneId,
) -> ClosePaneOutcome

pub fn focus_pane_in_direction(
    layout: &LayoutNode,
    current: &PaneId,
    direction: Direction,
) -> Option<PaneId>
```

#### 1.3 Tab 管理

**目标**: 实现标签页的完整生命周期

| 功能 | 描述 | 实现位置 | 状态 |
|------|------|----------|------|
| 新建标签 | 创建新终端/Agent/文件标签 | `layout/ops.rs` | ✅ 已实现 |
| 关闭标签 | 关闭当前标签 | `layout/ops.rs` | ✅ 已实现 |
| 切换标签 | 点击/快捷键切换 | `workspace/ops.rs` | ✅ 已实现 |
| 标签拖拽 | 跨 Pane 移动标签 | `commands.rs` | ⚠️ 需完善 |
| 固定标签 | 钉钉标签到左侧 | `workspace/ops.rs` | ✅ 已实现 |
| 重命名标签 | 双击标签重命名 | `workspace/ops.rs` | ✅ 已实现 |

---

### Phase 2: 终端系统 (P0)

#### 2.1 PTY 后端

**目标**: 实现跨平台的伪终端支持

| 功能 | Windows | Linux | macOS |
|------|---------|-------|-------|
| 创建会话 | ConPTY | posix_openpt | posix_openpt |
| 写入输入 | WriteConsoleInput | write | write |
| 读取输出 | ReadConsoleOutput | read | read |
| 调整大小 | SetConsoleScreenBufferSize | resize | resize |
| 信号处理 | GenerateConsoleCtrlEvent | kill/tcsetpgrp | kill/tcsetpgrp |

**关键接口**:
```rust
// platform/terminal/backend.rs
pub trait TerminalBackend: Send + Sync {
    fn create_session(&self, profile: TerminalLaunchProfile) -> Result<TerminalSessionId, String>;
    fn write_input(&self, id: &TerminalSessionId, data: &[u8]) -> Result<(), String>;
    fn resize(&self, id: &TerminalSessionId, cols: u16, rows: u16) -> Result<(), String>;
    fn kill(&self, id: &TerminalSessionId) -> Result<(), String>;
    fn metadata(&self, id: &TerminalSessionId) -> Result<TerminalSessionMetadata, String>;
}
```

**实现状态**:
- Windows: `platform/src/windows/conpty.rs` - ⚠️ 需完善
- Linux: `platform/src/unix/mod.rs` - ⚠️ 需完善
- macOS: `platform/src/unix/mod.rs` - ⚠️ 需完善

#### 2.2 终端渲染

**目标**: 实现高性能的终端渲染

| 功能 | 描述 | 状态 |
|------|------|------|
| 字符渲染 | 绘制字符网格 | ⚠️ 基础实现 |
| 光标渲染 | 光标形状/闪烁 | ❌ 需实现 |
| 颜色支持 | 256色/TrueColor | ⚠️ 需完善 |
| 字体渲染 | 等宽字体/字距 | ❌ 需实现 |
| 滚动支持 | 回滚历史 | ⚠️ 基础实现 |
| 选中复制 | 选中文本复制 | ❌ 需实现 |
| 粘贴 | 粘贴文本 | ⚠️ 基础实现 |
| 链接检测 | 检测并点击 URL | ❌ 需实现 |

#### 2.3 Shell 集成

| 功能 | 描述 | 状态 |
|------|------|------|
| WSL 集成 | 检测/启动 WSL Shell | ⚠️ 需完善 |
| Shell 检测 | 自动检测默认 Shell | ⚠️ 需完善 |
| OSC 7 | 工作目录同步 | ❌ 需实现 |
| OSC 0/2 | 终端标题同步 | ❌ 需实现 |

---

### Phase 3: AI Agent 集成 (P0)

#### 3.1 Agent 探测与启动

| 功能 | 描述 | 状态 |
|------|------|------|
| 自动探测 | 检测已安装的 AI CLI | ✅ 已实现 |
| 支持列表 | Claude Code / Codex / OpenCode / Aider | ✅ 已实现 |
| 一键启动 | 点击启动 Agent | ✅ 已实现 |
| Attached 模式 | 附加到终端 | ✅ 已实现 |
| Managed 模式 | 独立进程管理 | ⚠️ 需完善 |

#### 3.2 Agent 表面状态

```rust
// surface/agent.rs
pub struct AgentSurfaceState {
    pub surface_id: SurfaceId,
    pub provider_id: String,
    pub session_id: Option<TerminalSessionId>,
    pub status: AgentStatus,
    pub history: Vec<AgentMessage>,
}
```

---

### Phase 4: 文件与编辑 (P1)

#### 4.1 文件树

| 功能 | 描述 | 状态 |
|------|------|------|
| 目录浏览 | 树形展示目录 | ✅ 已实现 |
| 文件过滤 | 过滤隐藏文件/模式 | ✅ 已实现 |
| 文件操作 | 新建/删除/重命名 | ✅ 已实现 |
| WSL 文件 | 跨 WSL 浏览 | ✅ 已实现 |

#### 4.2 编辑器

| 功能 | 描述 | 状态 |
|------|------|------|
| 文本编辑 | 基本编辑能力 | ✅ 已实现 |
| 语法高亮 | 语言识别高亮 | ⚠️ 需完善 |
| 保存文件 | 写回文件系统 | ✅ 已实现 |
| 多文件 | 多标签编辑 | ✅ 已实现 |

#### 4.3 预览

| 功能 | 描述 | 状态 |
|------|------|------|
| Markdown | 渲染 Markdown | ✅ 已实现 |
| 图片 | 图片预览 | ⚠️ 需完善 |
| PDF | PDF 预览 | ❌ 暂不实现 |

---

### Phase 5: UI 交互 (P1)

#### 5.1 快捷键系统

| 快捷键 | 功能 | 状态 |
|--------|------|------|
| Ctrl+Shift+N | 新建工作区 | ✅ 已实现 |
| Ctrl+Shift+W | 关闭工作区 | ⚠️ 需完善 |
| Ctrl+D | 水平分割 | ✅ 已实现 |
| Ctrl+Shift+D | 垂直分割 | ✅ 已实现 |
| Ctrl+W | 关闭 Pane/Tab | ✅ 已实现 |
| Ctrl+Tab | 下一个 Tab | ✅ 已实现 |
| Ctrl+Shift+Tab | 上一个 Tab | ✅ 已实现 |
| Ctrl+Arrow | 方向移动焦点 | ✅ 已实现 |
| Ctrl+1-9 | 切换工作区 | ✅ 已实现 |
| Ctrl+M | 切换侧边栏 | ✅ 已实现 |
| Ctrl+T | 新建终端 | ✅ 已实现 |
| Ctrl+K | 清除终端回滚 | ✅ 已实现 |
| Ctrl++ | 放大字体 | ✅ 已实现 |
| Ctrl+- | 缩小字体 | ✅ 已实现 |
| Ctrl+0 | 重置字体 | ✅ 已实现 |
| F11 | 全屏切换 | ⚠️ 需完善 |
| Ctrl+Q | 退出应用 | ✅ 已实现 |

#### 5.2 命令面板

| 功能 | 描述 | 状态 |
|------|------|------|
| 快速打开 | Ctrl+P 打开 | ⚠️ 需完善 |
| 命令过滤 | 模糊搜索命令 | ⚠️ 需完善 |
| 工作区切换 | 切换到指定工作区 | ⚠️ 需完善 |
| Pane 操作 | 分割/关闭命令 | ⚠️ 需完善 |

#### 5.3 上下文菜单

| 功能 | 描述 | 状态 |
|------|------|------|
| 终端菜单 | 复制/粘贴/分割/清除 | ⚠️ 需完善 |
| 标签菜单 | 关闭/关闭其他/固定 | ✅ 已实现 |
| 工作区菜单 | 关闭/重命名/固定 | ⚠️ 需完善 |

---

### Phase 6: 会话与持久化 (P1)

#### 6.1 会话保存

| 功能 | 描述 | 状态 |
|------|------|------|
| 自动保存 | 定时自动保存 | ✅ 已实现 |
| 退出保存 | 退出时保存 | ✅ 已实现 |
| 增量保存 | 只保存变更 | ⚠️ 需完善 |
| 原子保存 | 防止损坏 | ✅ 已实现 |

#### 6.2 会话恢复

| 功能 | 描述 | 状态 |
|------|------|------|
| 恢复布局 | 恢复 Pane/Tab 布局 | ✅ 已实现 |
| 恢复终端 | 恢复终端状态 | ⚠️ 需完善 |
| 恢复工作目录 | 恢复 CWD | ⚠️ 需完善 |

**持久化数据**:
```rust
// session/model.rs
pub struct SessionState {
    pub version: u32,
    pub active_workspace_id: Option<WorkspaceId>,
    pub workspaces: Vec<WorkspaceState>,
    pub recent_workspaces: Vec<RecentWorkspace>,
    pub ui_preferences: UiPreferences,
    pub last_saved: Option<u64>,
}

pub struct UiPreferences {
    pub sidebar_collapsed: bool,
    pub sidebar_width: u32,
    pub font_size: u16,
    pub theme: String,
    pub top_bar_visible: bool,
}
```

---

### Phase 7: 高级功能 (P2)

#### 7.1 内置浏览器

| 功能 | 描述 | 状态 |
|------|------|------|
| WebView 标签 | 嵌入浏览器标签 | ✅ 已实现 |
| 页面导航 | 后退/前进/刷新 | ⚠️ 需完善 |
| 地址栏 | URL 输入 | ⚠️ 需完善 |
| 分屏打开 | 在新 Pane 打开链接 | ⚠️ 需完善 |

#### 7.2 通知系统

| 功能 | 描述 | 状态 |
|------|------|------|
| 系统通知 | 发送系统通知 | ⚠️ 需完善 |
| 未读标记 | 标签页未读红点 | ⚠️ 需完善 |
| 通知中心 | 查看所有通知 | ⚠️ 需完善 |

#### 7.3 查找功能

| 功能 | 描述 | 状态 |
|------|------|------|
| 终端查找 | Ctrl+F 搜索 | ⚠️ 需完善 |
| 高亮匹配 | 匹配内容高亮 | ⚠️ 需完善 |
| 上下查找 | 查找上一个/下一个 | ⚠️ 需完善 |

---

### Phase 8: 外部接口 (P3)

#### 8.1 Unix Socket 控制

| 功能 | 描述 | 状态 |
|------|------|------|
| Socket 服务器 | 监听 Unix Socket | ⚠️ 需完善 |
| 命令解析 | 解析 JSON 命令 | ⚠️ 需完善 |
| 响应返回 | 返回执行结果 | ⚠️ 需完善 |
| CLI 客户端 | limux-cli 对等客户端 | ⚠️ 需完善 |

---

## 4. UI/UX 交互流程

### 4.1 启动流程

```
1. 应用启动
   ├── 加载 session.json
   │   ├── 存在 → 恢复工作区布局
   │   └── 不存在 → 创建默认工作区
   ├── 初始化 PTY 后端
   ├── 创建主窗口
   ├── 显示侧边栏
   └── 聚焦第一个终端
```

### 4.2 工作区创建流程

```
1. Ctrl+Shift+N 或点击 "+"
   └── 打开文件夹选择器
       └── 选择文件夹
           ├── 创建 WorkspaceState
           ├── 初始化默认 Pane + Terminal
           ├── 添加到侧边栏
           └── 聚焦新工作区
```

### 4.3 Pane 分割流程

```
1. Ctrl+D (水平) 或 Ctrl+Shift+D (垂直)
   ├── 获取当前聚焦 Pane
   ├── 创建新 PaneNode + TerminalTab
   ├── 修改 LayoutNode 为 SplitNode
   ├── 设置 ratio = 0.5
   ├── 聚焦新 Pane
   └── 触发保存
```

### 4.4 Tab 拖拽流程

```
1. 开始拖拽 Tab
   ├── 设置 tab_dragging = true
   ├── 记录拖拽数据 (pane_id, tab_id)
   └── 显示放置指示器

2. 拖拽到目标 Pane
   ├── 检测放置区域 (左/右/上/下)
   └── 预览放置效果

3. 释放
   ├── 从原 Pane 移除 Tab
   ├── 添加到目标 Pane
   ├── 更新 active_tab_id
   └── 清理拖拽状态
```

---

## 5. 验收标准

### 5.1 功能验收

| 功能 | 验收条件 |
|------|----------|
| 工作区管理 | 可创建/关闭/切换/重命名/拖拽排序 |
| Pane 布局 | 可分割/关闭/方向键移动/比例调整 |
| Tab 管理 | 可创建/关闭/切换/拖拽/固定 |
| 终端 | 三个平台均可正常输入输出 |
| Agent | 可探测/启动常见 AI CLI |
| 文件树 | 可浏览目录和打开文件 |
| 编辑器 | 可编辑并保存文件 |
| 会话 | 退出重启后恢复完整状态 |
| 快捷键 | 所有 P0 快捷键正常工作 |

### 5.2 平台验收

| 平台 | 最低要求 |
|------|----------|
| Windows | ConPTY 正常工作，WSL 集成正常 |
| Linux | PTY 正常工作，bash/zsh 正常 |
| macOS | PTY 正常工作，bash/zsh/fish 正常 |

### 5.3 性能验收

| 指标 | 目标 |
|------|------|
| 启动时间 | < 2 秒 |
| 终端输入延迟 | < 10ms |
| 内存占用 | < 200MB (空闲) |
| 分割操作 | < 50ms |

---

## 6. 实施步骤

### Step 1: 完善核心布局 (1 周)

- [ ] 实现 Pane 方向键焦点移动
- [ ] 实现 Pane 关闭逻辑
- [ ] 实现 Tab 切换和关闭
- [ ] 实现分割比例拖拽调整

### Step 2: 完善终端后端 (1 周)

- [ ] 完成 Windows ConPTY 实现
- [ ] 完成 Linux/macOS PTY 实现
- [ ] 实现终端渲染 (光标/颜色/选中)
- [ ] 实现复制粘贴

### Step 3: 完善快捷键和菜单 (1 周)

- [ ] 实现所有 P0 快捷键
- [ ] 实现右键上下文菜单
- [ ] 实现命令面板

### Step 4: 完善文件功能 (1 周)

- [ ] 完善文件树操作
- [ ] 完善编辑器
- [ ] 实现 Markdown 预览

### Step 5: 会话持久化 (1 周)

- [ ] 完善自动保存
- [ ] 实现完整状态恢复
- [ ] 实现原子保存

### Step 6: 高级功能 (1 周)

- [ ] 实现通知系统
- [ ] 实现查找功能
- [ ] 实现外部控制接口

### Step 7: 测试和优化 (1 周)

- [ ] 跨平台测试
- [ ] 性能优化
- [ ] Bug 修复

---

## 7. 技术风险与对策

| 风险 | 影响 | 对策 |
|------|------|------|
| ConPTY 复杂性 | 高 | 参考 vscode/terminal 实现 |
| GPUI 跨平台 | 中 | 使用 gpui-component |
| PTY 兼容性 | 中 | 使用 portable-pty crate |
| WSL 文件系统 | 中 | 使用 wsl-api crate |
| 性能瓶颈 | 中 | 使用增量渲染 |

---

## 8. 参考资料

- limux 源码: `third_party/limux/rust/limux-host-linux/`
- limux 快捷键: `third_party/limux/shortcut-remap-plan.md`
- GPUI 文档: `third_party/gpui-component/`
- portable-pty: https://github.com/wez/portable-pty
- ConPTY: https://docs.microsoft.com/en-us/windows/console/console-virtual-terminal-sequences
