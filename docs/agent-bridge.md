# Agent Bridge — 跨 Pane Agent 协作协议

## 它是什么？

Amux 在每个 pane 里运行独立的 AI Agent（Claude Code、Codex、Aider 等）。默认情况下这些 Agent 是隔离的——互相看不见、也无法通信。

**Agent Bridge** 打破这种隔离。它为每个 pane 赋予身份标识，并提供命令让 Agent 之间可以：

- **发现** — 列出所有 pane 及其 Agent 状态
- **观察** — 读取任意 pane 的终端输出
- **通信** — 向其他 pane 发送结构化消息
- **自识别** — 让 Agent 知道自己在哪个 workspace/pane 里

消息使用简单的文本信封格式，作为终端输入送达目标 pane。无需共享内存、无需 socket、无需自定义协议——只需 Amux 进程内直接调用。

---

## 应用场景

### 场景 1：任务流水线 — 多 Agent 分工协作

```
┌─────────────────────────────────────────────────────────┐
│  Pane 1: Claude (架构)  │  Pane 2: Aider (代码)  │  Pane 3: Codex (测试)  │
│  设计 API 接口          │  实现业务逻辑           │  编写测试用例           │
│  ──────────────→        │  ──────────────→        │                        │
│  "接口设计完毕，         │  "代码写完了，           │                        │
│   请按 api.rs 实现"     │   请写测试"             │                        │
└─────────────────────────────────────────────────────────┘
```

Claude 完成架构设计后，执行：
```bash
amux pane message pane-2 "接口设计完毕，请按 src/api.rs 的类型签名实现业务逻辑"
```
Aider 收到消息开始写代码。完成后通知 Codex：
```bash
amux pane message pane-3 "业务逻辑已实现，请为 src/services/ 编写单元测试"
```

**价值**：Agent 间形成自动化流水线，用户只需启动任务，无需手动复制粘贴上下文。

### 场景 2：上下文共享 — 跨 Pane 读取输出

Agent A 在 pane-1 遇到编译错误，Agent B 在 pane-2 可以主动读取错误输出：

```bash
amux pane read pane-1 --lines 20
```

看到错误后自行分析并修复，不需要用户手动传递错误信息。

**价值**：消除 Vibe Coding 中最大的效率瓶颈——手动在 Agent 之间复制上下文。

### 场景 3：Agent 状态监控 — Sidebar Agents 视图

用户同时运行 5 个 Agent。Sidebar 切换到 Agents 模式，按状态分组显示：

```
⚠ ATTENTION (需要输入)
  ├─ Claude [pane-3] — waiting     ❗ [回复]
  └─ Codex [pane-5] — error        ✗

⚡ RUNNING
  └─ Aider [pane-7] — thinking     ⟳

✅ COMPLETED
  └─ Gemini [pane-2] — done        ✓
```

鼠标悬停任意 Agent 行 → 弹出最后 8 行输出预览，不用切换 pane。

**价值**：一目了然地管理多个并行 Agent，不再迷失在 pane 切换中。

### 场景 4：Quick Reply — 不打断工作流回复 Agent

Claude 在 pane-3 问了一个问题等待回复。用户正在 pane-1 工作。

在 Sidebar 的 Agents 视图中看到 Claude 的 "waiting" 状态 → 点击回复按钮 → 输入回答 → Enter 发送。全程不需要切换 pane。

**价值**：保持工作流连贯，同时响应多个 Agent 的交互需求。

### 场景 5：文件所有权协调 — 防止冲突编辑

两个 Agent 可能同时修改同一个文件导致冲突：

```bash
amux pane message pane-2 "我正在重构 src/auth/ 目录，在我通知你之前请不要修改这些文件"
```

**价值**：避免多 Agent 并行开发时的文件冲突。

### 场景 6：Build/Test 监控 — 自动通知

一个 pane 运行测试监控脚本，失败时自动通知 Agent：

