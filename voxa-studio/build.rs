use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    println!("cargo:rerun-if-changed=assets/app.rc");
    println!("cargo:rerun-if-changed=assets/voxa-icon.ico");

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR not set"));
    let res_obj = out_dir.join("voxa_res.o");

    let status = Command::new("windres")
        .args([
            "-i",
            "assets/app.rc",
            "-I",
            "assets",
            "-O",
            "coff",
            "-o",
            res_obj
                .to_str()
                .expect("OUT_DIR path must be valid UTF-8"),
        ])
        .status()
        .expect("Failed to invoke `windres` to compile Windows application icon & version resource");

    if !status.success() {
        panic!("`windres` failed with exit status {status}");
    }

    println!("cargo:rustc-link-arg-bin=voxa-studio={}", res_obj.display());
}
