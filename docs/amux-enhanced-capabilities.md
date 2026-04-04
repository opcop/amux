# Amux 增强能力设计 — 超越单 Agent 体验

## 核心命题

> Claude Code 是一把好锤子。Amux 让你同时挥舞十把锤子，并且锤子之间会互相配合。

单独用 Claude 是**单人单工具**。在 Amux 里用 Claude 是**多 Agent 协作系统**的一部分。这个从"工具"到"系统"的跃迁，就是 Amux 的核心价值。

## 单独使用 Claude Code 的痛点

| # | 痛点 | 描述 |
|---|------|------|
| 1 | 单线程阻塞 | Claude 在思考时你被阻塞，只能干等 |
| 2 | 盲写前端 | 写了前端代码但看不到效果，得切出去看浏览器 |
| 3 | 历史失忆 | 关了终端历史就没了，2 小时前 agent 做了什么全忘了 |
| 4 | Agent 孤岛 | 同一项目跑 3 个 agent，互相不知道对方的存在 |
| 5 | 质量失控 | Agent 写代码越来越多，但没有人审查 |
| 6 | 手动协调 | "Claude 写完 API 后告诉 Codex 写测试"只能靠人工传话 |
| 7 | 无法监控 | 5 个 agent 同时跑，不知道谁完成了谁卡住了 |
| 8 | 人肉日志搬运 | Claude 加了调试打印让你运行、复制输出、粘贴回来，反复多轮 |
| 9 | 调试噪音 | 程序输出 200 行，Claude 只需要其中 3 行调试输出，但无法过滤 |

---

## 第一层：现有能力已经可以实现

### 1. 即时视觉反馈闭环

```
┌──────────────────┬──────────────────┐
│ Claude Code      │ Browser Tab      │
│                  │                  │
│ > 修改了首页样式  │  [实时看到效果]   │
│ > 调整了按钮颜色  │  [立刻看到变化]   │
│                  │                  │
└──────────────────┴──────────────────┘
```

**现状**：浏览器 tab 已实现。

**增强方向**：浏览器 tab 支持 **auto-refresh** — 检测到文件变更自动刷新页面。Claude 改了代码 → 保存 → 浏览器自动刷新 → 开发者立刻看到效果。不需要手动刷新，不需要切窗口。

**对比单独使用 Claude**：写了前端代码要手动打开浏览器验证。在 Amux 里，写代码和看效果在同一个屏幕，零延迟反馈。

### 2. 多 Agent 并行加速

```
┌──────────┬──────────┬──────────┐
│ Claude   │ Codex    │ Aider    │
│ 写 API   │ 写测试   │ 写文档   │
│ ⟳ 思考中  │ ⟳ 思考中  │ ✓ 完成   │
└──────────┴──────────┴──────────┘
  状态栏：3 agents | 2 running | 1 done
```

**现状**：多 pane + Agent 状态检测 + Sidebar Agents 视图已实现。

**对比单独使用 Claude**：串行工作（写 API → 写测试 → 写文档）。在 Amux 里是并行的，3 个 agent 同时干，吞吐量提升数倍。

### 3. 跨 Agent 上下文传递

```
Claude (pane-1)：遇到编译错误
  ↓
Codex (pane-2)：amux pane read pane-1 --lines 20
  ↓ 读取错误信息
  ↓ 自行修复
```

**现状**：Agent Bridge 的 pane read/message 已实现。

**对比单独使用 Claude**：遇到问题要手动复制错误信息给另一个 agent。在 Amux 里，agent 自己读取，自己修复，无需人工传话。

---

## 第二层：短期可实现的高价值能力

### 4. 测试驱动的自动修复循环

```
Pane 1: Claude (写代码)
Pane 2: 测试监控 (cargo test --watch)
         ↓ 测试失败
         ↓ amux pane message pane-1 "测试失败: src/auth.rs:42 assertion failed"
         ↓ Claude 自动读取失败信息
         ↓ Claude 修复代码
         ↓ 测试再次运行
         ↓ 测试通过
         ↓ 循环继续
```