```bash
while true; do
  output=$(cargo test 2>&1)
  if echo "$output" | grep -q "FAILED"; then
    amux pane message pane-1 "测试失败: $(echo "$output" | tail -5)"
  fi
  sleep 60
done
```

**价值**：构建自动反馈闭环，Agent 无需轮询测试状态。

### 场景 7：问题升级 — 弱 Agent 求助强 Agent

能力较弱的 Agent 遇到难题时，升级给更强的 Agent：

```bash
amux pane message pane-1 "卡在 Rust 生命周期问题：src/cache.rs:142，能帮忙看看吗？"
```

**价值**：让不同能力的 Agent 形成层级协作。

### 场景 8：Orchestrator 模式 — 一个 Agent 协调多个 Worker

一个 "总管" Agent 分配任务给多个 Worker Agent：

```bash
# 总管分配任务
amux pane message pane-2 "实现缓存层，参考 src/cache/"
amux pane message pane-3 "更新 API 文档 docs/api.md"
amux pane message pane-4 "按 Figma 设计稿构建设置页面"

# 轮询进度
for pane in pane-2 pane-3 pane-4; do
  echo "=== $pane ==="
  amux pane read $pane --lines 5
done
```

**价值**：实现复杂项目的自动化项目管理。

### 场景 9：Agent 自发现 — 开箱即知如何协作

新启动的 Agent 通过环境变量知道自己的身份：

```bash
echo $AMUX_PANE_ID    # pane-3
echo $AMUX_WORKSPACE  # myapp

# 发现其他 Agent
amux pane list | jq '.[] | select(.agent_status == "running")'
```

结合 AGENTS.md 模板，Agent 启动后即知如何使用 Bridge。

**价值**：零配置的 Agent 协作能力。

---

## 快速开始

### 1. 查看所有 Pane

```bash
$ amux pane list
[
  {
    "pane_id": "pane-1",
    "tab_title": "claude",
    "agent_kind": "claude",
    "agent_status": "thinking",
    "workspace": "myapp"
  },
  {
    "pane_id": "pane-3",
    "tab_title": "codex",
    "agent_kind": "codex",
    "agent_status": "waiting",
    "workspace": "myapp"
  }
]
```

### 2. 读取其他 Pane 的输出

```bash
$ amux pane read pane-3 --lines 20
```

返回 pane-3 的最后 20 行终端输出。

### 3. 发送消息

```bash
$ amux pane message pane-3 "API schema 变更了，请更新测试"
```

pane-3 收到的输入：
```
[amux-bridge workspace:myapp pane:pane-1 agent:claude] API schema 变更了，请更新测试
```

### 4. 查看当前身份

```bash
$ amux pane id
myapp/pane-1/claude
```

---

## CLI 命令参考

### `amux pane list`

列出所有 pane 及其 Agent 状态。输出为 JSON。

| 字段 | 说明 |
|------|------|
| `pane_id` | Pane 唯一标识（如 `pane-1`） |
| `tab_title` | Tab 标题 |
| `agent_kind` | Agent 类型（`claude`/`codex`/`aider` 等）或 null |
| `agent_status` | `thinking`/`waiting`/`done`/`error` 或 null |
| `workspace` | 所在 workspace 名称 |

### `amux pane read <pane-id> [--lines N]`

读取指定 pane 的终端输出。

- **默认**：50 行
- **上限**：200 行
- 输出为原始文本

### `amux pane message <pane-id> "<text>"`

向指定 pane 发送消息。消息自动包裹信封格式，包含发送者身份。

### `amux pane id`

显示当前 pane 的身份信息。读取环境变量：

| 变量 | 说明 | 示例 |
|------|------|------|
| `AMUX_WORKSPACE` | Workspace 名称 | `myapp` |
| `AMUX_PANE_ID` | Pane 标识 | `pane-3` |
| `AMUX_PANE_TITLE` | Tab 标题 | `claude` |
| `AMUX_VERSION` | Amux 版本 | `0.1.0` |

