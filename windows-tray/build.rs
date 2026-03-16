use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let target = env::var("TARGET").unwrap_or_default();
    if !target.contains("windows-gnu") {
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

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let obj_path = out_dir.join("icon.o");

    let windres_candidates = [
        env::var("WINDRES").ok(),
        Some("x86_64-w64-mingw32-windres".to_string()),
        Some("windres".to_string()),
    ];

    let mut success = false;
    for candidate in windres_candidates.into_iter().flatten() {
        let status = Command::new(&candidate)
            .args(["-O", "coff"])
            .args(["-I", assets_dir.to_string_lossy().as_ref()])
            .args(["-i", rc_path.to_string_lossy().as_ref()])
            .args(["-o", obj_path.to_string_lossy().as_ref()])
            .status();
        if let Ok(status) = status {
            if status.success() {
                success = true;
                break;
            }
        }
    }

    if success {
        println!("cargo:rustc-link-arg-bins={}", obj_path.display());
    }
}
