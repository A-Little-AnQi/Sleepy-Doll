fn main() {
    println!("cargo:rerun-if-changed=sleepy-doll.manifest");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
        && std::env::var_os("CARGO_FEATURE_DESKTOP").is_some()
    {
        let manifest = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap())
            .join("sleepy-doll.manifest");
        println!("cargo:rustc-link-arg-bin=sleepy-doll=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg-bin=sleepy-doll=/MANIFESTINPUT:{}",
            manifest.display()
        );
        println!("cargo:rustc-link-arg-bin=sleepy-doll=/MANIFESTUAC:NO");
    }
}
