use std::{env, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=../integration/graphics-opaque-probe");
    println!("cargo:rerun-if-env-changed=WSX_RUNTIME_CATALOG");
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("output directory"));
    let catalog = if let Some(path) = env::var_os("WSX_RUNTIME_CATALOG") {
        let path = PathBuf::from(path);
        println!("cargo:rerun-if-changed={}", path.display());
        let bytes = std::fs::read(&path).expect("read trusted Native runtime catalog");
        assert!(bytes.len() <= 65536, "Native runtime catalog exceeds size limit");
        bytes
    } else {
        br#"{"schema_version":1,"bundles":[]}"#.to_vec()
    };
    std::fs::write(out.join("trusted-runtime-catalog.json"), catalog).expect("write Native runtime catalog");
    println!("cargo:rerun-if-env-changed=WSX_GUEST_ADAPTER_CATALOG");
    let adapters = if let Some(path) = env::var_os("WSX_GUEST_ADAPTER_CATALOG") {
        let path = PathBuf::from(path);
        println!("cargo:rerun-if-changed={}", path.display());
        let bytes = std::fs::read(path).expect("read trusted Guest adapter catalog");
        assert!(bytes.len() <= 65536, "Guest adapter catalog exceeds size limit");
        bytes
    } else {
        br#"{"schema_version":1,"adapters":[]}"#.to_vec()
    };
    std::fs::write(out.join("trusted-guest-adapter-catalog.json"), adapters).expect("write Guest adapter catalog");
    println!("cargo:rerun-if-env-changed=CC");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") { return; }
    if env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("msvc")
        || env::var("CARGO_CFG_TARGET_ARCH").as_deref() != Ok("x86_64") {
        panic!("The embedded opaque graphics probe requires Windows x64 MSVC");
    }
    let identity = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest directory"))
        .join("../integration/graphics-identity");
    let headers = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest directory"))
        .join("../integration/graphics-opaque-probe/venus-protocol");
    println!("cargo:rerun-if-changed={}", identity.display());
    for name in ["display-inventory", "vulkan-identity"] {
        let status = Command::new(env::var_os("CC").unwrap_or_else(|| "cl.exe".into()))
            .current_dir(&out).args(["/nologo", "/std:c11", "/W3", "/O2", "/TC", "/c"])
            .arg(format!("/I{}", headers.display())).arg(identity.join(format!("{name}.c")))
            .arg(format!("/Fo{}", out.join(format!("{name}.obj")).display()))
            .status().expect("start MSVC for graphics inventory");
        assert!(status.success(), "MSVC failed to compile graphics inventory");
    }
    let library = out.join("wsx-graphics-identity.lib");
    let status = Command::new(env::var_os("AR").unwrap_or_else(|| "lib.exe".into()))
        .current_dir(&out).arg("/nologo").arg(format!("/OUT:{}", library.display()))
        .arg(out.join("display-inventory.obj")).arg(out.join("vulkan-identity.obj"))
        .status().expect("start MSVC librarian for graphics inventory");
    assert!(status.success(), "MSVC failed to archive graphics inventory");
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=wsx-graphics-identity");
    println!("cargo:rustc-link-lib=user32");
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest directory"))
        .join("../integration/graphics-opaque-probe");
    let cc = env::var_os("CC").unwrap_or_else(|| "cl.exe".into());
    let status = Command::new(cc).current_dir(&out)
        .args(["/nologo", "/std:c11", "/W3", "/O2", "/TC"])
        .arg(format!("/I{}", root.display()))
        .arg(root.join("check-buffer.c"))
        .arg(format!("/Fe:{}", out.join("opaque-buffer-probe.exe").display()))
        .args(["/link", "advapi32.lib"])
        .status().expect("start MSVC for embedded opaque probe");
    assert!(status.success(), "MSVC failed to build embedded opaque probe");
    let status = Command::new(env::var_os("CC").unwrap_or_else(|| "cl.exe".into())).current_dir(&out)
        .args(["/nologo", "/std:c11", "/W3", "/O2", "/TC"])
        .arg(format!("/I{}", root.display()))
        .arg(root.join("staging-capability.c"))
        .arg(format!("/Fe:{}", out.join("staging-capability-probe.exe").display()))
        .args(["/link", "advapi32.lib"])
        .status().expect("start MSVC for embedded staging capability probe");
    assert!(status.success(), "MSVC failed to build embedded staging capability probe");
}
