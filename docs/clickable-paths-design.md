# 可点击文件路径设计 — 借鉴 osc8wrap

## 背景

Claude Code / Codex / Aider 等 AI Agent 在输出中频繁引用文件路径，但这些路径只是纯文本，用户无法直接点击打开。当前 Amux 的 Ctrl+Click 方案通过字符扫描提取路径，但可靠性低——路径格式千变万化，字符扫描经常提取到错误内容。

`osc8wrap` 项目（third_party/osc8wrap）提供了一个成熟的解决方案，本文分析其思路并设计 Amux 的实现方案。

## osc8wrap 的核心思路

### 工作流程

```
程序输出 → PTY → osc8wrap 拦截输出流
                     ↓
              ANSI tokenizer 分离文本和转义码
                     ↓
              正则匹配文件路径/URL
                     ↓
              验证文件存在性（内存索引）
                     ↓
              包裹成 OSC 8 超链接转义码
                     ↓
              终端渲染 → 路径变成可点击链接
```

### 路径匹配正则

osc8wrap 用一个复合正则覆盖所有常见格式：

| 模式 | 示例 |
|------|------|
| 绝对路径 | `/path/to/file.go` |
| Home 路径 | `~/src/project/main.go` |
| 相对路径 | `./src/main.go:10` |
| 带行号 | `/path/to/file.go:42` |
| 带行列号 | `/path/to/file.go:42:10` |
| 带行范围 | `/path/to/file.go:10-20` |
| 无前缀有扩展名 | `main.go:10`、`auth.rs` |
| 特殊文件名 | `Makefile`、`Dockerfile` |
| Git diff 路径 | `a/src/main.go`、`b/src/main.go` |
| HTTPS URL | `https://example.com/docs` |

### Basename 解析

当 `main.go:10` 在当前目录不存在时，osc8wrap 在后台文件索引中搜索匹配的文件：

1. 启动时异步遍历项目目录，建立文件索引（跳过 vendor/node_modules/.git 等）
2. Git 仓库自动排除 `.gitignore` 中的路径
3. 按 basename 匹配，再按路径后缀过滤
4. 多个匹配时选最近修改的文件

### ANSI 感知

osc8wrap 有完整的 ANSI tokenizer（ansi_tokenizer.go），将输出流分离为：
- `TokenText`：纯文本（做路径匹配）
- `TokenSGR`：颜色/样式控制码（透传不修改）
- `TokenOSC8`：已有的超链接（透传不重复处理）
- 其他转义码（透传）

### Symbol 链接

在 SGR 样式文本中（如 Claude 输出的彩色标识符），识别 3+ 字符的标识符名称，链接到编辑器的符号定义。需要配合 VS Code 扩展。

## 方案对比

### 方案 A：PTY 输出流注入（osc8wrap 方式）

在 PTY read 循环里拦截输出数据，注入 OSC 8 转义码。

**优点**：
- 和 osc8wrap 一样的成熟方案
- alacritty_terminal 已支持 OSC 8 渲染

**风险**：
- **内容篡改**：修改了 PTY 数据流，如果处理不当会破坏终端内容
- **程序兼容性**：vim/less/fzf 等程序可能对注入的 OSC 8 码反应异常
- **转义码损坏**：正则匹配到一半的转义码会导致花屏
- **调试困难**：问题出现时很难确定是原始输出的问题还是注入导致的

### 方案 B：渲染层后处理（推荐）

不修改 PTY 数据，只在渲染到屏幕时标记超链接。

**原理**：

```
PTY 输出 → alacritty_terminal 解析 → Grid cells (完全不修改)
                                          ↓
                                     渲染前后处理
                                          ↓
                              对每行文本做正则匹配
                              匹配到的 cells 标记 hyperlink
                                          ↓
                              GPUI 渲染：有 hyperlink 的 cells
                              显示下划线 + 颜色 + 点击处理
```

**优点**：
- **零内容篡改**：PTY 数据完全不修改，终端程序看到的数据完全正常
- **零兼容性问题**：vim/less/fzf 等程序不受任何影响
- **最坏情况可控**：正则匹配错误最多导致多了个下划线，不会花屏
- **可随时开关**：纯视觉层面的增强，用户可配置关闭

**缺点**：
- 每次渲染需要对可见行做正则匹配（但只处理可见区域，通常 30-50 行）
- 需要在 gpui_terminal.rs 的 collect_render_data 中添加后处理逻辑

### 方案选择：B（渲染层后处理）

理由：安全性远重于性能。PTY 数据是用户的生命线，不能冒篡改风险。

## 实现设计

### 架构

```
crates/amux-platform/src/terminal/
  path_linker.rs (新文件)
    ├─ PathPattern (编译后的正则)
    ├─ FileIndex (后台文件索引)
    └─ scan_line(text: &str) -> Vec<PathMatch>

apps/desktop/src/gpui_terminal.rs
  collect_render_data()
    └─ 对每行可见文本调用 scan_line()
    └─ 匹配到的 cell 设置 hyperlink_url
```

### 核心组件

#### 1. PathPattern — 路径匹配

移植 osc8wrap 的正则到 Rust：

