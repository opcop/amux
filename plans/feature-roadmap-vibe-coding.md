# Amux 功能优化与改进路线图

> 从 Vibe Coding 开发者视角出发，梳理 Amux 需要优化和新增的功能。
> 创建日期：2026-04-01

---

## 一、现有功能优化（体验打磨）

### 1. Windows Live CWD 获取

**痛点**：Windows 上 split pane 无法获取终端当前工作目录，只能用 spawn-time 的初始目录。用户 `cd` 后 split 出来的 pane 还在旧目录。

**现状**：Linux/Mac 通过 `/proc/PID/cwd` 已解决。Windows 上 `/proc` 不存在，标题解析方案已验证不可靠（不同 shell 标题格式各异）。

**方案**：
- 通过 Windows API `NtQueryInformationProcess` 读取子进程 PEB 中的 `CurrentDirectory`
- 前提：在 `AlacrittyTerminal::new()` 中获取并保存 ConPTY 子进程 PID
- 封装为 `current_cwd()` 的 Windows 实现，和 Linux 的 `/proc` 方案对齐

**优先级**：高
**复杂度**：中（需要 unsafe Windows API 调用）

---

### 2. 终端渲染质量

**痛点**：AI 工具（Claude Code、Aider）输出包含大量 ANSI 颜色、markdown 格式、进度条、spinner。渲染不精确会严重影响可读性和日常使用体验。

**需要打磨的方面**：
- 字体渲染清晰度（尤其 CJK 字符、等宽对齐）
- 滚动流畅度（大量输出时不卡顿）
- 选区复制准确性（跨行选择、方块选择）
- 下划线样式（single/double/curly/dotted/dashed 在 Alacritty 中的完整支持）
- 光标闪烁和形状（Block/Beam/Underline 的正确渲染）

**优先级**：高
**复杂度**：中-高（涉及 GPUI canvas 渲染细节）

---

### 3. 启动速度优化

**痛点**：恢复 layouts.json + 检测工具（遍历 PATH + WSL） + spawn 所有 PTY，workspace 多时启动明显慢。

**优化方案**：
- **懒加载终端**：只 spawn 当前可见 workspace 的终端，其他 workspace 延迟到切换时再创建
- **后台工具检测**：vibe tools 检测移到后台线程，不阻塞首屏渲染
- **增量恢复**：layout 结构先恢复（毫秒级），PTY 进程按需创建
- **缓存检测结果**：工具检测结果缓存到磁盘，启动时只验证是否过期

**优先级**：高
**复杂度**：中

---

## 二、核心新功能（差异化价值）

### 4. AI Agent 状态感知 ⭐ 杀手级功能

**痛点**：同时运行多个 AI agent 时，不知道哪个在忙、哪个在等输入、哪个已完成。频繁切换 tab 检查状态，打断工作流。

**功能设计**：

#### 4.1 Tab 级状态指示
- 通过分析终端输出（正则匹配 agent 特征）自动检测 agent 状态
- Tab 标题显示状态标记：`Claude [thinking...]`、`Claude [waiting]`、`Claude [done ✓]`
- 状态颜色编码：🟡 思考中、🟢 等待输入、✅ 完成、🔴 出错

#### 4.2 全局 Agent Dashboard
- 侧边栏或状态栏显示所有运行中的 agent 概览
- 显示：agent 名称、运行时长、当前状态
- 点击直接跳转到对应 pane

#### 4.3 通知系统
- Agent 完成任务时：tab 闪烁 + 可选系统通知（Windows Toast / Linux notify-send）
- Agent 出错时：tab 变红 + 声音提示（可配置）
- 非活跃 workspace 有 agent 完成时，workspace 标签也显示提示

**实现思路**：
- 每个已知 agent（claude、aider、codex 等）定义一组正则模式来识别状态
- 在 `poll_activity()` 中检查终端输出，匹配模式后更新 tab 状态
- 状态变化时触发通知

**优先级**：最高（这是 Amux 区别于普通终端复用器的核心价值）
**复杂度**：中

---

### 5. 跨 Pane 文本流转

**痛点**：AI agent 生成代码后，需要手动复制粘贴到另一个 pane 测试。操作繁琐，打断思路。

