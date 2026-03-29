# AMUX

Windows-first AI coding workspace for terminals, agents, and project surfaces.

See [plans/amux-technical-design.md](plans/amux-technical-design.md) for the current technical design baseline.


# 禁用增量编译
$env:CARGO_INCREMENTAL=0
# 增加栈空间（预防万一）
$env:RUST_MIN_STACK=134217728

cargo build --release --features gpui -j 1

cargo build --release --target x86_64-pc-windows-gnu -p amux-desktop --features gpui -j 1 -v