**实现方式**：一个简单的 shell 脚本 + Agent Bridge。

**对比单独使用 Claude**：Claude 不知道测试是否通过，开发者要手动运行测试、复制错误、粘贴给 Claude。在 Amux 里形成「写代码 → 自动测试 → 失败通知 → 自动修复」的闭环，无需人工干预。

### 5. Agent 失败自动升级

```
Codex (pane-2) 尝试修复 → 3 次失败 → 检测到 error 状态
         ↓ Amux 自动升级
Claude (pane-1) 收到: "Codex 在 src/cache.rs:142 卡了 3 次，请接手"
         ↓ Claude 解决问题
         ↓ 通知 Codex: "已修好，你可以继续了"
```

**实现方式**：Amux 检测 agent 连续 error 状态 → 通过 Bridge 自动升级给更强的 agent。

**对比单独使用任何 agent**：一个 agent 卡住了只能人工发现和干预。在 Amux 里，弱 agent 搞不定的自动交给强 agent，形成能力互补链。

### 6. 文件冲突预防

```
Claude 正在编辑 src/api.rs
Codex 也要编辑 src/api.rs
         ↓ Amux 检测到冲突风险
         ↓ Toast: "⚠ Claude 和 Codex 同时修改 src/api.rs"
         ↓ 自动通知 Codex: "src/api.rs 正在被 Claude 修改，请等待"
```

**实现方式**：监控文件变更事件（`inotify` / `ReadDirectoryChangesW`），追踪哪个 pane 在修改哪些文件。

**对比单独使用 agent**：多 agent 并行时最大的风险就是文件冲突。单独使用根本没有这个概念。Amux 提前预防，省去合并冲突的痛苦。

### 7. 开发成本仪表盘

```
┌─────────────────────────────────────┐
│  Session Cost Tracker                │
│                                      │
│  Claude:  $2.34 (12K in + 8K out)    │
│  Codex:   $0.87 (45K in + 3K out)    │
│  Aider:   $1.12 (8K in + 6K out)     │
│  ─────────────────────────           │
│  Total:   $4.33                      │
│  Per feature: ~$0.72                 │
└─────────────────────────────────────┘
```

**实现方式**：解析各 agent 的输出日志，提取 token 用量（Claude Code 输出 token 统计）。

**对比单独使用 agent**：Vibe Coding 的隐性成本很高但不可见。单独用 Claude 只能看到当前 session 的用量。Amux 汇总所有 agent 的成本，让开发者做出成本优化决策。

---

## 第三层：中期可实现的差异化能力

### 8. 智能上下文注入

新启动一个 agent 时，Amux 自动注入相关上下文：

```
启动 Claude (pane-3)
  ↓ Amux 检测到 pane-1 的 Aider 已经修改了 src/auth.rs
  ↓ 自动注入到 Claude 的环境：
    "注意：src/auth.rs 刚被 Aider 修改（pane-1），
     最近变更：添加了 JWT 验证中间件。
     请基于这个变更继续工作。"
```

**对比单独使用 agent**：新启动的 agent 什么上下文都没有，要从零开始了解项目状态。在 Amux 里，新 agent 一启动就知道其他 agent 刚做了什么。

### 9. 多模型 A/B 对比

```
用户输入一个任务描述
  ↓ Amux 同时发送给 Claude 和 Codex
  ↓
┌──────────────────┬──────────────────┐
│ Claude 的方案     │ Codex 的方案     │
│                  │                  │
│ 用了 trait 抽象   │ 用了 enum 分发    │
│ 15 个文件修改     │ 8 个文件修改      │
│ 测试全过          │ 2 个测试失败      │
└──────────────────┴──────────────────┘
  用户选择：采用 Codex 的方案（更简洁）
```

