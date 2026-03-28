# AMUX 近期开发任务拆分

## 1. 文档目的

这份文档是对 [developer-handoff.md](/mnt/d/repository/arden/Ai/ide/amux/plans/developer-handoff.md) 的执行补充。

前一份文档回答的是：

- 项目为什么这样设计
- 当前做到了哪里
- 下一位开发者应该如何理解系统

这一份文档回答的是：

- 现在应该先做什么
- 每个任务的范围是什么
- 做完以后如何验证
- 哪些改动是允许的，哪些不应该碰

目标是让接手者能够直接拿一个任务开始开发，而不是重新自己拆阶段。

---

## 2. 当前阶段判断

项目现在不再适合继续做“泛泛的骨架铺设”，而应该进入：

`在现有稳定架构上，用小步迭代把 mock 能力替换成真实能力。`

因此接下来所有任务都应满足三个条件：

1. 对现有抽象友好
2. 可以单独验证
3. 不引入大面积返工

---

## 3. 任务优先级总表

按建议优先级，近期任务顺序如下：

1. `T1` 命令面板可用化
2. `T2` GPUI 视图模块继续拆分
3. `T3` 真实文件系统 backend 接入
4. `T4` 真实 terminal backend spike
5. `T5` editor / preview surface 真实能力增强

其中：

- `T1` 最适合快速推进产品可用性
- `T4` 风险最高，但决定产品上限

---

## 4. T1 命令面板可用化

### 4.1 目标

把当前“静态命令列表 + 点击执行”的 command palette，推进成真正可用的统一动作入口。

### 4.2 当前状态

当前 palette 已具备：

- 弹层
- 命令列表展示
- 点击执行
- 通过 `DesktopApp::run_command(...)` 走统一命令路由

当前缺失：

- query 状态
- 输入框
- 过滤能力
- 键盘导航
- 高优命令排序

### 4.3 建议实施顺序

第一步：

- 在 `amux-ui` 中为 palette 增加 query 状态
- 不要把 query 状态放到 `apps/desktop`

第二步：

- 在 `gpui` 视图里增加最小输入框
- 先做到“显示 query + 修改 query”

第三步：

- 对命令列表做前缀过滤或包含过滤
- 只过滤显示层，不改 command router

第四步：

- 增加高频命令置顶
- 可选按分组展示：
  - workspace
  - pane
  - agent
  - file

第五步：

- 增加键盘导航：
  - 上下选择
  - enter 执行
  - esc 关闭

### 4.4 范围边界

这个任务里不要做：

- 模糊搜索算法优化
- 命令历史系统
- 最近使用排序学习
- 命令参数补全

先把“可用”做出来，不要过早做“聪明”。

### 4.5 建议改动文件

- [crates/amux-ui/src/state.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-ui/src/state.rs)
- [crates/amux-ui/src/controller.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-ui/src/controller.rs)
- [crates/amux-ui/src/render/gpui.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-ui/src/render/gpui.rs)
- [apps/desktop/src/gpui_command_palette.rs](/mnt/d/repository/arden/Ai/ide/amux/apps/desktop/src/gpui_command_palette.rs)
- [apps/desktop/src/gpui_entry.rs](/mnt/d/repository/arden/Ai/ide/amux/apps/desktop/src/gpui_entry.rs)

### 4.6 验收标准

- 可在 palette 中看到当前 query
- 可根据 query 过滤命令
- 可以点击执行过滤结果
- 打开/关闭 palette 后状态符合预期
- 不影响现有 command bar 和 command router

### 4.7 验证命令

```bash
cargo test -p amux-ui
cargo check -p amux-desktop --features gpui
```

---

## 5. T2 GPUI 视图模块继续拆分

### 5.1 目标

避免 [gpui_entry.rs](/mnt/d/repository/arden/Ai/ide/amux/apps/desktop/src/gpui_entry.rs) 再次膨胀，继续把窗口里的各类面板拆成独立视图模块。

### 5.2 当前状态

目前已经拆出去的模块有：

- active surface 视图
- status bar
- command bar
- command palette

仍留在 `gpui_entry.rs` 的部分有：

- workspace panel
- agent panel
- file panel
- open files panel
- active tabs panel
- pane panel
- toolbar
- metric cards

### 5.3 建议拆分顺序

第一批先拆：

- `workspace/agent/file` 左侧导航面板
- `open files / active tabs / panes` 主区辅助面板

第二批再拆：

- toolbar
- metric cards

### 5.4 建议新模块

- `apps/desktop/src/gpui_sidebar_panels.rs`
- `apps/desktop/src/gpui_workspace_panels.rs`

命名不用完全照搬，但建议按“功能分区”而不是“单函数工具箱”来拆。

### 5.5 范围边界

这个任务里不要顺便改业务逻辑。

只做：

- 渲染函数迁移
- 交互回调保持不变
- 入口文件职责收敛

### 5.6 验收标准

- `gpui_entry.rs` 明显缩短
- 提取后的模块职责清晰
- 所有现有交互仍可工作

### 5.7 验证命令

```bash
cargo check -p amux-desktop --features gpui
```

---

## 6. T3 真实文件系统 backend 接入

### 6.1 目标

把当前主要依赖 `InMemoryFsBackend` 的 workspace/file 流程，逐步接到真实文件系统上。

### 6.2 当前状态

