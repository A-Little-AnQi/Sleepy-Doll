fn main() {
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
        // Build Tools，所以这不引入新的构建前提。只链进桌面壳：开发用的
        // sleepy-doll-dev 不需要图标，链接它会白等一次 rc 调用。
        embed_resource::compile_for(
            root.join("assets/sleepy-doll.rc"),
            ["sleepy-doll"],
            embed_resource::NONE,
        )
        .manifest_required()
        .expect("compiling assets/sleepy-doll.rc");
    }
}

fn windows_msvc_desktop() -> bool {
    std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
        && std::env::var_os("CARGO_FEATURE_DESKTOP").is_some()
}
