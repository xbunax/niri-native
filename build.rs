use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn native_spaces_bridge() {
    println!("cargo:rerun-if-changed=native-spaces/bridge.m");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let object = out.join("rift_native_spaces.o");
    let archive = out.join("librift_native_spaces.a");
    let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    let arch = if arch == "aarch64" { "arm64" } else { &arch };
    assert!(
        Command::new("xcrun")
            .args([
                "--sdk",
                "macosx",
                "clang",
                "-arch",
                arch,
                "-fobjc-arc",
                "-Wall",
                "-Wextra",
                "-Werror",
                "-c",
                "native-spaces/bridge.m",
                "-o"
            ])
            .arg(&object)
            .status()
            .expect("run macOS clang")
            .success()
    );
    assert!(
        Command::new("xcrun")
            .args(["ar", "crs"])
            .arg(&archive)
            .arg(&object)
            .status()
            .expect("run macOS ar")
            .success()
    );
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=rift_native_spaces");
    println!("cargo:rustc-link-lib=framework=Foundation");
}

fn main() {
    native_spaces_bridge();
    for path in ["--git-dir", "--git-common-dir"] {
        if let Some(dir) = git(&["rev-parse", path]) {
            for entry in ["HEAD", "refs", "packed-refs", "index"] {
                println!("cargo:rerun-if-changed={dir}/{entry}");
            }
        }
    }
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=Cargo.lock");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets");
    println!("cargo:rerun-if-changed=crates");
    let version = std::env::var("CARGO_PKG_VERSION").unwrap();

    let plist = std::fs::read_to_string("assets/Info.plist")
        .unwrap()
        .replace("@VERSION@", &version);
    std::fs::write(
        std::path::Path::new(&std::env::var_os("OUT_DIR").unwrap()).join("Info.plist"),
        plist,
    )
    .unwrap();

    let release_tag = format!("v{version}");
    let tagged = git(&["tag", "--points-at", "HEAD"])
        .is_some_and(|tags| tags.lines().any(|tag| tag == release_tag || tag == version));
    let dirty = git(&["diff", "HEAD", "--quiet"]).is_none();
    let display_version = if (tagged && !dirty)
        || git(&["rev-parse", "--is-inside-work-tree"]).is_none()
    {
        version
    } else {
        let commit = git(&["rev-parse", "--short=12", "HEAD"]).unwrap_or_else(|| "unknown".into());
        format!("{version}+{commit}{}", if dirty { ".dirty" } else { "" })
    };
    println!("cargo:rustc-env=RIFT_VERSION={display_version}");
    println!("cargo:rustc-link-search=framework=/System/Library/PrivateFrameworks");

    println!("cargo:rustc-link-lib=framework=SkyLight");
    println!("cargo:rustc-link-lib=framework=CoreFoundation");
    println!("cargo:rustc-link-lib=framework=IOKit");
    println!("cargo:rustc-link-lib=framework=MultitouchSupport");
    println!("cargo:rustc-link-lib=framework=Carbon");
}
