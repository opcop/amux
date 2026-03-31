# AMUX 多 Agent 协作 — 头脑风暴分析文档

> 日期：2026-03-31
> 状态：探索阶段，尚未进入实现

---

## 一、背景与动机

在 amux 中可以同时运行多个 AI agent（Claude、Codex、OpenCode、Aider、Gemini 等），每个 agent 独占一个终端 pane。当前各 agent 之间完全隔离，无法感知彼此的存在或协作。

如果能让这些 agent 相互通信、分工协作，可能释放出远超单 agent 的生产力。

**核心问题**：
1. 技术上怎么实现 agent 间通信？
2. 什么商业模式能让用户自愿付费？
3. 交互上怎么自然地融入 amux？

---

## 二、技术方案分析

### 2.1 amux 已有的能力

| 能力 | API | 说明 |
|---|---|---|
| 读取任意 pane 终端内容 | `term.with_term(\|t\| t.renderable_content())` | 等价于 tmux capture-pane |
| 向任意 pane 注入文字 | `term.send_input(bytes)` | 等价于 tmux send-keys |
| Pane/Tab/Workspace 管理 | TerminalManager | 完整的布局管理 |
| 标签命名 | `tab.title` / `tab.custom_title` | 等价于 tmux pane label |
| 所有 pane 在同一进程 | GPUI 单进程架构 | 比 tmux socket IPC 更低延迟 |

### 2.2 五种通信方案对比

#### 方案 A：读取 Alacritty 终端缓冲区（被动观察）

直接读取任意 pane 的网格内容。

- **优势**：零侵入，不需要 agent 适配，现成 API
- **劣势**：终端内容是展示层数据，解析语义困难；滚动缓冲区有限；轮询延迟高
- **适合**：状态监控（agent 是否空闲、最后输出是什么）

#### 方案 B：自定义 OSC 转义序列（终端原生通道）

定义 amux 私有 OSC 序列，agent 通过 stdout 发送，amux 拦截并路由：

```bash
printf '\e]amux;msg;target=pane-2;hello world\a'
```

- **优势**：纯 PTY 通道，不需要额外进程/socket；agent 只需 print/read
- **劣势**：读取结果返回给 agent 需要 stdin 注入，解析复杂；二进制数据需 base64
- **适合**：状态通知、简单消息传递

#### 方案 C：内置 amux-bridge CLI + Unix Socket（smux 同款思路）

amux 启动时开一个 Unix domain socket，提供 CLI 工具：

```bash
amux-bridge read codex 20        # 读取 codex pane 最后 20 行
amux-bridge type codex "请审查"   # 往 codex pane 注入文字
amux-bridge keys codex Enter     # 发送回车
amux-bridge list                 # 列出所有 pane
```

- **优势**：与 smux 相同的使用模式，agent 零适配成本
- **劣势**：需要实现 socket server + CLI 工具
- **适合**：完整的双向通信，主要通道

#### 方案 D：MCP Server（结构化通信）

amux 内嵌 MCP 服务器，暴露 tools（list_agents, read_pane, send_message 等）。

- **优势**：Claude 原生支持 MCP，结构化、有类型
- **劣势**：非所有 agent 支持 MCP，实现复杂度高
- **适合**：为 Claude 等支持 MCP 的 agent 提供增强体验

#### 方案 E：共享文件系统（万能兜底）

`~/.amux/mailbox/` 目录，agent 读写文件通信。

- **优势**：100% 通用
- **劣势**：延迟高、需要轮询、竞争条件
- **适合**：不支持其他方案的 agent 的兜底方案

### 2.3 推荐分层架构

```
┌─────────────────────────────────────────────────┐
│                   amux (hub)                     │
│                                                  │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐       │
│  │ Pane 1   │  │ Pane 2   │  │ Pane 3   │       │
│  │ Claude   │  │ Codex    │  │ OpenCode │       │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘       │
│       │PTY          │PTY          │PTY           │
│       ▼             ▼             ▼              │
│  ┌─────────────────────────────────────────┐     │
│  │           Message Router                │     │
│  │                                         │     │
│  │  Layer 1: amux-bridge CLI + Socket      │     │
│  │  Layer 2: OSC 序列拦截                    │     │
│  │  Layer 3: 终端缓冲区读取（状态观察）        │     │
│  │  Layer 4: MCP Server（Claude 增强）       │     │
│  │  Layer 5: 共享文件（兜底）                 │     │
│  └─────────────────────────────────────────┘     │
└─────────────────────────────────────────────────┘
```

