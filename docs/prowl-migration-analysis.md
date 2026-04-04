# Prowl → Amux 迁移分析

站在 Prowl 忠实用户的角度，分析 Amux 需要做什么才能让用户彻底迁移且不再回头。

## Amux 已经超越 Prowl 的地方

| 能力 | Prowl | Amux | 优势方 |
|------|-------|------|--------|
| 跨平台 | macOS only | Windows + Linux | **Amux** |
| Agent 间通信 | 无（CLI 只是控制终端） | Bridge 协议（发现/观察/通信/自识别） | **Amux** |
| Agent 状态检测 | 简单的 running/idle | 6 种 Agent + 4 种状态 + 自动检测 | **Amux** |
| Agent 协作教学 | 无 | `amux pane teach` + agent-prompt.md | **Amux** |
| 浏览器集成 | 无 | WebView2 作为 pane tab | **Amux** |
| 文件预览 | 无 | Markdown + 14 语言语法高亮预览 tab | **Amux** |
| 依赖复杂度 | Zig + Ghostty + Sparkle + PostHog + Sentry | 纯 Rust，零运行时依赖 | **Amux** |
| Tab 类型系统 | 终端 only | Terminal / Browser / Preview（可扩展） | **Amux** |

## Prowl 有而 Amux 缺失的功能

### P0 — 缺了就不可能迁移

#### 1. Git Worktree 管理

Prowl 的核心卖点。一键创建 worktree/分支，agent 可以在不同 worktree 间切换而不丢失终端上下文。

Amux 的 workspace 只是终端分组，完全没有 git 集成。

**Prowl 的能力：**
- 一键创建 worktree + 分支（带 base ref 选择）
- Worktree 归档/恢复/删除
- 侧边栏按仓库分组显示所有 worktree
- 切换 worktree 保留终端状态
- `PROWL_WORKTREE_PATH` / `PROWL_ROOT_PATH` 环境变量注入
- Setup script 自动执行（新 worktree 创建后）

**迁移阻力**：做 Vibe Coding 必须频繁切分支。没有 worktree 管理 = 每次切分支都要手动操作 git。

#### 2. 稳定的 CLI 机器接口（JSON Contracts）

Prowl 的 CLI 有版本化的 JSON 合约（v1），agent 可以可靠地脚本化操作：

```bash
prowl list --json    # 结构化输出，合约锁定
prowl send pane-1 "text"  # 机器可读的成功/失败
prowl read pane-1 --json  # 返回内容 + 光标位置
```

Amux 的 `amux pane` 命令输出到 temp file + cat，不够稳定，没有 JSON 模式，没有错误码。

**迁移阻力**：Agent 需要机器可读的输出来自动化工作流。当前 Amux 的输出方式对 agent 不友好。

#### 3. 自动更新

Prowl 有 Sparkle 自动更新（后台检查 + 自动下载 + 一键安装）。

Amux 没有任何更新机制，用户需要 `git pull && cargo build`。

**迁移阻力**：Vibe Coding 开发者不想每次都手动编译。需要至少一个发布渠道（GitHub Releases + 自动检查）。

### P1 — 缺了体验明显不如 Prowl

#### 4. 可自定义快捷键

Prowl 有完整的快捷键重映射系统：
- ~50 个内置命令可重新绑定
- 冲突检测 + 级联重置
- 录制模式（按下快捷键自动识别）
- 每仓库可覆盖

Amux 的快捷键全部硬编码在 `gpui_input_handler.rs` 里，用户无法自定义。

#### 5. 每仓库自定义命令（Custom Commands）

Prowl 允许用户给每个 repo 配置工具栏按钮：
- "运行测试"、"启动 dev server" 等常用操作
- 两种执行模式：新 tab 运行 / 在当前终端输入
- 工具栏前 3 个按钮直接显示，多余的放溢出菜单
- 支持快捷键绑定

Amux 没有这个能力。用户每次都要手动输入命令。

#### 6. GitHub PR 集成

Prowl 在侧边栏直接显示：
- PR 状态徽章（open/draft/merged）
- CI 检查结果环形指示器（passing/failing/pending）
- 一键操作：Mark Ready / Merge / Close
- 失败 CI 的日志提取 + 重新运行
- PR 合并后自动归档 worktree

Amux 完全没有 GitHub 集成。

#### 7. Command Palette 增强

