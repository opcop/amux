fn main() {
    // Re-run this build script when icon files change. We list both
    // the Windows .ico (consumed below by winresource) and the macOS
    // .jpg (consumed at runtime by gpui_entry::set_macos_dock_icon
    // via include_bytes!). The macOS path doesn't actually need
    // build.rs to do anything — `include_bytes!` causes rustc to
    // re-build the source file when the asset changes — but listing
    // it here also keeps it consistent in incremental rebuilds when
    // build.rs itself is re-evaluated for unrelated reasons.
    println!("cargo:rerun-if-changed=../../assets/icons/amux.ico");
    println!("cargo:rerun-if-changed=../../assets/icons/amux.jpg");
    println!("cargo:rerun-if-changed=build.rs");

    #[cfg(target_os = "windows")]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../assets/icons/amux.ico");
        res.set("ProductName", "AMUX");
        res.set("FileDescription", "AMUX Terminal Multiplexer");
        if let Err(e) = res.compile() {
            eprintln!("cargo:warning=Failed to set Windows icon: {}", e);
        }
    }
}