---

## 消息信封格式

每条消息包裹在结构化信封中：

```
[amux-bridge workspace:<name> pane:<id> agent:<kind>] <text>
```

示例：
```
[amux-bridge workspace:myapp pane:pane-1 agent:claude] 请检查 src/auth.rs 的测试
```

### 解析示例

**Bash：**
```bash
if [[ "$line" =~ ^\[amux-bridge\ workspace:(.+)\ pane:(.+)\ agent:(.+)\]\ (.+)$ ]]; then
  workspace="${BASH_REMATCH[1]}"
  pane_id="${BASH_REMATCH[2]}"
  agent="${BASH_REMATCH[3]}"
  message="${BASH_REMATCH[4]}"
fi
```

**Python：**
```python
import re
m = re.match(r'^\[amux-bridge workspace:(.+) pane:(.+) agent:(.+)\] (.+)$', line)
if m:
    workspace, pane_id, agent, message = m.groups()
```

---

## 教 Agent 使用 Bridge

将以下内容添加到 Agent 的 system prompt 或 `AGENTS.md`：

```markdown
## Amux 跨 Agent 协作

你运行在 Amux 终端多路复用器中。可以通过 `amux` 命令与其他 Agent 协作：

- **发现**: `amux pane list` — JSON 列出所有 pane 及 Agent 状态
- **观察**: `amux pane read <pane-id> --lines 20` — 读取其他 pane 的输出
- **通信**: `amux pane message <pane-id> "消息内容"` — 向其他 Agent 发送消息
- **身份**: `amux pane id` — 显示你所在的 pane 信息

收到的消息格式为：
`[amux-bridge workspace:<w> pane:<id> agent:<kind>] <text>`

看到此格式的输入时，这是来自其他 Agent 的消息，请阅读并响应。
```

---

## 最佳实践

1. **消息简短可执行**。消息作为终端输入送达——过长的消息可能被截断。
2. **发消息前先 `pane list`**。确认目标 pane 存在且有运行中的 Agent。
3. **用 `pane read` 确认结果**。不要假设消息已被执行。
4. **消息中包含完整上下文**。接收方没有共享内存，需要文件路径、分支名等信息。
5. **避免消息循环**。两个 Agent 互相通知完成会导致无限 ping-pong。设计单向流或使用协调者。
6. **优先 `pane read` 而非发消息**。读取输出是即时且不打断的，发消息会中断 Agent 当前工作。

---

## 与 Mori Agent Bridge 的差异

| 方面 | Mori | Amux |
|------|------|------|
| 通信基础 | tmux send-keys（跨进程） | 进程内直接调用（更快更可靠） |
| 身份发现 | MORI_* 环境变量 | AMUX_* 环境变量 + 进程内直接查询 |
| 输出读取 | tmux capture-pane（有延迟） | AlacrittyTerminal::last_lines()（实时） |
| 状态检测 | tmux pane option 轮询 5s | poll_activity() 每帧检测 |
| 消息信封 | `[mori-bridge project:X worktree:Y ...]` | `[amux-bridge workspace:X pane:Y ...]` |
| IPC 机制 | Unix Domain Socket | 进程内直接调用；后续可选 Named Pipe/UDS |
| 外部 CLI | 独立 `mori` 二进制 | 终端内 `amux pane` 命令；后续可选独立 CLI |

**Amux 的架构优势**：单进程架构意味着 Bridge 实现更简单、延迟更低、可靠性更高。不需要处理进程间序列化、socket 连接断开、权限等问题。

---

## 局限性

- **单向投递**。消息是 fire-and-forget，无送达确认——用 `pane read` 检查。
- **无消息队列**。如果目标 Agent 未在等待输入，消息作为意外终端输入到达。
- **纯文本**。不支持二进制数据或结构化载荷（信封格式之外）。
- **环境变量依赖**。`amux pane id` 和发送者身份依赖 AMUX_* 环境变量。