Prowl 的命令面板：
- 模糊匹配搜索
- 最近使用排序（recency scoring）
- 多来源聚合：worktree + actions + ghostty commands + custom commands + PR actions
- 图标 + 快捷键提示

Amux 的命令面板功能较弱，只有基础的命令列表。

#### 8. Canvas（全局终端概览）

Prowl 独特的 Canvas 视图：
- 同时看到所有 worktree 的所有终端（类似仪表盘）
- 可拖拽、可缩放的卡片布局
- 多选 + 批量操作（同步分屏/缩放）
- 双击卡片标题栏直接跳转

Amux 的 Agents 侧边栏是文字列表，没有视觉化的终端概览。

### P2 — 锦上添花

#### 9. Diff 查看器

Prowl 可以看 worktree 的 git diff：
- 独立窗口，侧边栏列出变更文件
- Split（左右对比）和 Unified 两种模式
- Amux 完全没有 diff 功能。

#### 10. 通知系统增强

Prowl 的通知：
- 命令完成通知（可配置阈值，默认 10 秒）
- 系统通知中心集成
- 声音提示
- 通知后自动将 worktree 移到侧边栏顶部

Amux 只有简单的 toast。

#### 11. 设置 UI 完善

Prowl 有完整的设置界面：
- 外观（暗/亮/自动）
- 通知（应用内/系统/声音 各独立开关）
- 更新（频道选择 + 自动检查间隔）
- 行为（退出确认 + 分析 + 崩溃报告）
- 编辑器选择（VS Code / Terminal / Finder 等）

Amux 只有 `~/.amux/config.toml` 基础配置（字体、主题、滚动行数）。

#### 12. 终端布局持久化增强

Prowl 的布局保存/恢复：
- Tab + Split 结构完整保存
- 启动时自动恢复上次布局
- 错误恢复（损坏布局触发重置 + 用户通知）

Amux 有基础的布局保存，但浏览器/预览 tab 的持久化还不完善。

## 让用户无法回头的「决胜点」

### 1. Agent Bridge 是杀手锏

Prowl 的 CLI 只能"控制"终端（发送输入、读取输出），但 agent 之间不能对话。Amux 的 Bridge 让 agent 互相发现、沟通、协作——这是 Prowl 根本做不到的。

**但目前 Bridge 还太初级**，需要增强：
- `amux pane list --json` JSON 输出模式
- 独立的 `amux-cli` 二进制（通过 Named Pipe / Unix Socket 与 GUI 通信）
- 标准化错误码
- 消息送达确认机制

### 2. 跨平台覆盖所有开发者

Prowl 锁死 macOS。Amux 在 Windows + Linux 上工作。如果 Amux 质量足够高，即使 Mac 用户也会迁移（因为团队协作需要统一工具）。

### 3. 浏览器 + 预览 tab 构成开发闭环

Prowl 没有内嵌浏览器。开发 Web 应用时，Amux 可以在同一个窗口里写代码、运行 agent、预览页面、查看文档。这个闭环 Prowl 做不到。

### 4. 可扩展的 Tab 类型系统

Amux 的 `TabKind` 枚举可以无限扩展。未来可以加：
- Git Diff tab（替代独立 diff 窗口）
- Log Viewer tab（结构化日志查看）
- Database tab（数据库查询）
- API Test tab（类似 Postman）

Prowl 的 tab 只能是终端，没有扩展性。

## 建议的开发优先级

| 优先级 | 功能 | 理由 | 工作量 |
|--------|------|------|--------|
| **第一** | Git worktree 管理 | Prowl 用户的核心依赖，没有就不迁移 | 大 |
| **第二** | CLI 独立二进制 + JSON 输出 | Agent 自动化的基础设施 | 中 |
| **第三** | 可自定义快捷键系统 | 每个用户习惯不同，硬编码无法满足 | 中 |
| **第四** | 自动更新机制 | 降低用户使用门槛 | 中 |
| **第五** | 每仓库自定义命令 | 高频使用的效率工具 | 小 |
| **第六** | GitHub PR 集成 | 开发工作流闭环 | 大 |
| **第七** | Command Palette 增强 | 效率提升 | 小 |
| **第八** | Canvas 全局概览 | 差异化体验 | 大 |

## 迁移决策线

```
做完第一和第二 → Amux 具备迁移的基本条件
做完前四个     → 大多数 Prowl 用户会认真考虑迁移
做完前六个     → 我再也不想打开 Prowl
全部完成       → Amux 成为 Vibe Coding 的事实标准
```