现在文件工作流已经走通，但 demo 依赖 mock 文件系统注入。

这意味着：

- UI 流程已验证
- service 接口已验证
- 真正缺的是 backend 替换

### 6.3 建议实施顺序

第一步：

- 在 `amux-platform/src/fs.rs` 新增 `RealFsBackend`
- 只实现：
  - `list_dir`
  - `read_to_string`
  - `write_string`

第二步：

- 在 `DesktopApp` / `AppController` 初始化路径上允许选择 real fs
- demo 模式仍保留 mock fs

第三步：

- 优先支持 `WindowsPath`
- `WslPath` 的真实访问可以先走 UNC 或后置

第四步：

- 让 `workspace open <path>` 后能够读取真实目录

### 6.4 范围边界

不要在这一阶段同时做：

- 文件监听
- 大目录性能优化
- `.gitignore` 深度支持
- 复杂递归缓存

先把真实读写能力接通。

### 6.5 建议改动文件

- [crates/amux-platform/src/fs.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-platform/src/fs.rs)
- [crates/amux-workspace/src/manager.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-workspace/src/manager.rs)
- [crates/amux-ui/src/controller.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-ui/src/controller.rs)
- [crates/amux-ui/src/root.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-ui/src/root.rs)

### 6.6 验收标准

- 可以打开一个真实目录 workspace
- file list 来自真实文件系统
- 打开文件显示真实内容
- 保存文件可以落盘
- mock fs 仍可用于测试

### 6.7 验证建议

```bash
cargo test -p amux-workspace -p amux-ui
cargo run -p amux-desktop --features gpui -- "workspace open /some/path"
```

---

## 7. T4 真实 terminal backend spike

### 7.1 目标

验证并建立真实 terminal 会话能力，这是未来 Windows-first 产品化的关键路径。

### 7.2 当前状态

terminal 相关功能当前依赖：

- `TerminalBackend` 抽象
- `InMemoryTerminalBackend`
- Windows/WSL command planning skeleton

缺失的是：

- 真实子进程生命周期
- 真实 IO 读取
- 真实 resize
- 真实会话 metadata 更新

### 7.3 实施原则

这一任务不要和大量 UI 改造并行。

优先顺序应是：

1. 先做平台层 spike
2. 再做 controller 注入
3. 最后才做真正 terminal surface view

### 7.4 最小目标

先只验证：

- Windows 本地 shell 启动
- WSL shell 启动
- 写入输入
- 读取输出
- 关闭会话

### 7.5 范围边界

不要在 spike 阶段做：

- 终端光标渲染
- ANSI 全量处理
- 滚屏缓存系统
- 多平台统一高级抽象清理

先回答一个问题：

`真实 terminal backend 能不能稳定工作？`

### 7.6 建议改动文件

- [crates/amux-platform/src/terminal.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-platform/src/terminal.rs)
- [crates/amux-platform/src/windows/conpty.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-platform/src/windows/conpty.rs)
- [crates/amux-platform/src/windows/wsl.rs](/mnt/d/repository/arden/Ai/ide/amux/crates/amux-platform/src/windows/wsl.rs)

### 7.7 验收标准

- 有一条真实 terminal session 可以被创建
- 可以写入命令并拿到输出
- 可以关闭
- 不破坏现有 mock backend

### 7.8 验证建议

优先补测试或小型 demo，而不是先接主 UI。

---

## 8. T5 Editor / Preview surface 增强

### 8.1 目标

把当前只读内容块推进到更像真实内容面板，但仍然不一开始实现完整编辑器。

### 8.2 当前状态

当前 `editor/preview` 已具备：

- surface 级专用视图
- summary lines
- content lines
- markdown / plaintext 基础分化

### 8.3 可做内容

- editor header 更明确
- 更稳定的内容分段
- 更好的 markdown 展示
- 预览内容与文件内容同步刷新

### 8.4 不建议现在做

- 完整编辑输入
- 复杂光标模型
- 撤销重做
- LSP

### 8.5 验收标准

- editor/preview 看起来更像独立 surface
- 内容组织更稳定
- 不破坏当前 snapshot 模型

---

## 9. 推荐执行顺序

如果由一个人连续推进，建议按下面顺序执行：

1. 完成 `T1`
2. 完成 `T2`
3. 评估是否切 `T3` 或 `T4`

建议判断标准：

- 如果目标是尽快提升 demo 可用性：先做 `T3`
- 如果目标是尽快逼近产品核心壁垒：先做 `T4`

---

## 10. 每次任务开始前的检查清单

每次开始开发前，先确认：

1. 是否会改动 `amux-core` 的核心语义
2. 是否会破坏 `WorkspaceTarget` 抽象
3. 是否会把业务逻辑塞进 `gpui_entry.rs`
4. 是否需要兼容旧 session 数据
5. 是否保留 mock backend 作为测试后路

如果其中任意一项答案不清楚，先回到 `developer-handoff.md` 重新对齐。

---

## 11. 当前最推荐的下一步

如果你现在就开始开发，我建议直接拿：

`T1 命令面板可用化`

原因很简单：

- 风险低
- 反馈快
- 能直接改善 GPUI 原型的使用方式
- 不会影响后续真实 terminal/backend 接入

做完 `T1` 以后，再决定是继续 `T2/T3`，还是切去做 `T4`。
