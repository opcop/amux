# Amux Agent Orchestration — 产品方案与商业模式

> 创建日期: 2026-04-02
> 状态: 方案探讨阶段，尚未进入开发

---

## 1. 核心洞察

现有 AI Coding Agent（Claude Code、Codex CLI、OpenCode、Aider 等）都是**孤岛**：

- 各自独立运行，互不感知
- 用户手动切换、复制粘贴、重复解释需求
- 没有跨 Agent 的上下文共享或结果对比

**Amux 的独特位置**：作为终端复用器，它是唯一能同时"看到"所有 Agent 输入输出的那一层。这是基础设施级的卡位优势。

---

## 2. 三个产品方向

### 方向 A：Agent 编排层（最有壁垒）

用户下一个需求，Amux 自动拆解为多步任务，分配给不同 Agent 依次执行：

- 用户："重构用户认证模块，改用 JWT"
- Amux 拆解：Claude 做架构设计 → Codex 写实现 → Claude 写测试
- 每个 Agent 的输出自动成为下一个的输入
- 用户只看到一个统一的进度流

**价值**：用户买的是"完成一件事"，不是"调用一个 API"。

### 方向 B：共享上下文总线（最省钱）

所有 Agent 重复做的事：读 codebase、理解项目结构、分析依赖。Amux 维护一份项目理解缓存：

- Agent A 已读过的文件，Agent B 不需要重复花 token
- 减少 50-70% 的重复 token 消耗
- 团队成员间也可共享缓存

**价值**："用 Amux 跑 3 个 Agent，比单独跑便宜 60%"。

### 方向 C：Agent 效果对比竞技场（最容易落地）

同一个任务同时发给 2-3 个 Agent，让用户对比结果选最好的：

- 左 pane Claude 的方案，右 pane Codex 的方案
- 一键采纳某个方案，或合并两者优点
- 积累数据：哪个 Agent 在什么类型任务上更强

**价值**：用户不用自己试错，Amux 帮你选最好的 Agent。

### 建议执行顺序

先做 C（竞技场/Compare）来获取用户和数据 → 用数据指导 A（编排）的设计 → B（共享上下文）作为基础设施贯穿始终。

---

## 3. UI 交互设计

### 3.1 整体布局演进

在现有布局基础上增加 **Task Bar** 层：

```
[ Sidebar ] [ ====== Task Bar ====== ]    ← 新增，平时折叠为一行
            [ Pane  |  Pane          ]
            [ Pane                   ]
            [ ───── Status Bar ───── ]
```

Task Bar 是核心交互层，平时折叠为一行高度（~28px），展开时占顶部 ~120px。

### 3.2 用户操作流程

#### Step 1：发起任务

用户按 `Ctrl+Shift+T` 或点击 Task Bar，弹出任务输入面板：

```
┌─────────────────────────────────────────────┐
│ 🔍 重构用户认证模块，改用 JWT              │
│                                             │
│  Strategy:  ○ Single Agent (fast)           │
│             ● Compare (2-3 agents)          │
│             ○ Pipeline (sequential)         │
│                                             │
│  Agents:    [✓] Claude  [✓] Codex  [ ] Aider│
│                                             │
│            [ Start Task ]                   │
└─────────────────────────────────────────────┘
```

三种执行策略：

| 策略 | 说明 | 适用场景 |
|------|------|----------|
| Single | 一个 Agent 做完 | 简单任务、快速迭代 |
| Compare | 多个 Agent 同时做，选最好的 | 重要决策、不确定哪个 Agent 更合适 |
| Pipeline | 拆解成步骤依次执行 | 复杂任务、需要多种能力 |

#### Step 2：执行中 — Task Bar 显示进度

折叠态（一行）：

```
┌─ Task: 重构用户认证模块 ──────────────────────┐
│ Claude ████████░░ Editing auth.rs...          │
│ Codex  ██████░░░░ Reading codebase...         │
│                                    [Expand ▼] │
└───────────────────────────────────────────────┘
```

展开后，下方 pane 自动分屏显示各 Agent 的终端：

