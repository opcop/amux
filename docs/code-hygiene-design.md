# Code Hygiene — 代码卫生自动提醒设计

## 问题背景

Vibe Coding 模式下，AI Agent 写代码速度极快，质量债务积累速度远超传统开发：

```
传统开发：写 100 行 → 自己 review → 提交
Vibe Coding：Agent 写 500 行 → 跑通了 → 提交 → 重复 10 次 → 回头看代码已经一团糟
```

`/simplify` 等代码清理命令是"事后补救"，但人性决定了开发者不会主动想起补救。等想起来时，积累的垃圾代码已经太多，清理成本远大于及时清理。

## 设计方向

### 方向 1：被动提醒（最小侵入）

**状态栏「代码卫生指标」**

在 Amux 状态栏已有信息（workspace 名、pane 数量等）旁，加一个代码卫生指示器：

```
┌────────────────────────────────────────────────────┐
│ myapp │ 3 panes │ 2 tabs │ pwsh  │ ●◌◌ 12 files  │
│                                    ↑               │
│                           绿=干净 黄=该清理 红=紧急 │
└────────────────────────────────────────────────────┘
```

**追踪维度：**
- 距离上次 simplify/review 过了多少次 commit
- 未 review 的新增/修改行数（通过 `git diff --stat` 统计）
- 时间衰减（2 小时没清理 → 黄色，半天 → 红色）

**交互：**
- 点击指示器 → 弹出详情："已有 847 行新代码未 review，建议在 Claude 里运行 /simplify"
- 指示器颜色变化本身就是视觉提醒，不需要弹窗打断工作流

**实现要点：**
- 在 60fps 定时器里低频（每 30 秒）检查 git 状态
- 记录上次 simplify 的时间戳到 `~/.amux/hygiene_state.json`
- 状态栏渲染增加卫生指标组件

### 方向 2：主动触发（自动化）

**Git Commit 计数提醒**

Amux 检测到终端内执行 `git commit` 时自动计数。每 N 次 commit 后弹 toast：

```
"已提交 5 次（+432 行），考虑运行 /simplify 清理一下？"
[稍后提醒] [跳过] [立即执行]
```

**交互：**
- "立即执行" → Amux 在当前 agent pane 自动输入 `/simplify`
- "稍后提醒" → 再过 2 次 commit 后再次提醒
- "跳过" → 本次 session 不再提醒（直到下次启动）

**实现要点：**
- 在 `poll_activity` 中检测终端输出是否包含 git commit 相关特征
- 或者用 `fs::watch` 监听 `.git/refs/heads/` 变化（更可靠）
- 计数器存在 workspace 级别的状态里

### 方向 3：Guardian Agent（Agent Bridge 集成）

利用 Agent Bridge，Amux 支持一个**后台守卫 Agent**：

```
Pane 1: Claude (写功能)
Pane 2: Aider (写测试)
Pane 3: [Guardian] (静默监控，自动 review)
```

**Guardian Agent 的行为：**
- 监听 git log，发现新 commit 后自动 `git diff` 分析
- 发现代码异味（重复代码、硬编码、过长函数）时，通过 Bridge 通知开发 agent
- 可在检测到问题后自动在目标 pane 运行 simplify

**通知示例：**
```
[amux-bridge workspace:myapp pane:guardian agent:guardian] 
检测到 src/api.rs 有 3 处重复模式和 2 个未使用的变量，建议运行 /simplify
```

**配置选项：**
- 敏感度级别：aggressive / balanced / relaxed
- 监控范围：所有文件 / 指定目录 / 排除测试文件
- 通知方式：Bridge 消息 / Toast / 静默记录

**差异化价值：**
这是 Amux 独有的能力——Prowl 没有 Agent Bridge，无法实现 agent 间的自动化质量监控。竞品完全做不到这一点。

### 方向 4：Workspace 级别质量策略（配置化）

在 `~/.amux/config.toml` 或 per-workspace 设置里添加：

```toml
[code_hygiene]
# 启用代码卫生提醒
enabled = true

# 提醒触发条件（满足任一即提醒）
remind_after_commits = 5        # 每 5 次 commit 提醒
remind_after_lines = 500        # 每 500 行新增提醒
remind_after_minutes = 120      # 每 2 小时提醒

# 提醒方式
notification = "toast"          # "toast" / "statusbar" / "sound" / "all"

# 自动操作
auto_simplify = false           # 是否在阈值后自动触发 simplify
guardian_agent = false           # 是否启用 guardian agent 模式

# 忽略规则
ignore_paths = ["*.md", "docs/", "*.lock"]
```

**价值：** 不同项目有不同的质量要求。个人项目可以 relaxed，团队项目 aggressive。

### 方向 5：Session 收尾仪式

每次关闭 workspace 或退出 Amux 前，弹一个**收尾检查**：

```
┌─────────────────────────────────────────┐
│  Session Summary                         │
│                                          │
│  本次 session：                          │
│  - 7 次 commit                          │
│  - +1,243 行 / -89 行                   │
│  - 上次 simplify：3 小时前              │
│                                          │
│  建议在关闭前运行 /simplify             │
│                                          │
│  [运行 Simplify]  [跳过并关闭]           │
└─────────────────────────────────────────┘
```

**价值：** 形成"写代码 → 清理 → 关闭"的习惯闭环。类似 IDE 在退出前提示保存未保存的文件。

## 实现建议

### 短期（最小改动，立即见效）

**方向 1 + 2 的组合：**
- 状态栏加代码卫生指标（追踪 commit 次数 + 变更行数）
- 达到阈值时弹 toast 提醒
- 工作量小，不需要新架构
- 预计 1-2 天开发

### 中期（差异化能力）

**方向 3：Guardian Agent**
- 利用已有的 Agent Bridge 基础设施
- 这是 Amux 独有的能力，竞品做不到
- 前置依赖：CLI 独立二进制（Agent Bridge Phase 1.4）
- 预计 3-5 天开发

### 长期（产品化）

**方向 4 + 5：**
- 可配置的质量策略
- Session 收尾仪式
- 与 CI/CD 集成（推送前自动检查）
- 预计 5-7 天开发

## 与竞品的差异

| 能力 | Prowl | tmux/WezTerm | VS Code | Amux (目标) |
|------|-------|-------------|---------|-------------|
| 代码卫生提醒 | 无 | 无 | 扩展可做 | 内置 |
| Agent 自动 review | 不可能 | 不可能 | 扩展可做 | Agent Bridge 原生支持 |
| 质量策略配置 | 无 | 无 | settings.json | config.toml |
| Session 收尾检查 | 无 | 无 | 无 | 内置 |

**核心优势：** Amux 是唯一一个把代码卫生提醒深度集成到终端多路复用器里的产品。其他工具要么是 IDE 插件（依赖特定 IDE），要么根本没有这个概念。对于 Vibe Coding 开发者来说，终端就是 IDE——在终端里解决质量问题是最自然的体验。