### 2.4 smux 参考项目分析

smux（`third_party/smux/`）基于 tmux 实现了多 agent 通信，核心机制：

- **读取**：`tmux capture-pane` 提取 pane 屏幕内容
- **写入**：`tmux send-keys -l` 向 pane stdin 注入文字
- **协议**：纯文本消息头 `[tmux-bridge from:claude] ...`
- **架构**：完全去中心化，无 server/broker，pane 之间 peer-to-peer
- **纪律**：Read Guard 机制——必须先 read 才能 type，防止盲目注入
- **异步**：agent 发完消息不等待，回复自然出现在自己的终端里

**smux 的启发**：
1. 终端屏幕本身就是消息总线
2. 异步不等待是正确的协作模式
3. Read Guard 简单但关键
4. 去中心化 > 中心化 broker

**amux vs smux 的优势**：
- amux 所有 pane 在同一进程，通信延迟更低
- amux 不受 tmux 纯文本 UI 限制，可以有原生 GUI
- amux 不依赖外部 tmux 安装

---

## 三、amux 的产品定位思考

### 3.1 amux 不是终端复用器

| 类型 | 代表 | 特征 |
|---|---|---|
| 终端复用器 | tmux, screen, zellij | 运行在已有终端内部，纯文本 UI |
| 终端模拟器 | Alacritty, iTerm2, WezTerm | 创建自己的窗口，GPU 渲染 |
| **amux** | — | GPUI 原生窗口 + alacritty_terminal，自带 split/tab/workspace |

amux 是终端模拟器，不是复用器。拥有完整的 GPUI 渲染能力，可以构建超越纯文本的 UI 组件（进度条、节点图、面板、拖拽交互等）。

### 3.2 更准确的定位

**以终端为核心的 AI 开发环境（IDE-lite）**

| | tmux + smux | VS Code + Copilot | amux |
|---|---|---|---|
| 多 agent | ✅ 纯文本 | ❌ 单 agent | ✅ 原生 UI |
| 可视化编排 | ❌ | ❌ | ✅ GPUI |
| Agent 间通信 | ✅ hack 级 | ❌ | ✅ 原生 |
| 终端体验 | ✅ tmux 级 | 一般 | ✅ Alacritty 级 |
| 可扩展 UI | ❌ 文字限制 | ✅ Web | ✅ GPUI 原生 |

---

## 四、商业模式探索

### 4.1 单个 AI agent 正在商品化

Claude、GPT、Gemini、Codex 能力趋同。真正稀缺的是让多个 agent 协同产出高质量结果的**编排能力**。类比：单台服务器便宜，Kubernetes（编排层）是千亿市场。

### 4.2 五种商业模式

#### 模式 1：AI 工程团队（最直接）

用户描述任务，amux 自动启动多 agent 团队（Coder + Reviewer + Tester），协作完成。

- **价值**：一个人的成本，三个人的产出质量
- **定价**：3-agent $49/月，5-agent $99/月，不限 $199/月
- **可行性**：★★★★★ 痛点明确，价值可量化

#### 模式 2：多 Agent Debug（高频刚需）

Bug 报告 → 自动启动 debug swarm（Reproducer + Analyzer + Historian + Fixer）。

- **价值**：2 小时 debug 压缩到 5 分钟
- **定价**：包含在订阅中
- **可行性**：★★★★☆ 效果立竿见影，demo 效果好

#### 模式 3：AI Quality Gate（企业级）

PR 提交 → 多 agent review pipeline（Security + Performance + Correctness + Standards）。

- **价值**：降低安全和质量风险
- **定价**：$20/dev/月
- **可行性**：★★★★☆ 企业付费意愿强

#### 模式 4：Agent 工作流市场（平台模式）

用户创建和分享多 agent 工作流模板，平台抽成。

- **价值**：买经验不买工具
- **定价**：平台抽成 30%
- **可行性**：★★★☆☆ 需要临界规模

#### 模式 5：持续 AI 开发环境（最有想象力）

7×24 后台 agent 持续维护代码（依赖升级、测试覆盖、文档更新）。

- **价值**：睡觉时 AI 团队帮你维护代码
- **定价**：$99-299/月/项目
- **可行性**：★★☆☆☆ 需要信任建立，适合成熟期

