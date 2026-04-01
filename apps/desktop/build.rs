fn main() {
    // Re-run this build script when the icon file changes
    println!("cargo:rerun-if-changed=../../assets/icons/amux.ico");
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