**功能设计**：
- `Send to Pane` 快捷键：选中文本 → 按快捷键 → 选择目标 pane → 文本作为输入发送到目标终端
- `Send Last Block`：自动识别最近一个代码块（通过 ``` 围栏或缩进检测），发送到指定 pane
- Command Palette 集成：`send selection to pane 2`
- 可选：发送时自动加 bracketed paste 包裹

**优先级**：中-高
**复杂度**：低-中

---

### 6. 智能 Layout 模板

**痛点**：每次新项目都要手动创建相同的 pane 布局（AI + Test + Git），重复操作。

**功能设计**：

#### 6.1 内置模板
- `AI + Shell`：左 70% AI agent，右 30% shell
- `AI + Test + Git`：左 AI agent，右上 test runner，右下 git log
- `Multi-Agent`：左右两个 AI agent + 底部 shell
- `Full Stack`：前端 + 后端 + 数据库 + 日志 四格

#### 6.2 自定义模板
- 当前布局保存为模板：`Save Layout as Template`
- 模板包含：pane 布局 + 每个 pane 的启动命令
- 模板存储在 `~/.amux/templates/`

#### 6.3 项目绑定
- 目录 → 模板映射：打开 `/project/foo` 自动使用 `AI + Test` 模板
- 配置在 `~/.amux/config.toml` 或项目根目录 `.amux.toml`

**优先级**：中
**复杂度**：低-中（基础设施大部分已有）

---

### 7. 终端输出搜索增强

**痛点**：AI agent 输出几百行后需要回溯查找特定内容，当前搜索功能较基础。

**功能设计**：
- 搜索结果高亮持久化（不仅跳转，还标记所有匹配）
- 正则表达式搜索支持
- 搜索结果计数显示（3/15 matches）
- 按时间范围过滤（「最近 5 分钟」「最近 100 行」）
- 搜索历史（最近 10 条搜索词）
- 跨 pane 全局搜索（在所有 pane 的输出中搜索）

**优先级**：中
**复杂度**：中

---

### 8. 截图粘贴完善

**痛点**：给 AI agent 发送截图是 Vibe Coding 的高频操作。当前 smart paste 方向正确但需打磨。

**功能设计**：
- 粘贴图片前弹出预览确认（小窗显示图片缩略图 + 目标路径）
- 支持拖拽图片文件到终端 pane
- 自动识别当前 agent 的图片参数格式：
  - Claude Code：直接粘贴 WSL 路径
  - Aider：`/image <path>`
  - 其他：可配置
- 图片自动压缩（如果超过 agent 限制）

**优先级**：低-中
**复杂度**：低

---

## 三、长期差异化方向

### 9. Session 录制与回放

**价值**：记录完整的 Vibe Coding 会话用于分享、教学、复盘。

**功能设计**：
- 录制：所有 pane 的输入输出 + 时间戳 + 布局变化
- 回放：按原始时间线或加速回放，支持暂停/跳转
- 导出：生成 asciicast 格式（兼容 asciinema）或 HTML 回放页面
- 分析：统计操作频率、agent 使用时长、常用命令

**优先级**：低
**复杂度**：高

---

### 10. 多 Agent 编排

**价值**：从终端复用器进化为 AI 开发工作流控制台。

**功能设计**：
- 定义 agent 工作流：A 写代码 → B 写测试 → C review
- 自动触发：A 的 pane 检测到「完成」状态后，自动在 B 的 pane 发送命令
- 工作流可视化：状态流转图，显示每个 agent 的进度
- 工作流模板：可保存和复用常见的 agent 协作模式

**优先级**：低（远期愿景）
**复杂度**：很高

---

## 优先级总览

| 阶段 | 功能 | 优先级 | 价值 |
|------|------|--------|------|
| **第一阶段：基础体验** | 终端渲染打磨 (#2) | 🔴 高 | 日常使用的基础门槛 |
| | 启动速度优化 (#3) | 🔴 高 | 第一印象 |
| | Windows Live CWD (#1) | 🔴 高 | split 核心功能完整性 |
| **第二阶段：差异化** | Agent 状态感知 (#4) | 🔴 最高 | 让用户从其他终端迁移的核心理由 |
| | 跨 Pane 文本流转 (#5) | 🟡 中高 | 提升 AI 协作效率 |
| | 智能 Layout 模板 (#6) | 🟡 中 | 减少重复操作 |
| **第三阶段：深度打磨** | 搜索增强 (#7) | 🟡 中 | 回溯查找效率 |
| | 截图粘贴完善 (#8) | 🟢 低中 | 多模态 AI 交互 |
| | 内置文件预览 (#11) | 🟡 中高 | 终端内预览 Markdown/图片/代码，不离开 Amux |
| | 终端路径点击预览 (#14) | 🟡 中 | 点击终端输出中的文件路径直接预览，Vibe Coding 核心体验 |
| **第三阶段+** | Agent 状态悬浮窗 (#12) | 🟡 中高 | 最小化后不失明，详见 agent-notification-pet.md |
| | 电子宠物系统 (#13) | 🟡 中 | 情感化设计 + 小额付费获客 |
| **第四阶段：远期愿景** | Session 录制 (#9) | 🟢 低 | 分享与教学 |
| | 多 Agent 编排 (#10) | 🟢 低 | 平台化方向（详见 agent-orchestration-product.md） |

---

## 核心判断

> **让用户从 Windows Terminal / tmux 迁移到 Amux 的三个理由：**
>
> 1. **Agent 状态感知** — 只有 Amux 能告诉我哪个 AI 在忙、哪个在等我
> 2. **终端渲染达到生产级** — 基础体验不能比现有工具差
> 3. **一键布局 + CWD 继承** — 每天省下的 5 分钟重复操作累积起来就是迁移动力