```rust
use regex::Regex;

pub struct PathPattern {
    pattern: Regex,
}

impl PathPattern {
    pub fn new() -> Self {
        // 移植 osc8wrap 的 buildPattern() 正则
        let pattern = concat!(
            r"(https?://[^\s<>\"'`\x00-\x1f\x7f]+)",           // URL
            r"|(?:^|[^/\w.\-])",
            r"((?:~|\.{0,2})/[\w./%+@-]+(?:\.\w+)?",           // 有前缀路径
            r"|[\w./%+@-]+\.\w+",                                // 有扩展名
            r"|\w+file)",                                        // *file
            r"(:\d+(?:[-:]\d+)?)?",                              // :line:col
        );
        Self { pattern: Regex::new(pattern).unwrap() }
    }

    pub fn scan_line(&self, text: &str) -> Vec<PathMatch> { ... }
}
```

#### 2. FileIndex — 后台文件索引

```rust
pub struct FileIndex {
    files: HashMap<String, Vec<PathBuf>>,  // basename → full paths
    ready: AtomicBool,
}

impl FileIndex {
    /// 异步构建索引（后台线程）
    pub fn build_async(root: &Path, exclude: &[&str]) -> Arc<FileIndex> { ... }

    /// basename 查找，返回最近修改的匹配文件
    pub fn resolve_basename(&self, name: &str, suffix: &str) -> Option<PathBuf> { ... }
}
```

#### 3. 渲染集成

在 `collect_render_data` 或 `prepaint_terminal` 中：

```rust
// 对每行可见文本做路径匹配
for row in 0..rows {
    let line_text: String = grid_row_to_string(grid, row);
    let matches = path_linker.scan_line(&line_text);
    for m in matches {
        // 设置匹配范围内的 cells 的 hyperlink_url
        for col in m.start_col..m.end_col {
            grid[row][col].hyperlink_url = Some(m.resolved_path.clone());
        }
    }
}
```

#### 4. 点击处理

在 `gpui_entry.rs` 中，Ctrl+Click 时检查 cell 的 `hyperlink_url`：

```rust
if event.modifiers.control {
    let (col, row) = pixel_to_term_cell(event.position);
    if let Some(url) = get_cell_hyperlink(col, row) {
        open_preview_file(&url);  // 或用系统默认程序打开
        return;
    }
}
```

### 性能考虑

| 环节 | 开销 | 优化 |
|------|------|------|
| 正则匹配 | ~0.1ms/行 | 只匹配可见行（30-50行），不匹配滚动历史 |
| 文件存在性检查 | ~0.01ms/次 | 用 FileIndex 内存查找，不做 syscall |
| FileIndex 构建 | 100-500ms | 后台线程异步构建，不阻塞启动 |
| 渲染标记 | 极小 | 只设置 cell 属性，不额外分配内存 |

**总额外开销**：每帧 ~5ms（50 行 × 0.1ms），对 60fps 渲染无感知影响。

### 配置

```toml
# ~/.amux/config.toml

[clickable_paths]
enabled = true
# 是否启用 basename 解析（在大项目中可能需要关闭）
resolve_basename = true
# 排除目录
exclude_dirs = ["vendor", "node_modules", ".git", "__pycache__", ".cache", "target"]
# 点击行为："preview" (Amux 内预览) 或 "editor" (系统默认编辑器)
click_action = "preview"
```

## 与当前 Ctrl+Click 方案的对比

| 维度 | 当前方案（字符扫描） | 新方案（渲染层正则） |
|------|-------------------|-------------------|
| 匹配准确性 | 低（逐字符扫描，易受特殊字符干扰） | 高（正则覆盖所有常见格式） |
| 路径解析 | 简单（只尝试绝对/CWD/git root） | 完整（+ basename 索引搜索） |
| 视觉反馈 | 无（点击才知道能不能打开） | 有（路径显示下划线，表示可点击） |
| 内容安全 | 安全（不修改 PTY） | 安全（不修改 PTY） |
| 性能 | 无额外开销（只在点击时执行） | 微小额外开销（每帧正则匹配可见行） |
| 覆盖格式 | 有限 | 全面 |

## 实现优先级

1. **PathPattern 正则匹配器** — 移植 osc8wrap 的正则，覆盖主要路径格式
2. **渲染层标记** — 在 collect_render_data 中标记 hyperlink cells
3. **点击处理** — Ctrl+Click 读取 cell hyperlink 打开文件
4. **视觉反馈** — 有 hyperlink 的 cells 显示下划线
5. **FileIndex 后台索引** — basename 解析支持
6. **配置化** — config.toml 控制开关和行为

## 与 osc8wrap 的关系

Amux 不需要集成 osc8wrap 本身（它是外部 Go 程序）。我们借鉴它的：
- **正则表达式设计** — 经过实战验证的路径匹配模式
- **Basename 解析逻辑** — 文件索引 + 后缀匹配 + 最近修改优先
- **ANSI 感知** — 在样式文本中正确识别路径

但采用不同的架构：
- osc8wrap 在 PTY 流中注入 OSC 8（侵入性）
- Amux 在渲染层标记（非侵入性，更安全）
