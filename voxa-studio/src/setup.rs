#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui::{
    self, Color32, CornerRadius, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Vec2,
};
use std::io::{BufRead, BufReader};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;

const CREATE_NO_WINDOW: u32 = 0x08000000;

const VOXA_EXE_BYTES: &[u8] = include_bytes!("../target/release/voxa-studio.exe");
const WORKER_PY_BYTES: &[u8] = include_bytes!("../backend/worker.py");
const REQUIREMENTS_BYTES: &[u8] = include_bytes!("../../requirements.txt");
const ICON_ICO_BYTES: &[u8] = include_bytes!("../assets/voxa-icon.ico");
const ICON_PNG_BYTES: &[u8] = include_bytes!("../assets/voxa-icon-256.png");
const LICENSE_BYTES: &[u8] = include_bytes!("../../LICENSE");

// Obsidian Dark Studio Palette (matches voxa-studio theme)
const BG: Color32 = Color32::from_rgb(14, 16, 21);
const PANEL_BG: Color32 = Color32::from_rgb(21, 24, 33);
const CARD_BG: Color32 = Color32::from_rgb(28, 32, 43);
const INPUT_BG: Color32 = Color32::from_rgb(17, 19, 26);
const BORDER: Color32 = Color32::from_rgb(38, 44, 58);
const BORDER_STRONG: Color32 = Color32::from_rgb(54, 63, 84);
const INK: Color32 = Color32::from_rgb(241, 245, 249);
const INK_MUTED: Color32 = Color32::from_rgb(148, 163, 184);
const PRIMARY: Color32 = Color32::from_rgb(0, 212, 229);
const ON_PRIMARY: Color32 = Color32::from_rgb(7, 19, 24);
const ACCENT_BLUE: Color32 = Color32::from_rgb(56, 189, 248);
const SUCCESS: Color32 = Color32::from_rgb(52, 211, 153);
const SUCCESS_SOFT: Color32 = Color32::from_rgb(12, 54, 43);
const DANGER: Color32 = Color32::from_rgb(248, 113, 113);
const DANGER_SOFT: Color32 = Color32::from_rgb(69, 24, 28);

#[derive(Debug, Clone, PartialEq, Eq)]
enum WizardStage {
    Configure,
    Installing,
    Complete,
    Failed(String),
}

enum InstallMsg {
    Progress { fraction: f32, status: String },
    Log(String),
    Finished,
    Failed(String),
}

#[derive(Debug, Clone)]
struct InstallOptions {
    install_dir: PathBuf,
    create_desktop_shortcut: bool,
    create_start_menu: bool,
    register_uninstaller: bool,
}

fn default_install_dir() -> PathBuf {
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        return PathBuf::from(local).join("VoxaStudio");
    }
    PathBuf::from(r"C:\VoxaStudio")
}

fn load_setup_icon() -> Option<egui::IconData> {
    eframe::icon_data::from_png_bytes(ICON_PNG_BYTES).ok()
}

fn draw_voxa_logo(ui: &mut egui::Ui, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let p = ui.painter_at(rect);
    let u = size / 512.0;
    let pt = |x: f32, y: f32| Pos2::new(rect.left() + x * u, rect.top() + y * u);

    p.rect_filled(rect, CornerRadius::same(11), Color32::from_rgb(13, 16, 23));
    p.rect_stroke(
        rect,
        CornerRadius::same(11),
        Stroke::new(1.4_f32, PRIMARY.gamma_multiply(0.65)),
        StrokeKind::Inside,
    );

    let left_wing = vec![pt(96.0, 132.0), pt(166.0, 132.0), pt(274.0, 412.0), pt(238.0, 412.0)];
    let right_wing = vec![pt(346.0, 132.0), pt(416.0, 132.0), pt(274.0, 412.0), pt(238.0, 412.0)];
    p.add(egui::Shape::convex_polygon(left_wing, PRIMARY, Stroke::NONE));
    p.add(egui::Shape::convex_polygon(right_wing, PRIMARY, Stroke::NONE));

    let bar_r = CornerRadius::same(((11.0 * u).round() as u8).max(2));
    p.rect_filled(Rect::from_min_max(pt(202.0, 114.0), pt(224.0, 186.0)), bar_r, ACCENT_BLUE);
    p.rect_filled(Rect::from_min_max(pt(244.0, 92.0), pt(268.0, 232.0)), bar_r, PRIMARY);
    p.rect_filled(Rect::from_min_max(pt(288.0, 114.0), pt(310.0, 186.0)), bar_r, ACCENT_BLUE);
}