**对比单独使用 agent**：一个任务只有一个方案。在 Amux 里，让两个 agent 竞争，选最好的方案，质量更高。

### 10. Session 录制与回放

```
[amux session record]

记录所有 pane 的：
- 终端输入/输出时间线
- Agent 状态变化
- 文件变更关联
- 成本数据

[amux session replay --speed 4x]

4 倍速回放整个开发过程
```

**对比单独使用 agent**：Vibe Coding 的过程是"黑箱"——agent 做了什么、为什么做、做的对不对，事后很难追溯。Session 录制让整个过程可审计、可学习、可复盘。

---

## 第四层：长期愿景

### 11. Orchestrator 工作流

```yaml
# ~/.amux/workflows/full-stack-feature.yaml
workflow: full-stack-feature
steps:
  - agent: claude
    task: "设计 API 接口和数据模型"
    output_to: pane-2

  - agent: aider
    task: "根据 pane-1 的设计实现业务逻辑"
    wait_for: pane-1.done

  - agent: codex
    task: "为 pane-2 的实现编写测试"
    wait_for: pane-2.done

  - run: "cargo test"
    on_failure: escalate_to: claude
```

**对比单独使用 agent**：把 Vibe Coding 从"手动协调多个 agent"变成"定义工作流，Amux 自动执行"。开发者变成架构师和审阅者，不再是 agent 的协调者。

### 12. 团队共享 Agent Session

```
开发者 A 在自己的 Amux 里看到：

┌─────────────────────────────────────┐
│  Team Agents (shared workspace)      │
│                                      │
│  👤 Alice - Claude [feature/auth]    │
│  👤 Bob   - Codex  [feature/api]     │
│  👤 You   - Aider  [feature/ui]      │
│                                      │
│  Alice 的 Claude 刚完成了 auth 模块    │
│  → 你的 Aider 可以直接引用            │
└─────────────────────────────────────┘
```

**对比所有现有工具**：团队级别的 Vibe Coding 协作。不只是个人多 agent，而是团队的多 agent 互相感知。这是全新的品类。

---

## 能力价值矩阵

| # | 能力 | 实现难度 | 用户价值 | 差异化程度 | 建议优先级 |
|---|------|---------|---------|-----------|-----------|
| 13 | Auto-Feed 自动喂日志 | 中 | 极高 | 极高 | ★★★★★ |
| 14 | Debug Tag 精确过滤 | 小 | 极高 | 极高 | ★★★★★ |
| 1 | 浏览器 auto-refresh | 小 | 高 | 中 | ★★★★★ |
| 4 | 测试驱动修复循环 | 小 | 极高 | 高 | ★★★★★ |
| 5 | Agent 失败自动升级 | 中 | 高 | 极高 | ★★★★☆ |
| 6 | 文件冲突预防 | 中 | 高 | 极高 | ★★★★☆ |
| 8 | 智能上下文注入 | 中 | 极高 | 极高 | ★★★★☆ |
| 7 | 开发成本仪表盘 | 小 | 中 | 高 | ★★★☆☆ |
| 9 | 多模型 A/B 对比 | 中 | 高 | 极高 | ★★★☆☆ |
| 10 | Session 录制回放 | 大 | 高 | 极高 | ★★★☆☆ |
| 11 | Orchestrator 工作流 | 大 | 极高 | 极高 | ★★★☆☆ |
| 12 | 团队共享 Session | 极大 | 极高 | 极高 | ★★☆☆☆ |

## 第五层：解决 Vibe Coding 最大痛点 — 自动化调试循环

### 痛点分析

Vibe Coding 中最耗精力的不是写代码，而是 **debug 循环**：

```
Claude: "我加了几个 println!，你运行一下把输出发给我"
你: (运行程序，等输出，复制，切回 Claude，粘贴)
Claude: "嗯，再加几个 log，再运行一下"
你: (又复制粘贴一轮)
Claude: "还是不对，换个位置加 log..."
你: (第 8 次当搬运工，已经崩溃)
```

