fn main() {
    println!("cargo:rerun-if-changed=assets/claude_crab.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/claude_crab.ico");
        res.set("ProductName", "Claude Crab");
        res.set("FileDescription", "Claude Crab");
        res.set("InternalName", "claude_crab");
        res.set("OriginalFilename", "claude_crab.exe");
        res.set("LegalCopyright", "MIT License");
        if let Err(e) = res.compile() {
            println!("cargo:warning=Icon konnte nicht eingebettet werden: {e}");
        }
    }
}