fn parse_python_probe_line(raw: &str) -> Option<(u32, u32, PathBuf)> {
    let line = raw.lines().find(|l| l.contains('|'))?.trim();
    let (ver, exe) = line.split_once('|')?;
    let (maj_s, min_s) = ver.trim().split_once('.')?;
    let major: u32 = maj_s.parse().ok()?;
    let minor: u32 = min_s.parse().ok()?;
    let path = PathBuf::from(exe.trim());
    if major == 3 && (10..=12).contains(&minor) {
        Some((major, minor, path))
    } else {
        None
    }
}

fn probe_python_candidate(program: &str, prefix_args: &[&str]) -> Option<(u32, u32, PathBuf)> {
    let mut cmd = Command::new(program);
    cmd.args(prefix_args)
        .args([
            "-c",
            "import sys; print(f'{sys.version_info[0]}.{sys.version_info[1]}|{sys.executable}')",
        ])
        .creation_flags(CREATE_NO_WINDOW);
    let out = cmd.output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let (maj, min, exe) = parse_python_probe_line(&text)?;
    if exe.is_file() {
        Some((maj, min, exe))
    } else {
        None
    }
}

fn find_compatible_python() -> Option<(String, PathBuf)> {
    let candidates: Vec<(&str, &[&str])> = vec![
        ("py", &["-3.11"]),
        ("py", &["-3.12"]),
        ("py", &["-3.10"]),
        ("python", &[]),
        ("python3", &[]),
    ];
    for (prog, args) in candidates {
        if let Some((maj, min, exe)) = probe_python_candidate(prog, args) {
            return Some((format!("{maj}.{min}"), exe));
        }
    }

    let mut path_candidates = Vec::new();
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        let base = PathBuf::from(local).join("Programs").join("Python");
        for ver in ["Python311", "Python312", "Python310"] {
            path_candidates.push(base.join(ver).join("python.exe"));
        }
    }
    for base_str in [r"C:\Program Files", r"C:\Python311", r"C:\Python312"] {
        let base = Path::new(base_str);
        if base.join("python.exe").is_file() {
            path_candidates.push(base.join("python.exe"));
        }
        for ver in ["Python311", "Python312", "Python310"] {
            path_candidates.push(base.join(ver).join("python.exe"));
        }
    }

    for exe in path_candidates {
        if exe.is_file() {
            if let Some(s) = exe.to_str() {
                if let Some((maj, min, resolved)) = probe_python_candidate(s, &[]) {
                    return Some((format!("{maj}.{min}"), resolved));
                }
            }
        }
    }
    None
}

fn run_logged_command(
    program: &str,
    args: &[&str],
    workdir: Option<&Path>,
    tx: &Sender<InstallMsg>,
) -> Result<(), String> {
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(CREATE_NO_WINDOW);
    if let Some(dir) = workdir {
        cmd.current_dir(dir);
    }

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to start `{program}`: {e}"))?;

    if let Some(stdout) = child.stdout.take() {
        let tx_out = tx.clone();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    let _ = tx_out.send(InstallMsg::Log(trimmed.to_string()));
                }
            }
        });
    }
    if let Some(stderr) = child.stderr.take() {
        let tx_err = tx.clone();
        thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    let _ = tx_err.send(InstallMsg::Log(trimmed.to_string()));
                }
            }
        });
    }

    let status = child
        .wait()
        .map_err(|e| format!("Failed waiting for `{program}`: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("Command `{program}` exited with code {status}"))
    }
}

