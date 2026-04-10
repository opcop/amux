# AMUX Vibe Coding Feature Wishlist

> 这份文档是从"重度 vibe coding 用户"视角对 amux 的产品评估，记录的是
> "让我真正爱上 amux 的功能清单"，按对日常工作流影响排序。**不是**当前
> 迭代的开发任务表 —— 当前优先级仍然是把现有功能在三平台上跑通，这份
> 清单是后续产品方向的输入。
>
> 写于 2026-04-09，依据当时的代码状态（commit `efb77c1` 之后）。

## 评估前提

amux 当前状态：技术上"能用"，UI 上"是个 Mac 应用"，但**它还不知道自己想
成为什么**。一个普通终端用户用 amux，跟用 iTerm2 + tmux 没有本质区别。

让 amux 真正区别于普通终端 + IDE 的关键定位：**在 vibe coding 这一个垂
直方向做到没有竞争对手**。下面的 P0 五件事都是普通终端做不到、IDE 也做
不到的，是 amux 独有的位置。

---

## 🔥 P0 — Killer 特性，没有这些 amux 跟 tmux 没本质区别

### P0-1. Prompt Library（最高频痛点）

**问题陈述**：
重度 vibe coder 每个人都有 100+ 条反复用的 prompts（"审计这段代码的边界
条件"、"把这个改成更 idiomatic 的 Rust"、"为这个写测试"、"解释这段代码
我哪里没看懂"...）。这些 prompt 现在散落在 Notion / Obsidian / 截图文件
夹里，每次用要先翻出来 Cmd+C Cmd+V。

**目标**：
- amux 内置 prompt 库，组织成 categories（debug / refactor / test /
  explain / review）
- **一键 invoke**：例如 `Cmd+;` → 模糊搜索 prompt → 选中 → 自动 send 到
  当前 agent pane
- **变量插值**：`{{file}}`, `{{selection}}`, `{{git_branch}}`,
  `{{git_diff}}`, `{{cwd}}`, `{{recent_error}}` 等占位符自动填充
- **per-workspace + global** 两层：通用的放 global，项目特定的（"修这个
  codebase 的 hooks 系统"）放 workspace
- **使用记录**：哪些 prompt 用得最多自动浮顶
- **Markdown 文件后端**：`~/.amux/prompts/*.md` + per-workspace
  `.amux/prompts/*.md`，可以放进 dotfiles 同步

**已有线索**：commit `b75385d` 已经有 prompt library design 文档，可能已
经设计好了——可以从那个 doc 开始。

**为什么是 P0**：这一个特性能让用户每天省 30+ 分钟，并且降低了"开始一次
AI 对话"的心理摩擦。这是 amux 区别于普通终端的最直接价值。

### P0-2. 上下文一键注入（"把这个发给 Claude"）

**问题陈述**：
每次想让 Claude 看一个文件，要先 `cat 文件名 | pbcopy` 然后切到 Claude
pane 然后粘贴。或者每个 agent 自己有 file 命令但每个 agent 都不一样。

**目标**：
- 在 file picker (Cmd+P) 里选中一个文件 → Cmd+Enter → "发送到 active
  agent"，自动按当前 agent 的格式（Claude `@filename`，Cursor `#file:`，
  opencode 相对路径）
- 在终端里选中一段文字 → 已有的 Send to Pane (Cmd+Shift+Enter) → 但目标
  picker 应该显示**"frontend-claude" / "backend-codex"**这种 agent 名字，
  而不是 `pane-2 / pane-3`
- **拖拽**：从 Finder 拖一个文件到 amux pane → 自动插入文件路径
- **多文件组合**：选中 file picker 里的 5 个文件 → "全部发送给 Claude"
  按 Claude 期待的批量格式

**为什么是 P0**：vibe coding 90% 的工作就是"把上下文塞进对话"。这是最高
频动作，每次省 5 秒就是巨大的累积价值。

### P0-3. Agent 输出的代码块抽取（"把那段代码拿出来"）

**问题陈述**：
Claude 给一段长回复里有 3 个 code block，想要第二个。要选区拖拽，要小心
不要选到反引号，要 Cmd+C，还要粘到正确的位置。**这是 vibe coding 最高频
也最反人类的动作**。

**目标**：
- agent pane 自动识别 markdown code block 边界（语言标签 / 反引号 /
  fence）
- **快捷键**：例如 `Cmd+Shift+1/2/3` = 抓 agent 输出里最近的第 1/2/3 个
  code block 到剪贴板
- **更高级**：抓到剪贴板时自动包含 metadata（"from claude pane, file:
  src/foo.rs, language: rust"），其他 pane 的 paste 能用这个上下文
- **直接落盘**：如果 code block 紧跟着 `// src/foo.rs:42-58` 这种 hint，
  提供 "Apply to file" 一键写盘
- **diff preview**：apply 之前先在右侧 panel 显示对原文件的 diff，
  confirm 才真写

**为什么是 P0**：这是 vibe coding **签名动作**。如果 amux 把这个做到极
致，没有任何普通终端能跟它竞争。

### P0-4. Multi-agent broadcast + side-by-side compare

**问题陈述**：
经常想问 Claude / Codex / GPT-5 同一个问题，看谁答得好。今天要在三个 tab
里各自手动输入同一个 prompt。

**目标**：
- 选 N 个 agent pane → "Broadcast prompt" → 弹一个 textarea → 同一个
  prompt 同时发给所有选中的 agent
