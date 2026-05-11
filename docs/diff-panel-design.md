# Diff Panel 设计 — Workspace-Scoped Git Review

## 背景

Amux 的定位是"管理并行 agent 上下文的 workspace"，而不是另一个 Agentic IDE。但 agent 在 workspace 里改完代码后，用户需要一个**轻量、原地**的方式 review 改动 → commit → push，不必离开 Amux 跳到外部 Git Client。

这是 review context 断裂痛点的直接解法：agent 在 workspace 里改了什么，必须一眼能看清楚。

## 设计原则（三条红线）

1. **Shell out, don't link** — 全部走 `git` 子进程（复用 `portable-pty` 那套），不引入 `git2-rs` / libgit2 依赖。跨平台和 WSL 路径处理自动对齐已有逻辑。
2. **Read-first, write-with-confirm** — 写操作（stage / commit / push）必须键盘确认或显式按钮，不做"自动 commit on save"这类隐式行为。
3. **One surface, not a window** — diff 和 preview 共享右侧 dock 位置（同一物理空间，互斥显示），但状态机**独立**。Diff 用自己的 `DiffPanelState` 和 `gpui_diff_panel.rs`，不与 `PreviewState` 共用 enum 分支。最初设计想把 diff 塞进 `gpui_preview.rs`，实施时发现 preview 已经 87KB 且管 markdown / syntax / search / TOC / 选择等无关状态——硬塞会让那个文件变成两个独立 panel 的 bag of state。改为：右侧 dock 位置是一个 surface slot，preview 和 diff 都能填它，但谁也不污染谁。**不开新窗口、不增加新 layout 节点类型** 这条原意保留。

## 功能边界

### ✅ V1 范围内

| 功能 | 实现要点 |
|---|---|
| Workspace-scoped diff view | 以 active workspace 的 cwd 为根（已有 OSC 7 cwd 跟踪兜底），`git rev-parse --show-toplevel` 检测 repo |
| 变更文件列表 | 左列：`M`/`A`/`D`/`?` badge + 路径；点击 → 右侧显示 diff |
| Unified diff 渲染 | 复用 `gpui_preview.rs` 的 14 语言 syntax highlighter；`+`/`-` 行 Tomorrow Night 调色板 |
| 暂存 / 取消暂存 | 文件级和 hunk 级（封装 `git add` / `git restore --staged`）；行级**不做** |
| Commit message 输入框 | 单行 + 多行折叠；回车提交 → `git commit -m`；空消息禁用按钮 |
| Push 按钮 | 跟随当前 upstream，二次确认；失败把 `git` stderr 原样贴到 panel 底部 |
| Sidebar 状态徽标 | 每个 workspace 显示 `clean` / `N changes` / `↑N ↓N`；后台轮询 `git status --porcelain=v2 --branch` |
| Agent 改动来源标记（可选） | 配合现有 OSC 133，commit 时把 "edited-by: agent\|human" 作为 trailer 写入 commit message |
| 快捷键 toggle | `Ctrl/Cmd+G` 切换 diff panel（复用 preview panel surface） |

### ❌ 明确 out of scope

| 功能 | 不做的理由 |
|---|---|
| 分支管理（create/checkout/delete/rename） | `git switch` / `gh` 已经足够；做了就开始变 Git Client |
| Merge / rebase 冲突 UI | 复杂度爆炸，留给 `git mergetool` 或外部编辑器 |
| Commit log / 历史图 | `git log` / `tig` / `lazygit` 已经成熟，不重复造 |
| Blame view | 低频 + 复杂 |
| Stash / cherry-pick / interactive rebase | 危险操作；CLI 用户本来就会用 |
| PR 创建 / review / 评论 | **这是 2code 等竞品会变厚的方向**，留给 `gh pr` |
| Submodule / multi-repo | 假设一个 workspace = 一个 repo；先不破 |
| GPG 签名 UI | 走 git 配置层 |
| 行级 stage（line-by-line） | hunk 级够用，行级要操作 patch 太重 |
| 替换 git CLI 为 libgit2 | 违反"shell out, don't link" |

## 架构

### 模块划分

```
apps/desktop/src/
├── git_panel/               (纯逻辑层，无 GPUI 依赖)
│   ├── mod.rs
│   ├── model.rs             (WorkspaceGitState, GitFileEntry, FileStatus)
│   ├── status.rs            (detect_repo_root, run_git_status, parse_porcelain_v2)
│   └── diff.rs              (DiffFile, DiffHunk, DiffLine, run_git_diff, parse_unified_diff)
├── gpui_diff_panel.rs       (新增：DiffPanelState 状态机 + 右侧 panel 渲染)
├── gpui_preview.rs          (不动；preview 和 diff 共享 dock 位置但状态独立)
├── gpui_workspace_sidebar.rs (扩展：git status 徽标渲染)
└── gpui_entry.rs            (扩展：Ctrl+G 快捷键路由、维护 workspace_git_states 和 DiffPanelState、spawn poll loop)
```

### 数据流

```
Workspace.cwd (来自 OSC 7 / spawn cwd)
        ↓
git rev-parse --show-toplevel  → repo root 缓存
        ↓
后台轮询 (2s 间隔，可配置):
  git status --porcelain=v2 --branch  → WorkspaceGitState
        ↓
Sidebar 徽标 + DiffPanel 文件列表
        ↓
点击文件 → git diff [--cached] -- <path>  → 渲染 unified diff
        ↓
stage/unstage → git add / git restore --staged
        ↓
commit → git commit -F -  (message 走 stdin，避免 shell escaping)
        ↓
push → git push  (跟随 upstream)
```

### 关键类型（建议草案）

