# 构建问题汇总

## 问题描述

在 Windows 上使用 `cargo build --release -p amux-desktop --features gpui` 编译时遇到以下错误：

```
error[E0080]: scalar size mismatch: expected 140715025335504 bytes but got 4 bytes instead
```

该错误出现在 `stacksafe-macro` 编译过程中，同时还会遇到：
- `internal compiler error (ICE)`
- `miniz_oxide`, `zmij`, `shlex`, `windows-sys` 等依赖编译失败

## 根因分析

这是一个 Rust 编译器在 **Windows** 平台上的 **常量求值 bug**，影响了以下版本：

- Rust 1.88.x
- Rust 1.89.x
- Rust 1.90.x
- Rust 1.91.x
- Rust 1.92.x
- Rust 1.93.x (仅 debug 模式可用)
- Rust 1.94.x (完全不可用)

该 bug 与以下依赖的编译冲突有关：
- `stacksafe-macro` v0.1.4
- `stacksafe` v0.1.4
- `image-webp`, `resvg`, `moxcms` 等使用复杂常量求值的 crate

项目使用了 Rust Edition 2024，需要 Rust 1.85+，但 1.85-1.94 在 Windows 上存在此编译器 bug。

## 解决方案

### 方案一：使用 Rust 1.93 (推荐)

1. **安装 Rust 1.93**
   ```bash
   rustup install 1.93.0
   ```

2. **配置项目使用 Rust 1.93**
   
   修改 `rust-toolchain.toml`：
   ```toml
   [toolchain]
   channel = "1.93.0"
   components = ["rustfmt", "clippy"]
   ```

3. **构建时使用单线程**
   
   由于编译器 bug，使用 `-j 1` 避免并行编译导致的不稳定行为：
   ```bash
   cargo build --release -p amux-desktop --features gpui -j 1
   ```

### 方案二：等待上游修复

该问题已被报告给 Rust 团队，预计在后续版本中修复。可关注：
- [Rust GitHub Issues](https://github.com/rust-lang/rust/issues)

## 验证结果

| Rust 版本 | Debug 模式 | Release 模式 | 备注 |
|-----------|------------|--------------|------|
| 1.85.x    | ❌         | ❌           | MSRV 不足 |
| 1.86.x    | ❌         | ❌           | 依赖要求更高版本 |
| 1.87.x    | ❌         | ❌           | 同上 |
| 1.88.x    | ❌         | ❌           | 编译器 bug |
| 1.89.x    | ❌         | ❌           | 编译器 bug |
| 1.90.x    | ❌         | ❌           | 编译器 bug |
| 1.91.x    | ❌         | ❌           | 编译器 bug |
| 1.92.x    | ❌         | ❌           | 编译器 bug |
| 1.93.x    | ✅         | ✅           | 需要 -j 1 |
| 1.94.x    | ❌         | ❌           | 编译器 bug 严重 |

## 附录：相关依赖

- `gpui` v0.2.2 (Zed Industries)
- `stacksafe-macro` v0.1.4
- `stacksafe` v0.1.4

## 更新日志

- **2026-03-28**: 初始文档创建，确认 Rust 1.93 + `-j 1` 为可行方案