**本质问题**：Claude 的眼睛和手是断开的。它能写代码（手），但看不到运行结果（眼睛）。开发者就是那个在"手"和"眼睛"之间传递信息的**人肉管道**。

更糟糕的是，程序输出有很多种信息：

```
正常业务日志：[INFO] Server started on port 3000
用户请求日志：[INFO] GET /api/users 200 12ms
调试日志：    [DEBUG] user_id = None, token = "expired"
错误信息：    [ERROR] AuthenticationFailed at src/auth.rs:42
框架噪音：    [WARN] Deprecated API used in dependency xyz
编译输出：    Compiling amux-core v0.1.0
测试输出：    test auth::test_login ... FAILED
堆栈跟踪：    thread 'main' panicked at ...
```

Claude 加了 3 行调试打印，但程序可能输出 200 行。全部喂过去 Claude 反而迷惑，token 也浪费。Claude 需要的不是"所有输出"，而是**它自己刚加的那几行调试输出的结果**。

### 13. Auto-Feed — 自动把运行结果喂给 Agent

```
Pane 1: Claude (写代码)
Pane 2: 程序运行中

Claude 加了调试代码 → 保存 → 程序自动重启
  ↓ Pane 2 出现新输出
  ↓ Amux 自动检测增量输出
  ↓ 过滤后转发给 Pane 1
  ↓ Claude 自动分析并修复
  ↓ 用户全程不需要做任何事
```

**核心区别**：
- 写日志文件再读：还是需要 Claude "主动"做一个动作
- Auto-Feed：Claude 完全被动接收，像直接长了一双眼睛

**实现方式**：
- Amux 60fps 定时器里对"被监控"的 pane 增量读取新输出行
- 过滤匹配规则后，通过 Bridge 自动发送给"写代码"的 pane
- 用户通过 `amux feed pane-2 → pane-1` 建立 feed 链路

### 14. Debug Tag 协议 — 精确过滤调试输出

Auto-Feed 的关键问题：200 行输出里怎么知道哪些是 Claude 需要的？

**方案**：让 Agent 给自己的调试输出打上唯一标签，Amux 只转发带标签的行。

```
Claude 写的调试代码：
  eprintln!("[amux-debug-7f3a] user_id = {:?}", user_id);
  eprintln!("[amux-debug-7f3a] auth_result = {:?}", result);

程序输出 200 行，其中只有 2 行带标签：
  [INFO] Server started...
  [INFO] GET /api/users...
  [amux-debug-7f3a] user_id = None          ← 只转发这行
  [amux-debug-7f3a] auth_result = Err(...)  ← 只转发这行
  [WARN] deprecated API...
  ... 196 行其他内容 ...

Amux Auto-Feed 只发给 Claude：
  "[amux-output pane:pane-2 tag:7f3a]
   user_id = None
   auth_result = Err(AuthExpired)"
```

**标签 `7f3a` 是 Agent 自己生成的随机 ID**，每轮调试换一个，确保只收到本轮自己加的调试信息，不会和之前的调试残留混淆。

**分类标签扩展**：

```
[amux-debug-a1b2]         普通调试变量
[amux-debug-a1b2:perf]    性能计时
[amux-debug-a1b2:sql]     SQL 查询
[amux-debug-a1b2:net]     网络请求/响应
```

**多 Agent 隔离**：Claude 用标签 `a1b2`，Codex 用标签 `c3d4`，各自只收到自己的调试输出，互不干扰。

**自动清理提醒**：Amux 检测到调试完成（测试通过或 Agent 说 "fixed"），自动提醒"还有 3 行 debug 打印未清理"。和代码卫生提醒联动。

### 提示词配合

在 `~/.amux/agent-prompt.md` 里教 Agent 使用 Debug Tag 协议：

