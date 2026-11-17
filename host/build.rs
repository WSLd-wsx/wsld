fn main() {
    println!("cargo:rerun-if-changed=resources/host.rc");
    println!("cargo:rerun-if-changed=resources/icon.ico");
    println!("cargo:rerun-if-changed=resources/host.manifest");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("host.res");
    let status = std::process::Command::new("rc.exe")
        .arg("/nologo")
        .arg("/fo")
        .arg(&out)
        .arg("resources/host.rc")
        .status()
        .expect("Windows resource compiler rc.exe is required");
    assert!(status.success(), "Host resource compilation failed");
    println!("cargo:rustc-link-arg={}", out.display());
}