fn write_uninstall_script(install_dir: &Path) -> Result<PathBuf, String> {
    let script_path = install_dir.join("Uninstall-Voxa-Studio.ps1");
    let escaped_dir = install_dir.to_string_lossy().replace('\'', "''");
    let script = format!(
        r#"$ErrorActionPreference = 'SilentlyContinue'
$InstallDir = '{escaped_dir}'

Get-Process -Name 'Voxa-Studio' -ErrorAction SilentlyContinue | Stop-Process -Force

$DesktopLnk = Join-Path ([Environment]::GetFolderPath('Desktop')) 'Voxa Studio.lnk'
$StartMenuDir = Join-Path ([Environment]::GetFolderPath('Programs')) 'Voxa Studio'
Remove-Item -Force -ErrorAction SilentlyContinue $DesktopLnk
Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $StartMenuDir
Remove-Item -Recurse -Force -ErrorAction SilentlyContinue 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\VoxaStudio'

if (Test-Path $InstallDir) {{
    Start-Process cmd.exe -WindowStyle Hidden -ArgumentList "/c timeout /t 2 /nobreak >nul & rmdir /s /q `"$InstallDir`""
}}
"#
    );
    std::fs::write(&script_path, script)
        .map_err(|e| format!("Failed to write uninstaller script: {e}"))?;
    Ok(script_path)
}

fn create_shortcut(
    lnk_path: &Path,
    target_exe: &Path,
    working_dir: &Path,
    icon_path: &Path,
    tx: &Sender<InstallMsg>,
) -> Result<(), String> {
    if let Some(parent) = lnk_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed creating shortcut folder: {e}"))?;
    }
    let esc = |p: &Path| p.to_string_lossy().replace('\'', "''");
    let ps = format!(
        "$ws = New-Object -ComObject WScript.Shell; \
         $s = $ws.CreateShortcut('{lnk}'); \
         $s.TargetPath = '{target}'; \
         $s.WorkingDirectory = '{wd}'; \
         $s.IconLocation = '{icon}'; \
         $s.Description = 'Voxa Studio — Local Neural Voice & Speech Studio'; \
         $s.Save()",
        lnk = esc(lnk_path),
        target = esc(target_exe),
        wd = esc(working_dir),
        icon = esc(icon_path),
    );
    run_logged_command(
        "powershell.exe",
        &["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", &ps],
        None,
        tx,
    )
}

fn register_windows_uninstaller(
    install_dir: &Path,
    icon_path: &Path,
    uninstall_ps: &Path,
    tx: &Sender<InstallMsg>,
) -> Result<(), String> {
    let esc = |p: &Path| p.to_string_lossy().replace('\'', "''");
    let ps = format!(
        "$key = 'HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\VoxaStudio'; \
         New-Item -Path $key -Force | Out-Null; \
         Set-ItemProperty -Path $key -Name 'DisplayName' -Value 'Voxa Studio'; \
         Set-ItemProperty -Path $key -Name 'DisplayVersion' -Value '1.0.0'; \
         Set-ItemProperty -Path $key -Name 'Publisher' -Value 'Voxa Studio Contributors'; \
         Set-ItemProperty -Path $key -Name 'DisplayIcon' -Value '{icon}'; \
         Set-ItemProperty -Path $key -Name 'InstallLocation' -Value '{dir}'; \
         Set-ItemProperty -Path $key -Name 'UninstallString' -Value 'powershell.exe -NoProfile -ExecutionPolicy Bypass -File \"{uninst}\"'; \
         Set-ItemProperty -Path $key -Name 'NoModify' -Type DWord -Value 1; \
         Set-ItemProperty -Path $key -Name 'NoRepair' -Type DWord -Value 1",
        icon = esc(icon_path),
        dir = esc(install_dir),
        uninst = esc(uninstall_ps),
    );
    run_logged_command(
        "powershell.exe",
        &["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", &ps],
        None,
        tx,
    )
}

fn perform_install(opts: InstallOptions, tx: &Sender<InstallMsg>) -> Result<(), String> {
    let dir = &opts.install_dir;
    let _ = tx.send(InstallMsg::Progress {
        fraction: 0.08,
        status: "Step 1/5 — Extracting Voxa Studio application & assets...".to_string(),
    });
    let _ = tx.send(InstallMsg::Log(format!("Install directory: {}", dir.display())));

    let backend_dir = dir.join("voxa-studio").join("backend");
    let assets_dir = dir.join("voxa-studio").join("assets");
    std::fs::create_dir_all(&backend_dir)
        .map_err(|e| format!("Cannot create directory `{}`: {e}", backend_dir.display()))?;
    std::fs::create_dir_all(&assets_dir)
        .map_err(|e| format!("Cannot create assets folder: {e}"))?;

    let exe_path = dir.join("Voxa-Studio.exe");
    let worker_path = backend_dir.join("worker.py");
    let req_path = dir.join("requirements.txt");
    let ico_path = assets_dir.join("voxa-icon.ico");
    let license_path = dir.join("LICENSE");

    std::fs::write(&exe_path, VOXA_EXE_BYTES)
        .map_err(|e| format!("Failed writing `Voxa-Studio.exe` (close any running copy first): {e}"))?;
    std::fs::write(&worker_path, WORKER_PY_BYTES)
        .map_err(|e| format!("Failed writing `worker.py`: {e}"))?;
    std::fs::write(&req_path, REQUIREMENTS_BYTES)
        .map_err(|e| format!("Failed writing `requirements.txt`: {e}"))?;
    std::fs::write(&ico_path, ICON_ICO_BYTES)
        .map_err(|e| format!("Failed writing `voxa-icon.ico`: {e}"))?;
    std::fs::write(&license_path, LICENSE_BYTES)
        .map_err(|e| format!("Failed writing `LICENSE`: {e}"))?;
    let uninstall_ps = write_uninstall_script(dir)?;

    let _ = tx.send(InstallMsg::Progress {
        fraction: 0.20,
        status: "Step 2/5 — Checking Python 3.10–3.12 runtime...".to_string(),
    });

    let (py_ver, py_exe) = match find_compatible_python() {
        Some(found) => found,
        None => {
            let _ = tx.send(InstallMsg::Log(
                "Python 3.10–3.12 not found. Attempting silent install of Python 3.11 via winget...".to_string(),
            ));
            let winget_res = run_logged_command(
                "winget",
                &[
                    "install",
                    "-e",
                    "--id",
                    "Python.Python.3.11",
                    "--scope",
                    "user",
                    "--silent",
                    "--accept-package-agreements",
                    "--accept-source-agreements",
                ],
                None,
                tx,
            );
            if winget_res.is_err() {
                let _ = run_logged_command(
                    "winget",
                    &[
                        "install",
                        "-e",
                        "--id",
                        "Python.Python.3.11",
                        "--silent",
                        "--accept-package-agreements",
                        "--accept-source-agreements",
                    ],
                    None,
                    tx,
                );
            }
            find_compatible_python().ok_or_else(|| {
                "Could not find or auto-install Python 3.10–3.12. Please install Python 3.11 from python.org (check 'Add python.exe to PATH') and click Install again.".to_string()
            })?
        }
    };
    let _ = tx.send(InstallMsg::Log(format!(
        "Using Python {py_ver}: {}",
        py_exe.display()
    )));

    let _ = tx.send(InstallMsg::Progress {
        fraction: 0.34,
        status: "Step 3/5 — Creating isolated Python virtual environment (.venv)...".to_string(),
    });
    let venv_dir = dir.join(".venv");
    let venv_py = venv_dir.join("Scripts").join("python.exe");
    if !venv_py.is_file() {
        run_logged_command(
            py_exe.to_str().unwrap_or("python"),
            &["-m", "venv", venv_dir.to_str().unwrap_or(".venv")],
            Some(dir),
            tx,
        )?;
    } else {
        let _ = tx.send(InstallMsg::Log("Reusing existing .venv runtime.".to_string()));
    }

    let venv_py_str = venv_py
        .to_str()
        .ok_or_else(|| "Invalid UTF-8 path for `.venv/Scripts/python.exe`".to_string())?;

    let _ = tx.send(InstallMsg::Progress {
        fraction: 0.48,
        status: "Step 4/5 — Installing PyTorch, ONNX Runtime & KittenTTS neural engine (first install downloads packages)...".to_string(),
    });
    run_logged_command(
        venv_py_str,
        &["-m", "pip", "install", "--upgrade", "pip", "wheel"],
        Some(dir),
        tx,
    )?;

    let _ = tx.send(InstallMsg::Progress {
        fraction: 0.62,
        status: "Step 4/5 — Installing speech synthesis packages from requirements.txt...".to_string(),
    });
    run_logged_command(
        venv_py_str,
        &[
            "-m",
            "pip",
            "install",
            "--prefer-binary",
            "-r",
            req_path.to_str().unwrap_or("requirements.txt"),
        ],
        Some(dir),
        tx,
    )?;

    let _ = tx.send(InstallMsg::Progress {
        fraction: 0.84,
        status: "Step 4/5 — Downloading English text normalization model (en_core_web_sm)...".to_string(),
    });
    run_logged_command(
        venv_py_str,
        &["-m", "spacy", "download", "en_core_web_sm"],
        Some(dir),
        tx,
    )?;

    let _ = tx.send(InstallMsg::Progress {
        fraction: 0.93,
        status: "Step 5/5 — Creating shortcuts & registering Voxa Studio...".to_string(),
    });
    if opts.create_desktop_shortcut {
        if let Some(user_profile) = std::env::var_os("USERPROFILE") {
            let desktop_lnk = PathBuf::from(user_profile)
                .join("Desktop")
                .join("Voxa Studio.lnk");
            create_shortcut(&desktop_lnk, &exe_path, dir, &ico_path, tx)?;
        }
    }
    if opts.create_start_menu {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            let start_lnk = PathBuf::from(appdata)
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs")
                .join("Voxa Studio")
                .join("Voxa Studio.lnk");
            create_shortcut(&start_lnk, &exe_path, dir, &ico_path, tx)?;
        }
    }
    if opts.register_uninstaller {
        register_windows_uninstaller(dir, &ico_path, &uninstall_ps, tx)?;
    }

    let _ = tx.send(InstallMsg::Progress {
        fraction: 1.0,
        status: "Installation complete!".to_string(),
    });
    Ok(())
}

struct VoxaSetupApp {
    install_dir_str: String,
    create_desktop_shortcut: bool,
    create_start_menu: bool,
    register_uninstaller: bool,
    launch_after: bool,
    stage: WizardStage,
    progress: f32,
    status_line: String,
    logs: Vec<String>,
    rx: Option<Receiver<InstallMsg>>,
    auto_launched: bool,
}

impl VoxaSetupApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut style = (*cc.egui_ctx.style()).clone();
        style.visuals = egui::Visuals::dark();
        style.visuals.window_fill = BG;
        style.visuals.panel_fill = BG;
        style.visuals.extreme_bg_color = INPUT_BG;
        style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(44, 51, 68);
        style.visuals.widgets.inactive.fg_stroke = Stroke::new(1.2_f32, INK);
        style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, BORDER_STRONG);
        style.visuals.widgets.inactive.corner_radius = CornerRadius::same(8);
        style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(56, 65, 86);
        style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.2_f32, PRIMARY);
        style.visuals.widgets.hovered.corner_radius = CornerRadius::same(8);
        style.visuals.selection.bg_fill = PRIMARY.gamma_multiply(0.35);
        style.spacing.item_spacing = Vec2::new(8.0, 8.0);
        style.spacing.button_padding = Vec2::new(12.0, 7.0);
        cc.egui_ctx.set_style(style);

        Self {
            install_dir_str: default_install_dir().to_string_lossy().to_string(),
            create_desktop_shortcut: true,
            create_start_menu: true,
            register_uninstaller: true,
            launch_after: true,
            stage: WizardStage::Configure,
            progress: 0.0,
            status_line: "Ready to install Voxa Studio v1.0.0".to_string(),
            logs: Vec::new(),
            rx: None,
            auto_launched: false,
        }
    }

    fn start_install(&mut self, ctx: egui::Context) {
        let trimmed = self.install_dir_str.trim();
        let dir = if trimmed.is_empty() {
            default_install_dir()
        } else {
            PathBuf::from(trimmed)
        };
        self.install_dir_str = dir.to_string_lossy().to_string();

        let opts = InstallOptions {
            install_dir: dir,
            create_desktop_shortcut: self.create_desktop_shortcut,
            create_start_menu: self.create_start_menu,
            register_uninstaller: self.register_uninstaller,
        };

        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        self.stage = WizardStage::Installing;
        self.progress = 0.03;
        self.status_line = "Preparing installation...".to_string();
        self.logs.clear();
        self.auto_launched = false;

        thread::spawn(move || {
            match perform_install(opts, &tx) {
                Ok(()) => {
                    let _ = tx.send(InstallMsg::Finished);
                }
                Err(err) => {
                    let _ = tx.send(InstallMsg::Failed(err));
                }
            }
            ctx.request_repaint();
        });
    }

    fn poll_installer(&mut self, ctx: &egui::Context) {
        let Some(rx) = &self.rx else {
            return;
        };
        let mut msgs = Vec::new();
        while let Ok(msg) = rx.try_recv() {
            msgs.push(msg);
        }
        if !msgs.is_empty() {
            ctx.request_repaint();
        }
        for msg in msgs {
            match msg {
                InstallMsg::Progress { fraction, status } => {
                    self.progress = fraction;
                    self.status_line = status;
                }
                InstallMsg::Log(line) => {
                    self.logs.push(line);
                    if self.logs.len() > 250 {
                        let excess = self.logs.len() - 250;
                        self.logs.drain(0..excess);
                    }
                }
                InstallMsg::Finished => {
                    self.progress = 1.0;
                    self.status_line = "Voxa Studio is installed and ready!".to_string();
                    self.stage = WizardStage::Complete;
                    self.rx = None;
                    break;
                }
                InstallMsg::Failed(err) => {
                    self.stage = WizardStage::Failed(err);
                    self.rx = None;
                    break;
                }
            }
        }
    }

    fn launch_installed_app(&self) {
        let exe = PathBuf::from(&self.install_dir_str).join("Voxa-Studio.exe");
        if exe.is_file() {
            let _ = Command::new(&exe)
                .current_dir(Path::new(&self.install_dir_str))
                .spawn();
        }
    }

    fn open_install_folder(&self) {
        let _ = Command::new("explorer.exe")
            .arg(&self.install_dir_str)
            .spawn();
    }
}

impl eframe::App for VoxaSetupApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_installer(ctx);
        if self.stage == WizardStage::Installing {
            ctx.request_repaint_after(std::time::Duration::from_millis(80));
        }
        if self.stage == WizardStage::Complete && self.launch_after && !self.auto_launched {
            self.auto_launched = true;
            self.launch_installed_app();
        }

        egui::TopBottomPanel::top("setup_header")
            .frame(
                egui::Frame::new()
                    .fill(PANEL_BG)
                    .stroke(Stroke::new(1.0_f32, BORDER))
                    .inner_margin(egui::Margin::symmetric(18, 12)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    draw_voxa_logo(ui, 38.0);
                    ui.add_space(6.0);
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Voxa Studio").size(19.0).strong().color(INK));
                            egui::Frame::new()
                                .fill(PRIMARY.gamma_multiply(0.16))
                                .corner_radius(CornerRadius::same(6))
                                .inner_margin(egui::Margin::symmetric(7, 2))
                                .show(ui, |ui| {
                                    ui.label(
                                        RichText::new("SETUP · v1.0.0")
                                            .size(11.0)
                                            .strong()
                                            .color(PRIMARY),
                                    );
                                });
                        });
                        ui.label(
                            RichText::new("Local-First Neural Voice, Narration & Cloning Workspace")
                                .size(12.0)
                                .color(INK_MUTED),
                        );
                    });
                });
            });

        egui::TopBottomPanel::bottom("setup_footer")
            .frame(
                egui::Frame::new()
                    .fill(PANEL_BG)
                    .stroke(Stroke::new(1.0_f32, BORDER))
                    .inner_margin(egui::Margin::symmetric(18, 12)),
            )
            .show(ctx, |ui| {
                egui::Sides::new().show(
                    ui,
                    |ui| {
                        ui.label(
                            RichText::new("Apache-2.0 · Zero Terminal Window · Powered by KittenTTS v2")
                                .size(11.5)
                                .color(INK_MUTED),
                        );
                    },
                    |ui| match &self.stage {
                        WizardStage::Configure => {
                            let install_btn = egui::Button::new(
                                RichText::new("Install Voxa Studio")
                                    .size(13.5)
                                    .strong()
                                    .color(ON_PRIMARY),
                            )
                            .fill(PRIMARY)
                            .min_size(Vec2::new(165.0, 34.0))
                            .corner_radius(CornerRadius::same(8));
                            if ui.add(install_btn).clicked() {
                                self.start_install(ctx.clone());
                            }
                            if ui
                                .add(
                                    egui::Button::new(RichText::new("Cancel").size(13.0))
                                        .min_size(Vec2::new(82.0, 34.0)),
                                )
                                .clicked()
                            {
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                        }
                        WizardStage::Installing => {
                            ui.spinner();
                            ui.label(RichText::new("Installing...").size(12.5).color(PRIMARY));
                        }
                        WizardStage::Complete => {
                            let launch_btn = egui::Button::new(
                                RichText::new("Launch Voxa Studio")
                                    .size(13.5)
                                    .strong()
                                    .color(ON_PRIMARY),
                            )
                            .fill(PRIMARY)
                            .min_size(Vec2::new(155.0, 34.0))
                            .corner_radius(CornerRadius::same(8));
                            if ui.add(launch_btn).clicked() {
                                self.launch_installed_app();
                            }
                            if ui
                                .add(
                                    egui::Button::new(RichText::new("Open Folder").size(12.5))
                                        .min_size(Vec2::new(98.0, 34.0)),
                                )
                                .clicked()
                            {
                                self.open_install_folder();
                            }
                            if ui
                                .add(
                                    egui::Button::new(RichText::new("Finish").size(13.0))
                                        .min_size(Vec2::new(80.0, 34.0)),
                                )
                                .clicked()
                            {
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                        }
                        WizardStage::Failed(_) => {
                            let retry_btn = egui::Button::new(
                                RichText::new("Retry Install")
                                    .size(13.5)
                                    .strong()
                                    .color(ON_PRIMARY),
                            )
                            .fill(PRIMARY)
                            .min_size(Vec2::new(125.0, 34.0))
                            .corner_radius(CornerRadius::same(8));
                            if ui.add(retry_btn).clicked() {
                                self.start_install(ctx.clone());
                            }
                            if ui
                                .add(
                                    egui::Button::new(RichText::new("Close").size(13.0))
                                        .min_size(Vec2::new(80.0, 34.0)),
                                )
                                .clicked()
                            {
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                        }
                    },
                );
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BG).inner_margin(egui::Margin::same(18)))
            .show(ctx, |ui| {
                match &self.stage {
                    WizardStage::Configure => {
                        egui::Frame::new()
                            .fill(CARD_BG)
                            .stroke(Stroke::new(1.0_f32, BORDER))
                            .corner_radius(CornerRadius::same(12))
                            .inner_margin(egui::Margin::same(14))
                            .show(ui, |ui| {
                                ui.set_min_width(ui.available_width());
                                ui.label(
                                    RichText::new("Destination Folder")
                                        .size(14.0)
                                        .strong()
                                        .color(INK),
                                );
                                ui.label(
                                    RichText::new("No admin rights or Git required. Sets up Voxa-Studio.exe and an isolated Python runtime inside this folder.")
                                        .size(12.0)
                                        .color(INK_MUTED),
                                );
                                ui.add_space(6.0);
                                ui.horizontal(|ui| {
                                    let field_w = (ui.available_width() - 96.0).max(200.0);
                                    ui.add_sized(
                                        Vec2::new(field_w, 30.0),
                                        egui::TextEdit::singleline(&mut self.install_dir_str),
                                    );
                                    if ui
                                        .add(
                                            egui::Button::new("Browse...")
                                                .min_size(Vec2::new(88.0, 30.0)),
                                        )
                                        .clicked()
                                    {
                                        if let Some(picked) = rfd::FileDialog::new()
                                            .set_directory(&self.install_dir_str)
                                            .pick_folder()
                                        {
                                            self.install_dir_str =
                                                picked.to_string_lossy().to_string();
                                        }
                                    }
                                });
                            });

                        ui.add_space(10.0);

                        egui::Frame::new()
                            .fill(CARD_BG)
                            .stroke(Stroke::new(1.0_f32, BORDER))
                            .corner_radius(CornerRadius::same(12))
                            .inner_margin(egui::Margin::same(14))
                            .show(ui, |ui| {
                                ui.set_min_width(ui.available_width());
                                ui.label(
                                    RichText::new("Setup Options")
                                        .size(14.0)
                                        .strong()
                                        .color(INK),
                                );
                                ui.add_space(4.0);
                                ui.checkbox(
                                    &mut self.create_desktop_shortcut,
                                    "Create a Desktop shortcut (Voxa Studio)",
                                );
                                ui.checkbox(
                                    &mut self.create_start_menu,
                                    "Add Start Menu shortcut",
                                );
                                ui.checkbox(
                                    &mut self.register_uninstaller,
                                    "Register in Windows Installed Apps (clean one-click uninstall)",
                                );
                                ui.checkbox(
                                    &mut self.launch_after,
                                    "Launch Voxa Studio immediately after setup finishes",
                                );
                            });
                    }
                    WizardStage::Installing | WizardStage::Complete | WizardStage::Failed(_) => {
                        let (banner_bg, banner_fg, headline) = match &self.stage {
                            WizardStage::Complete => (
                                SUCCESS_SOFT,
                                SUCCESS,
                                "Voxa Studio v1.0.0 Installed Successfully".to_string(),
                            ),
                            WizardStage::Failed(err) => (
                                DANGER_SOFT,
                                DANGER,
                                format!("Installation Failed: {err}"),
                            ),
                            _ => (
                                CARD_BG,
                                PRIMARY,
                                self.status_line.clone(),
                            ),
                        };

                        egui::Frame::new()
                            .fill(banner_bg)
                            .stroke(Stroke::new(1.0_f32, banner_fg.gamma_multiply(0.45)))
                            .corner_radius(CornerRadius::same(10))
                            .inner_margin(egui::Margin::same(12))
                            .show(ui, |ui| {
                                ui.set_min_width(ui.available_width());
                                ui.label(RichText::new(headline).size(13.5).strong().color(banner_fg));
                                ui.add_space(6.0);
                                ui.add(
                                    egui::ProgressBar::new(self.progress)
                                        .show_percentage()
                                        .animate(self.stage == WizardStage::Installing),
                                );
                            });

                        ui.add_space(10.0);

                        egui::Frame::new()
                            .fill(CARD_BG)
                            .stroke(Stroke::new(1.0_f32, BORDER))
                            .corner_radius(CornerRadius::same(12))
                            .inner_margin(egui::Margin::same(12))
                            .show(ui, |ui| {
                                ui.set_min_width(ui.available_width());
                                ui.label(
                                    RichText::new("Installer Activity Log")
                                        .size(13.0)
                                        .strong()
                                        .color(INK),
                                );
                                ui.add_space(4.0);
                                egui::ScrollArea::vertical()
                                    .max_height(210.0)
                                    .stick_to_bottom(true)
                                    .auto_shrink([false, false])
                                    .show(ui, |ui| {
                                        if self.logs.is_empty() {
                                            ui.label(
                                                RichText::new("Waiting for setup output...")
                                                    .size(11.5)
                                                    .color(INK_MUTED),
                                            );
                                        } else {
                                            for line in &self.logs {
                                                ui.label(
                                                    RichText::new(line)
                                                        .monospace()
                                                        .size(11.0)
                                                        .color(INK_MUTED),
                                                );
                                            }
                                        }
                                    });
                            });
                    }
                }
            });
    }
}

fn main() -> eframe::Result<()> {
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Voxa Studio Setup — v1.0.0")
        .with_inner_size([680.0, 520.0])
        .with_min_inner_size([620.0, 480.0]);
    if let Some(icon) = load_setup_icon() {
        viewport = viewport.with_icon(Arc::new(icon));
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "Voxa Studio Setup",
        options,
        Box::new(|cc| Ok(Box::new(VoxaSetupApp::new(cc)))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_python_version_parsing() {
        assert_eq!(
            parse_python_probe_line("3.11|C:\\Python311\\python.exe"),
            Some((3, 11, PathBuf::from("C:\\Python311\\python.exe")))
        );
        assert_eq!(
            parse_python_probe_line("3.12|/usr/bin/python3"),
            Some((3, 12, PathBuf::from("/usr/bin/python3")))
        );
        assert_eq!(parse_python_probe_line("3.9|C:\\Py39\\python.exe"), None);
        assert_eq!(parse_python_probe_line("3.13|C:\\Py313\\python.exe"), None);
        assert_eq!(parse_python_probe_line("2.7|C:\\Py27\\python.exe"), None);
    }

    #[test]
    fn test_embedded_payloads_non_empty() {
        assert!(!VOXA_EXE_BYTES.is_empty());
        assert!(!WORKER_PY_BYTES.is_empty());
        assert!(!REQUIREMENTS_BYTES.is_empty());
        assert!(!ICON_ICO_BYTES.is_empty());
        assert!(!ICON_PNG_BYTES.is_empty());
        assert!(!LICENSE_BYTES.is_empty());
        assert!(load_setup_icon().is_some());
    }
}