```
┌─ Task: 重构用户认证模块 ──── [Compare Mode] ──┐
│ Claude ████████░░   │  Codex ██████░░░░       │
└───────────────────────────────────────────────┘
┌─ Claude Code ─────────┬─ Codex CLI ───────────┐
│ > Editing auth.rs     │ > Analyzing deps...   │
│ > Writing tests...    │ > Planning changes... │
│                       │                       │
└───────────────────────┴───────────────────────┘
```

用户可以随时切到某个 Agent 的 pane 里直接交互（补充需求），也可以完全不管让它们自动跑。

#### Step 3：结果对比 — Diff View

Compare 模式完成后，Task Bar 变成对比视图：

```
┌─ Task Complete ─ Pick a winner ───────────────┐
│                                               │
│  Claude (3 files, +120 -45)  ★ Recommended    │
│  ├ auth.rs      [View Diff]                   │
│  ├ middleware.rs [View Diff]                   │
│  └ test_auth.rs [View Diff]                   │
│                                               │
│  Codex (5 files, +200 -80)                    │
│  ├ auth.rs      [View Diff]                   │
│  ├ jwt.rs (new) [View Diff]                   │
│  └ ...                                        │
│                                               │
│  [ Apply Claude's ] [ Apply Codex's ] [ Mix ] │
└───────────────────────────────────────────────┘
```

操作选项：
- **Apply**：一键采纳某个 Agent 的全部改动
- **Mix**：进入逐文件选择模式（Claude 的 auth.rs + Codex 的 jwt.rs）

#### Step 4：Pipeline 模式

选择 Pipeline 时的进度展示：

```
┌─ Pipeline: 重构认证模块 ──────────────────────┐
│                                               │
│  [1] Analyze ✅ → [2] Implement 🔄 → [3] Test ⏳ │
│      Claude        Codex              Claude  │
│      12s           Running...         Queue   │
│                                               │
│  Step 2 output is auto-fed to Step 3          │
└───────────────────────────────────────────────┘
```

每一步完成后用户可以审查、修改再继续，也可以设为全自动。

### 3.3 交互设计原则

1. **不打断现有工作流**：Task Bar 默认折叠。用户在普通 pane 里手动用 Claude 一样正常工作。Task 系统是增量能力，不是替代。

2. **Agent Pane = 普通 Pane + 状态指示**：Agent Pane 有额外的状态色条（顶部），Amux 能捕获输出来判断进度。但本质上还是终端——用户可以直接打字交互。

3. **渐进式复杂度**：用户可以只用 Compare（最简单），不碰 Pipeline。

### 3.4 快捷键

| 快捷键 | 功能 |
|--------|------|
| `Ctrl+Shift+T` | 新建 Task |
| `Ctrl+Shift+[` / `]` | 在 Compare 结果间切换 |
| `Enter` | 在结果视图中采纳选中方案 |
| `Escape` | 折叠 Task Bar |

---

## 4. 商业模式与定价

### 4.1 定价原则

用户不会为"工具"付费，会为**省钱**和**省时间**付费。

### 4.2 三层收费模型

#### 免费层 — 开源，让用户进来

- Amux 作为终端复用器完全免费开源
- 多 pane、主题、快捷键、workspace、sidebar 全部免费
- 手动启动 Agent、手动切换——跟现在一样
- **目的**：替代 Windows Terminal / tmux，成为 Vibe Coding 的默认终端

#### Pro 层 — $15-20/月 — 省时间

| 功能 | 免费 | Pro |
|------|------|-----|
| Compare 模式（同一需求发给多个 Agent） | 手动操作 | 一键发起 |
| Task Bar 进度追踪 | ✗ | ✓ |
| 结果对比 Diff View | ✗ | ✓ |
| Agent 性能统计（哪个 Agent 擅长什么） | ✗ | ✓ |
| Pipeline 编排（Agent 串联） | ✗ | ✓ |
| 历史任务回溯 | ✗ | ✓ |

**用户算账**：Pro 帮我每天省 30 分钟手动切换和对比的时间，$20/月很划算。

#### Team 层 — $40-50/人/月 — 省 token 费

- **共享上下文缓存**：团队成员 A 让 Claude 读过的项目结构，成员 B 不需要重复花 token
- **Token 用量看板**：谁花了多少、花在哪了、哪些是浪费的
- **Agent 路由策略**：简单问题自动发给便宜的模型，复杂的发给强模型
- **审计日志**：谁用 Agent 改了什么代码