- **同步 scroll**：N 个 pane 一起滚动看答案
- **diff view**：两个回答的 side-by-side diff，相同部分折叠
- **vote / mark winner**：标记某个回答为"winner"，下次同类问题优先给那
  个 agent

**为什么是 P0**：vibe coding 的一个真理是"agents 各有所长"。amux 如果能
让 multi-agent 协作零摩擦，会形成独特的工作流而不是单 agent 的简单
wrapper。

### P0-5. 跨对话搜索 + 历史

**问题陈述**：
"上周 Claude 跟我说过怎么修一个奇怪的 OAuth bug。这周又遇到，要重新问，
因为没有翻历史对话的入口。"

**目标**：
- amux 自动持久化每个 agent pane 的完整对话历史到
  `~/.amux/history/<pane-id>/<timestamp>.jsonl`
- **全局搜索 (Cmd+Shift+F)**：跨所有历史对话的全文检索，结果显示 snippet
  + 时间戳 + workspace
- **bookmark**：在某个 agent 回答上 `Cmd+B` 收藏，加 tag 和 note
- **export**：把一段对话 export 成 markdown 文件保存到当前 workspace

**为什么是 P0**：vibe coding 的"知识资产"在哪？现在散落在每个 agent 的
临时上下文里，会话结束就丢了。amux 应该是这些资产的家。

---

## ⚡ P1 — 显著提升日常体验

### P1-1. Agent identity & 命名
- 当前 tab 显示 `agent claude`，应该能命名 `frontend-claude` /
  `backend-claude` / `debug-codex`
- 命名持久化到 session，跨重启保留
- Send-to-pane picker 显示这些名字

### P1-2. Cmd+P 全局文件 picker
- 模糊搜索当前 workspace 的所有文件（不只是当前 cwd）
- 跨 workspace 搜索（"在所有打开的 workspace 里找 `auth.rs`"）
- 显示 git status icon（modified / staged / untracked）

### P1-3. 右上角"AI 在等你"通知
- amux 已经有 AgentStatus 检测（`terminal/manager.rs:1085` 的
  `poll_activity`）
- 但应该把这个利用起来：当一个非 active pane 的 agent 从 working → idle，
  dock badge / system notification / 顶部 toast
- 长跑的 Claude 任务可以切走干别的，回来不用一个个 pane 检查

### P1-4. Inline diff after agent edits
- amux 知道当前 workspace 是什么 + git status 能查
- 可以在 sidebar 显示一个 "Recently changed by AI" 列表，点开看 diff
- 用 git 自身的 reflog 跟踪是哪个 agent 改的（agent 是用 amux 启动的，
  就有 `AMUX_AGENT_KIND` env 标识）

### P1-5. Drag & Drop
- Finder → terminal pane = 插入路径
- 多文件 = 多行路径
- 图片 = 已经接通了，保留

---

## 🎨 P2 — Polish

### P2-1. Welcome / empty state
- 当前空 session 启动 GPUI 显示空白 + 一个空 sidebar，没有任何引导
- 应该有一个真的 welcome 屏：「Open Folder」「Start Claude」「Open
  Recent Workspace」「Browse Templates」+ 当前快捷键提示

### P2-2. Settings panel
- 现在配置只能编辑 `~/.amux/config.toml`
- 应该有 GUI settings：主题、字体、agent 偏好、快捷键自定义、prompt
  library 路径
- 支持 hot-reload

### P2-3. Per-pane visual mode
- "Agent driving" pane 周围有一圈淡色边框区分（Claude 是橙色边框，Codex
  是绿色，shell 是无）
- 当前 active pane 边框高亮
- pane 标题栏有 agent 图标

### P2-4. Token usage HUD
- 每个 agent pane 右下角小标签：`12.4k / 200k tokens used`
- 接近上限警告
- 总累计 cost (今天 / 本月)

---

## 🚀 P3 — 大块产品化工作

### P3-1. macOS/Linux Browser host (BrowserService 当前是 Noop)
- 真接通 wry / webkit2gtk
- 让"在 amux 里开 localhost:3000 看页面"这件事跨平台可用

### P3-2. macOS 原生 menubar
- File / Edit / View / Window / Help 标准菜单
- About / Preferences / Hide / Quit 都通过 menu

### P3-3. Workspace 模板
- "新建一个 Rust 项目 workspace" → 自动建目录 + cargo init + .startup
  文件 + 默认布局
- "新建一个 Web 项目 workspace" → npm init + dev server pane + claude
  pane + browser pane

### P3-4. Cloud sync
- prompt library / config / workspace list 同步到 GitHub gist / iCloud
  / dropbox
- 多机环境一致

---

## 评估结论

**让用户真正爱上 amux 的关键**：必须在 vibe coding 这一个垂直方向做到没
有竞争对手。Prompt library + 上下文注入 + code block 抽取 + multi-agent
broadcast + 历史搜索——这五件事都是普通终端做不到、IDE 也做不到的，是
amux 独有的位置。

**单点击穿优先级**：
- **P0-1 (Prompt Library)** 影响"输入侧"
- **P0-3 (Code block 抽取)** 影响"输出侧"

做哪个都能立即让 amux 跟普通终端拉开距离。如果让用户在 P0 里挑一个最想
先要的，**P0-1 + 模糊搜索 invoke + 变量插值** 是最优起点 —— 每 5 分钟就
会用到一次的功能，做出来的瞬间用户会卸载所有 "prompt 收藏夹" workaround。
