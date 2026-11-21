use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

fn compile_windows_rc(rc_path: &str, out_stem: &str, bin_name: &str, out_dir: &Path, is_gnu: bool) {
    println!("cargo:rerun-if-changed={rc_path}");

    if is_gnu {
        let obj = out_dir.join(format!("{out_stem}.o"));
        let status = Command::new("windres")
            .args([
                "-i",
                rc_path,
                "-I",
                "assets",
                "-O",
                "coff",
                "-o",
                obj.to_str().expect("OUT_DIR must be valid UTF-8"),
            ])
            .status()
            .expect("Failed to run `windres` for Windows resource compilation");
        if !status.success() {
            panic!("`windres` failed on {rc_path} with status {status}");
        }
        println!("cargo:rustc-link-arg-bin={bin_name}={}", obj.display());
    } else {
        let res = out_dir.join(format!("{out_stem}.res"));
        let rc_tool = find_msvc_rc_tool().unwrap_or_else(|| PathBuf::from("rc.exe"));
        let is_llvm_rc = rc_tool
            .file_name()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s.to_ascii_lowercase().contains("llvm-rc"));
        let mut cmd = Command::new(&rc_tool);
        if is_llvm_rc {
            cmd.args(["/I", "assets", "/FO", res.to_str().unwrap(), rc_path]);
        } else {
            cmd.args(["/nologo", "/I", "assets", "/fo", res.to_str().unwrap(), rc_path]);
        }
        let status = cmd
            .status()
            .unwrap_or_else(|e| panic!("Failed to invoke Windows resource compiler `{}`: {e}", rc_tool.display()));
        if !status.success() {
            panic!("Resource compiler failed on {rc_path} with status {status}");
        }
        println!("cargo:rustc-link-arg-bin={bin_name}={}", res.display());
    }
}

fn find_msvc_rc_tool() -> Option<PathBuf> {
    if let Ok(path) = env::var("PATH") {
        for dir in env::split_paths(&path) {
            for candidate in ["rc.exe", "llvm-rc.exe"] {
                let p = dir.join(candidate);
                if p.is_file() {
                    return Some(p);
                }
            }
        }
    }
    let kits_bin = Path::new(r"C:\Program Files (x86)\Windows Kits\10\bin");
    if let Ok(entries) = std::fs::read_dir(kits_bin) {
        let mut versions: Vec<PathBuf> = entries
            .filter_map(|e| e.ok().map(|d| d.path()))
            .filter(|p| p.join("x64").join("rc.exe").is_file())
            .collect();
        versions.sort();
        if let Some(latest) = versions.pop() {
            return Some(latest.join("x64").join("rc.exe"));
        }
    }
    None
}

fn main() {
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    println!("cargo:rerun-if-changed=assets/voxa-icon.ico");
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR not set"));
    let is_gnu = env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("gnu");

    compile_windows_rc("assets/app.rc", "voxa_res", "voxa-studio", &out_dir, is_gnu);

    if env::var_os("CARGO_FEATURE_INSTALLER").is_some() {
        compile_windows_rc("assets/setup.rc", "setup_res", "voxa-setup", &out_dir, is_gnu);
    }
}
