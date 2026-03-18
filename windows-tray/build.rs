use std::env;
use std::path::PathBuf;

fn main() {
    let target = env::var("TARGET").unwrap_or_default();
    if !target.contains("windows") {
        return;
    }

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let assets_dir = manifest_dir.join("assets");
    let rc_path = assets_dir.join("icon.rc");
    let ico_light_path = assets_dir.join("icon-light.ico");
    let ico_dark_path = assets_dir.join("icon-dark.ico");
    if !rc_path.exists() || !ico_light_path.exists() || !ico_dark_path.exists() {
        return;
    }

    println!("cargo:rerun-if-changed={}", rc_path.display());
    println!("cargo:rerun-if-changed={}", ico_light_path.display());
    println!("cargo:rerun-if-changed={}", ico_dark_path.display());

    embed_resource::compile(
        rc_path.to_string_lossy().as_ref(),
        embed_resource::ParamsIncludeDirs(std::iter::once(assets_dir.as_os_str())),
    )
    .manifest_optional()
    .unwrap();
}