**用户算账**：团队 5 人每月 token 花 $500，Amux Team 帮省 40% 就是 $200，收 $50/人也值。

### 4.3 付费卡点设计

在用户**尝到甜头后**收费，而非使用前：

```
第一次 Compare → 免费体验，结果并排展示
第二次 Compare → "You've used 1 of 3 free comparisons this month"
第四次 Compare → "Upgrade to Pro — $19/mo"
                  [ Start Free Trial ]  [ Maybe Later ]
```

- **Trial**：14 天全功能
- **到期后**：Compare 限 3 次/月，Pipeline 锁定，Task Bar 只显示最近 1 个任务
- 功能不是完全不能用，是**用起来有摩擦**——刚好够让你怀念 Pro 的体验

### 4.4 长期收费机会：Agent 分发

当用户量达到一定规模后，最值钱的是 **Agent 分发平台**：

- Amux 积累大量"哪个 Agent 在什么任务上表现最好"的数据
- 新 Agent（如垂直领域 Coding Agent）想获取用户 → 在 Amux 上架
- Amux 向 Agent 厂商收推荐费/分成
- 类似 App Store 模式，用户端免费或低价

---

## 5. MVP 定义（最小可行产品）

### 5.1 MVP 范围：Compare 模式

只做 Compare 模式，验证核心假设："用户愿意为跨 Agent 对比结果付费"。

**功能清单**：

- [ ] Task Bar UI（折叠/展开）
- [ ] 任务输入面板（需求文本 + 选择 Agent）
- [ ] 自动创建 2 个 pane，各启动选中的 Agent
- [ ] 将相同需求文本自动发送给两个 Agent（bracketed paste）
- [ ] Agent 完成检测（基于现有 agent status 检测机制）
- [ ] 结果并排展示（复用现有 split pane 布局）
- [ ] 简单的 "Apply" 按钮（基于 git stash/apply 实现分支切换）

**不做**：
- Pipeline 编排
- 共享上下文缓存
- Token 统计
- Agent 推荐算法

### 5.2 技术实现思路

1. **Task Bar** — 新的 GPUI 组件，放在 content area 顶部，与 status bar 类似的渲染方式
2. **任务状态机** — `Pending → Running → Completed → Applied/Dismissed`
3. **Agent 需求注入** — 复用现有 `send_paste_input()` 将需求文本发送到各 Agent pane
4. **完成检测** — 复用现有 `poll_activity()` 的 agent status 检测（Thinking → Done）
5. **结果对比** — 每个 Agent 跑之前 `git stash` 保存当前状态，跑完后 `git diff` 收集改动

### 5.3 依赖的现有能力

| 已有能力 | 用于 |
|----------|------|
| Agent status 检测 | 判断 Agent 是否完成 |
| Split pane 布局 | Compare 模式并排显示 |
| send_paste_input() | 向 Agent 发送需求 |
| Layout template | 快速创建 Compare 布局 |
| Toast notification | 任务状态通知 |

---

## 6. 执行节奏

| 阶段 | 时间 | 目标 |
|------|------|------|
| 现在 | 进行中 | 开源免费，做好终端体验，积累 Vibe Coding 用户群 |
| Phase 1 | 3-6 个月 | 上 Compare 模式 MVP，Pro 订阅验证付费意愿 |
| Phase 2 | 6-12 个月 | 上 Team 层 + Token 优化，打企业市场 |
| Phase 3 | 12 个月+ | Pipeline 编排 + Agent 上架/分发平台 |

---

## 7. 待决事项

- [ ] Compare 模式下如何隔离两个 Agent 的代码改动？（git branch / worktree / stash）
- [ ] Agent 完成检测的准确率是否足够支撑自动化流程？
- [ ] 结果 Diff View 是在 Amux 内实现还是调用外部 diff 工具？
- [ ] Pro 层是本地许可证还是云端账号系统？
- [ ] 定价敏感度：$15 还是 $20？需要用户调研
- [ ] Agent 分发平台是否需要标准化的 Agent 接入协议？
