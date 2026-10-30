use std::path::PathBuf;

fn main() {
    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .parent()
        .unwrap()
        .to_path_buf();
    let payload = std::env::var("CRAB_PAYLOAD")
        .map(PathBuf::from)
        .unwrap_or_else(|_| root.join("target").join("release").join("claude_crab.exe"));
    if !payload.exists() {
        panic!(
            "App-Exe fehlt: {}\nErst die App bauen (build_installer.bat macht beides).",
            payload.display()
        );
    }
    println!("cargo:rerun-if-env-changed=CRAB_PAYLOAD");
    println!("cargo:rerun-if-changed={}", payload.display());
    println!("cargo:rustc-env=CRAB_PAYLOAD={}", payload.display());
    println!("cargo:rerun-if-changed=../assets/claude_crab.ico");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../assets/claude_crab.ico");
        res.set("ProductName", "Claude Crab Setup");
        res.set("FileDescription", "Claude Crab Setup");
        res.set("OriginalFilename", "ClaudeCrab-Setup.exe");
        res.set("LegalCopyright", "MIT License");
        res.set_manifest(
            r#"<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
<trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
  <security><requestedPrivileges>
    <requestedExecutionLevel level="asInvoker" uiAccess="false"/>
  </requestedPrivileges></security>
</trustInfo>
</assembly>"#,
        );
        if let Err(e) = res.compile() {
            println!("cargo:warning=Ressourcen konnten nicht eingebettet werden: {e}");
        }
    }
}