```markdown
## Amux 调试协议

当你需要添加调试打印时，使用 amux-debug 标签格式：

  eprintln!("[amux-debug-{随机4字符}] 变量名 = {:?}", 变量);

Amux 会自动捕获带此标签的输出并反馈给你，无需用户手动复制。
每轮调试使用新的随机标签，避免与旧日志混淆。
调试完成后删除所有 amux-debug 打印语句。
```

### Auto-Feed 配置

```toml
# ~/.amux/config.toml

[auto_feed]
enabled = true
# 过滤规则：只转发匹配的行
filter_patterns = [
    "amux-debug",      # Debug Tag 协议
    "^error",          # 错误行
    "^panic",          # Rust panic
    "FAILED",          # 测试失败
    "Exception",       # 异常
]
# 同时转发 stderr 的所有内容
include_stderr = true
# 同时转发非零退出码
include_exit_errors = true
# 增量读取间隔（毫秒）
poll_interval_ms = 500
# 每次最多转发行数（防止刷屏）
max_lines_per_feed = 50
```

### 完整的自动化调试流程

```
┌─────────────────────────────────────────────────────────┐
│  Amux Auto Debug Loop (用户只需描述问题，等待结果)       │
│                                                          │
│  Pane 1: Claude              Pane 2: Test/Run            │
│  ┌──────────────────┐       ┌──────────────────┐        │
│  │ 1. 收到问题描述    │       │                    │       │
│  │ 2. 加调试代码      │       │                    │       │
│  │    [amux-debug-x]  │  ──→  │ 3. 文件变更触发运行 │       │
│  │                    │       │ 4. 程序输出 200 行  │       │
│  │ 5. 只收到 3 行     │  ←──  │    其中 3 行带标签  │       │
│  │    标签调试输出     │       │                    │       │
│  │ 6. 分析 → 定位问题  │       │                    │       │
│  │ 7. 修复代码        │  ──→  │ 8. 再次运行         │       │
│  │                    │       │ 9. 测试通过 ✓       │       │
│  │ 10. 收到成功通知   │  ←──  │                    │       │
│  │ 11. 删除调试代码   │       │                    │       │
│  │ 12. 完成 ✓         │       │                    │       │
│  └──────────────────┘       └──────────────────┘        │
│                                                          │
│  用户参与：第 1 步描述问题 + 第 12 步审查结果             │
│  Amux 自动化：第 3-11 步全部自动                         │
└─────────────────────────────────────────────────────────┘
```

---

## 与竞品的能力对比

| 能力 | 单独 Claude | Prowl | tmux | Amux (目标) |
|------|-----------|-------|------|-------------|
| 自动调试循环 | ✗ | ✗ | ✗ | ✓ (Auto-Feed + Debug Tag) |
| 多 Agent 并行 | ✗ | ✓ (手动) | ✓ (手动) | ✓ (自动协调) |
| Agent 间通信 | ✗ | ✗ | ✗ | ✓ (Bridge) |
| 实时预览 | ✗ | ✗ | ✗ | ✓ (Browser tab) |
| 状态监控 | ✗ | 基础 | ✗ | ✓ (Sidebar + 状态栏) |
| 自动升级 | ✗ | ✗ | ✗ | ✓ (Bridge) |
| 冲突预防 | ✗ | ✗ | ✗ | ✓ (文件监控) |
| 成本追踪 | 当前 session | ✗ | ✗ | ✓ (全局汇总) |
| 工作流编排 | ✗ | ✗ | 脚本 | ✓ (声明式 YAML) |
| 质量守卫 | ✗ | ✗ | ✗ | ✓ (Guardian Agent) |

## 产品定位

```
tmux/WezTerm:    终端多路复用器（面向运维）
Prowl:           Agent 启动器 + 状态查看器（面向 macOS 开发者）
Amux:            多 Agent 协作操作系统（面向 Vibe Coding 开发者）
```

Amux 的终极目标不是做一个更好的终端——而是做 **Vibe Coding 时代的操作系统**。终端只是载体，核心价值是让多个 AI Agent 形成高效协作的系统。