```rust
pub struct WorkspaceGitState {
    pub root: PathBuf,
    pub branch: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub files: Vec<GitFileEntry>,
    pub last_polled: Instant,
}

pub struct GitFileEntry {
    pub path: PathBuf,
    pub index_status: FileStatus,   // staged
    pub worktree_status: FileStatus, // unstaged
}

pub enum FileStatus {
    Unmodified,
    Modified,
    Added,
    Deleted,
    Renamed { from: PathBuf },
    Untracked,
    Ignored,
    Conflict,
}

pub enum DiffPanelMode {
    Hidden,
    FileList,
    Diff { path: PathBuf, staged: bool },
}
```

### 后台轮询

- 在 `gpui_entry.rs` 中已有的 background task 框架里加一个 git status 轮询任务
- 默认 2 秒一次；workspace 切换时立即触发一次
- `git status` 失败（不是 repo / 命令缺失）→ 静默忽略，sidebar 徽标隐藏
- 命令超时（>3s）→ 跳过本轮，不阻塞 UI

### Diff 渲染

- 复用 `gpui_preview.rs` 已经接入的 syntax highlighter（14 语言）
- Unified diff 解析：自己写一个最小 parser（`@@` hunk header + `+`/`-`/` ` 前缀），不引第三方 crate
- 高亮：`+` 行底色用 Tomorrow Night green 的 20% alpha，`-` 行用 red 的 20% alpha
- hunk header 用 Tomorrow Night blue
- 大 diff（>2000 行）→ 默认折叠到前 200 行 + 展开按钮

### 写操作保护

- **Commit**: 空消息 → 按钮禁用；消息含非 ASCII 或换行 → 走 `git commit -F -` stdin，不走 `-m`
- **Push**: 弹一行确认 footer（`Push 3 commits to origin/main? [y/N]`），快捷键 `y` 或点击确认；不做后台静默 push
- **Stage/Unstage**: 不需要确认（可撤销）；按 `u` 撤销最近一次 stage 动作（先不做撤销栈，V1 略过）

## UX 草图

```
┌─────────────────────────────────────────────────────────────┐
│ Workspaces       │  Terminal Pane    │  Diff Panel (Ctrl+G) │
│                  │                   │                       │
│ ● amux         3 │  $ claude         │  Changes (3)          │
│ ○ side-quest   ↑1│  ...              │  ├─ M src/main.rs     │
│ ○ docs       ✓   │                   │  ├─ A docs/diff.md    │
│                  │                   │  └─ M Cargo.toml      │
│                  │                   │  ───────────────────  │
│                  │                   │  @@ -1,5 +1,7 @@      │
│                  │                   │   use foo;            │
│                  │                   │  +use bar;            │
│                  │                   │   ...                 │
│                  │                   │  ───────────────────  │
│                  │                   │  [✓ Stage all]        │
│                  │                   │  Message: _______     │
│                  │                   │  [Commit] [Push ↑1]   │
└─────────────────────────────────────────────────────────────┘
```

Sidebar 徽标说明：
- `✓` clean
- `N` 数字 = uncommitted changes 数量
- `↑N` ahead；`↓N` behind；`↑N ↓N` 双向 diverged

## 实现路径

| 阶段 | 工作 | 验收 |
|---|---|---|
| Day 1-3 | cwd → repo root 检测 + `git status` 后台轮询 + sidebar 徽标渲染 | 切换 workspace 看到正确的 clean / N changes 徽标 |
| Day 4-7 | `gpui_preview.rs` 加 `DiffMode`，渲染 unified diff（先纯文本，再加 syntax highlight） | 点击文件能看到 diff，颜色正确 |
| Day 8-10 | stage/unstage（文件级）+ commit message 输入 + `git commit -F -` | 完整跑通"改动 → stage → commit"循环 |
| Day 11-12 | push 按钮 + 错误显示 + 二次确认 | push 成功后徽标更新为 `✓` |
| Day 13-14 | 快捷键 `Ctrl/Cmd+G` 注册 + 文档 + 跨平台测试（Win/macOS/Linux） | 三平台都能跑通完整流程 |

## 验收标准

跑一个 agent 在 workspace 里改 5 个文件 → 用户不离开 Amux 就能 review diff → commit → push，**全程不超过 30 秒**，且 Amux 二进制体积**增加不超过 500KB**。

## 风险与缓解

| 风险 | 缓解 |
|---|---|
| `git status` 轮询在大 repo 上慢 | 用 `--porcelain=v2 --untracked-files=normal --no-renames`；超过 3s 跳过本轮 |
| Windows 上路径分隔符 / WSL 路径混乱 | 复用 `amux-platform` 的 path mapping；diff 显示用 repo-relative POSIX 路径 |
| Agent 同时在多个 workspace 改文件，sidebar 徽标抖动 | 轮询结果用 `WorkspaceGitState.last_polled` 节流；UI 渲染走 diff 比较，不全量重绘 |
| 用户在 detached HEAD / rebase 进行中 | sidebar 徽标显示 `rebase` / `detached`；diff panel 仍可看，但 commit 按钮禁用并提示 |
| Commit message 含特殊字符（emoji / 多语言） | 走 `git commit -F -` stdin，不走 `-m`，避免 shell escaping 问题 |

## 不在本文档讨论

- 分支管理 UI（明确 out of scope）
- PR 创建流程（留给 `gh`）
- Conflict resolution（留给外部工具）
- Multi-repo workspace 支持（V2+ 再议）

## 参考

- `docs/clickable-paths-design.md` — 类似的 panel 扩展思路
- `crates/amux-platform/src/terminal/osc_intercept.rs` — OSC 7 cwd 来源
- `apps/desktop/src/gpui_preview.rs` — 复用的 preview panel 实现
