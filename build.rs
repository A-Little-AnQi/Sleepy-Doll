fn main() {
    for path in [
        ".git/HEAD",
        ".git/index",
        ".git/refs/heads",
        ".git/packed-refs",
        "src",
        "web",
        "plugins",
    ] {
        println!("cargo:rerun-if-changed={path}");
    }
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
    };
    let revision = git(&["rev-parse", "HEAD"]).unwrap_or_else(|| "unknown".into());
    let dirty = git(&["status", "--porcelain", "--untracked-files=no"]).map(|s| !s.is_empty());
    println!("cargo:rustc-env=SLEEPY_SOURCE_REVISION={revision}");
    println!(
        "cargo:rustc-env=SLEEPY_SOURCE_DIRTY={}",
        dirty
            .map(|v| v.to_string())
            .unwrap_or_else(|| "unknown".into())
    );
    println!("cargo:rerun-if-changed=release-channel.json");
    let channel_config: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string("release-channel.json").expect("reading release-channel.json"),
    )
    .expect("parsing release-channel.json");
    let channel = channel_config["channel"]
        .as_str()
        .expect("release channel is required");
    assert!(
        matches!(channel, "stable" | "test"),
        "release channel must be stable or test"
    );
    assert!(
        channel != "stable"
            || std::env::var("CARGO_PKG_VERSION_PRE")
                .unwrap_or_default()
                .is_empty(),
        "prerelease versions cannot be built for the stable channel"
    );
    println!("cargo:rustc-env=SLEEPY_RELEASE_CHANNEL={channel}");
    println!("cargo:rerun-if-changed=assets/sleepy-doll.manifest");
    println!("cargo:rerun-if-changed=assets/sleepy-doll.rc");
    println!("cargo:rerun-if-changed=assets/sleepy-doll.ico");
    if windows_msvc_desktop() {
        let root = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
        let manifest = root.join("assets/sleepy-doll.manifest");
        println!("cargo:rustc-link-arg-bin=sleepy-doll=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg-bin=sleepy-doll=/MANIFESTINPUT:{}",
            manifest.display()
        );
        println!("cargo:rustc-link-arg-bin=sleepy-doll=/MANIFESTUAC:NO");

        // 图标没有链接器开关，只能先过资源编译器。rc.exe 与桥的 cl.exe 来自同一套
        // Build Tools，所以这不引入新的构建前提。只链进桌面壳的两个 bin：开发用的
        // sleepy-doll-dev 不需要图标，链接它会白等一次 rc 调用。
        embed_resource::compile_for(
            root.join("assets/sleepy-doll.rc"),
            ["sleepy-doll"],
            embed_resource::NONE,
        )
        .manifest_required()
        .expect("compiling assets/sleepy-doll.rc");
    }
    #[cfg(feature = "setup")]
    build_setup();
}

#[cfg(feature = "setup")]
fn build_setup() {
    let root = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    println!("cargo:rerun-if-changed=src/setup/ui.slint");
    println!("cargo:rerun-if-changed=packages/ui/src/brand/icon.svg");
    let original = std::fs::read_to_string(root.join("packages/ui/src/brand/icon.svg")).unwrap();
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let svg = original
        .replace("currentColor", "#e9e9e9")
        .replace("var(--gold, #c9a227)", "#e4bc4a")
        .replace("var(--gold-bright, #f0d36a)", "#f6e19a");
    std::fs::write(out.join("setup-brand-dark.svg"), svg).unwrap();
    slint_build::compile(root.join("src/setup/ui.slint")).expect("compiling native installer UI");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rustc-link-arg-bin=sleepy-doll-setup=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg-bin=sleepy-doll-setup=/MANIFESTINPUT:{}",
            root.join("assets/sleepy-doll.manifest").display()
        );
        println!("cargo:rustc-link-arg-bin=sleepy-doll-setup=/MANIFESTUAC:NO");
        embed_resource::compile_for(
            root.join("assets/sleepy-doll-setup.rc"),
            ["sleepy-doll-setup"],
            embed_resource::NONE,
        )
        .manifest_required()
        .expect("compiling installer icon");
    }
}

fn windows_msvc_desktop() -> bool {
    std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
        && std::env::var_os("CARGO_FEATURE_DESKTOP").is_some()
}