### 4.3 优先级排序

1. **AI 工程团队** — 痛点最明确，先做 MVP
2. **多 Agent Debug** — 高频场景，容易 demo
3. **Quality Gate** — 自然延伸到 B2B
4. **工作流市场** — 后期叠加的平台效应
5. **持续开发环境** — 长期愿景

---

## 五、交互设计探索

### 5.1 核心矛盾

多个 agent 同时输出，用户看不过来。终端是线性的、一次聚焦一个。多 agent 协作是并行的、分布式的。

**关键不是"怎么显示更多 pane"，而是"怎么让用户不需要看每个 pane"。**

### 5.2 轻量方案：Workspace Template + Tab 状态

完全复用现有 pane/tab 系统，不引入新 UI 概念。

**团队模板**（`~/.amux/teams/`）：

```yaml
name: "Code Review Team"
agents:
  - role: coder
    tool: claude
    instruction: "你是 Coder。收到任务后写代码。完成后通知 reviewer。"
  - role: reviewer
    tool: codex
    instruction: "你是 Reviewer。收到代码后做 review。"
  - role: tester
    tool: claude
    instruction: "你是 Tester。reviewer 通过后写测试。"
```

一键启动：命令面板 `Start Team: Code Review Team` → 自动创建 workspace、pane、启动 agent、注入角色指令。

**Tab 状态指示器**：

```
🟢 Coder     ← 正在工作
🟡 Reviewer  ← 有新消息待处理
⚪ Tester    ← 空闲等待
🔴 Reviewer  ← 发现问题，需注意
```

**状态栏消息聚合**：

```
[Status Bar]  Reviewer→Coder: "auth.rs L42 有 SQL 注入风险"  [14:07]
```

### 5.3 进阶方案：Mission Panel（利用 GPUI 能力）

amux 不是 tmux，不受纯文本限制。可以构建原生 UI 面板：

```
┌─ Sidebar ─┐┌─ Mission Panel ──────────────────────────┐
│            ││ 🎯 实现用户注册功能                        │
│ Workspaces ││                                          │
│            ││ ┌──────┐  ┌──────┐  ┌──────┐             │
│ 📁 日常     ││ │Coder │→│Review│→│Tester│             │
│ 📁 Auth ←  ││ │ 🔄78% │  │ ⏳   │  │ ⏳   │             │
│            ││ └──────┘  └──────┘  └──────┘             │
│            ││                                          │
│            ││ 💬 Coder→Reviewer: module 完成，请 review  │
│            ││                                          │
│            │├─ Coder (Claude) ─────────────────────────┤
│            ││ $ ...终端输出...                           │
│            │└──────────────────────────────────────────┘
└────────────┘
```

Mission Panel 是原生 GPUI 组件（非终端 pane），可包含：可视化 agent 流水线、实时进度条、消息 timeline、点击展开/收起对应终端。

### 5.4 用户旅程

```
1. 安装 amux（免费）→ 当优秀终端用，留存
2. 发现一键启动 AI 工具 → 单 agent 体验
3. 发现"团队模板"→ 试用免费 2-agent Code Review
4. 付费解锁 → 更多模板、自定义团队、后台 agent
```

---

## 六、待决问题

1. **收费对象**：个人开发者（$10-50/月）vs 企业（$100-1000/月/团队）？两者功能需求差异大。
2. **API 费用**：用户 BYOK（自带 key）还是 amux 包 API？前者门槛低，后者利润高。
3. **第一个 demo**：建议从"双 agent code review"开始——最小可行、价值直观。
4. **壁垒**：amux 的护城河是什么？自建终端栈是技术壁垒，需要找到商业壁垒（网络效应？工作流市场？企业功能？）。
5. **"指挥官 pane" vs "平等 pane"**：用户只跟一个 pane 交互 vs 可以跟任何 agent 对话。
6. **通信可见性**：agent 间对话显示在终端里（透明）还是静默（干净）？
7. **错误处理**：agent 卡住、质量差、陷入循环怎么办？

---

## 七、下一步

- [ ] 确定产品定位（终端 multiplexer vs AI 开发环境）
- [ ] 选定第一个 MVP 场景（建议：双 agent code review）
- [ ] 实现 agent 通信基础设施（建议先做 amux-bridge CLI + Unix socket）
- [ ] 设计团队模板格式
- [ ] 原型验证用户价值
