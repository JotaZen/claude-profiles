#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::os::windows::fs::MetadataExt;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;

/// Elementos de ~/.claude que son configuracion personal (no credenciales)
/// y que tiene sentido replicar en un perfil nuevo.
const SHARED_ITEMS: &[&str] = &[
    "CLAUDE.md",
    "settings.json",
    "keybindings.json",
    "skills",
    "agents",
    "commands",
    "output-styles",
    "rules",
    "hooks",
];

// ---------------------------------------------------------------- rutas base

fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("C:\\"))
}

fn roaming() -> PathBuf {
    dirs::config_dir().unwrap_or_else(|| home().join("AppData").join("Roaming"))
}

fn home_claude_dir() -> PathBuf {
    home().join(".claude")
}

fn desktop_user_data() -> PathBuf {
    roaming().join("Claude")
}

fn app_dir() -> PathBuf {
    roaming().join("ClaudeProfiles")
}

fn app_config_path() -> PathBuf {
    app_dir().join("config.json")
}

// ------------------------------------------------------------------ modelos

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
struct Profile {
    id: String,
    label: String,
    #[serde(default = "default_color")]
    color: String,
    #[serde(default)]
    workdir: String,
}

fn default_color() -> String {
    "#d97757".into()
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
struct Config {
    profiles_root: String,
    portable_dir: String,
    profiles: Vec<Profile>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            profiles_root: home().join("ClaudeProfiles").to_string_lossy().into(),
            portable_dir: "C:\\ClaudePortable".into(),
            profiles: vec![],
        }
    }
}

fn load_config() -> Config {
    match fs::read_to_string(app_config_path()) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => Config::default(),
    }
}

fn save_config(cfg: &Config) -> Result<(), String> {
    fs::create_dir_all(app_dir()).map_err(e)?;
    let s = serde_json::to_string_pretty(cfg).map_err(e)?;
    fs::write(app_config_path(), s).map_err(e)
}

fn e<T: std::fmt::Display>(err: T) -> String {
    err.to_string()
}

// ------------------------------------------------------------------ helpers

fn is_reparse(p: &Path) -> bool {
    fs::symlink_metadata(p)
        .map(|m| m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0)
        .unwrap_or(false)
}

fn real_path(p: &Path) -> Option<PathBuf> {
    fs::canonicalize(p).ok().map(|c| {
        let s = c.to_string_lossy().to_string();
        PathBuf::from(s.trim_start_matches("\\\\?\\").to_string())
    })
}

fn same_path(a: &Path, b: &Path) -> bool {
    match (real_path(a), real_path(b)) {
        (Some(x), Some(y)) => {
            x.to_string_lossy().to_lowercase() == y.to_string_lossy().to_lowercase()
        }
        _ => false,
    }
}

fn run_hidden(program: &str, args: &[&str]) -> Result<String, String> {
    let out = Command::new(program)
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(e)?;
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    if out.status.success() {
        Ok(stdout)
    } else {
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        Err(format!(
            "{} fallo ({}): {}",
            program,
            out.status.code().unwrap_or(-1),
            if stderr.trim().is_empty() { stdout } else { stderr }
        ))
    }
}

fn powershell(script: &str) -> Result<String, String> {
    run_hidden(
        "powershell",
        &["-NoProfile", "-NonInteractive", "-Command", script],
    )
}

fn make_junction(link: &Path, target: &Path) -> Result<(), String> {
    fs::create_dir_all(target).map_err(e)?;
    if let Some(parent) = link.parent() {
        fs::create_dir_all(parent).map_err(e)?;
    }
    run_hidden(
        "cmd",
        &[
            "/C",
            "mklink",
            "/J",
            &link.to_string_lossy(),
            &target.to_string_lossy(),
        ],
    )
    .map(|_| ())
}

fn copy_dir_all(src: &Path, dst: &Path) -> Result<(), String> {
    fs::create_dir_all(dst).map_err(e)?;
    for entry in fs::read_dir(src).map_err(e)? {
        let entry = entry.map_err(e)?;
        let ty = entry.file_type().map_err(e)?;
        let to = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &to)?;
        } else if ty.is_file() {
            fs::copy(entry.path(), &to).map_err(e)?;
        }
    }
    Ok(())
}

fn slugify(label: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for c in label.to_lowercase().chars() {
        let mapped = match c {
            'a'..='z' | '0'..='9' => Some(c),
            '\u{e1}' | '\u{e0}' | '\u{e4}' | '\u{e2}' => Some('a'),
            '\u{e9}' | '\u{e8}' | '\u{eb}' | '\u{ea}' => Some('e'),
            '\u{ed}' | '\u{ec}' | '\u{ef}' | '\u{ee}' => Some('i'),
            '\u{f3}' | '\u{f2}' | '\u{f6}' | '\u{f4}' => Some('o'),
            '\u{fa}' | '\u{f9}' | '\u{fc}' | '\u{fb}' => Some('u'),
            '\u{f1}' => Some('n'),
            _ => None,
        };
        match mapped {
            Some(ch) => {
                out.push(ch);
                prev_dash = false;
            }
            None => {
                if !prev_dash && !out.is_empty() {
                    out.push('-');
                }
                prev_dash = true;
            }
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() {
        "perfil".into()
    } else {
        out
    }
}

// ------------------------------------------------------- lectura de cuentas

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
struct Account {
    email: Option<String>,
    org: Option<String>,
    org_role: Option<String>,
    plan: Option<String>,
    uuid: Option<String>,
}

fn read_json(p: &Path) -> Option<Value> {
    let s = fs::read_to_string(p).ok()?;
    serde_json::from_str(&s).ok()
}

fn str_of(v: &Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(|x| x.as_str())
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
}

fn read_code_account(code_dir: &Path) -> Option<Account> {
    let mut acc = Account::default();
    let mut found = false;

    if let Some(j) = read_json(&code_dir.join(".claude.json")) {
        if let Some(oa) = j.get("oauthAccount") {
            acc.email = str_of(oa, "emailAddress");
            acc.org = str_of(oa, "organizationName");
            acc.org_role = str_of(oa, "organizationRole");
            acc.uuid = str_of(oa, "accountUuid");
            found = acc.email.is_some() || acc.uuid.is_some();
        }
    }
    if let Some(c) = read_json(&code_dir.join(".credentials.json")) {
        if let Some(o) = c.get("claudeAiOauth") {
            acc.plan = str_of(o, "subscriptionType");
            found = true;
        }
    }
    if found {
        Some(acc)
    } else {
        None
    }
}

fn read_desktop_account(desktop_dir: &Path) -> Option<Account> {
    let j = read_json(&desktop_dir.join("config.json"))?;
    let uuid = str_of(&j, "lastKnownAccountUuid")?;
    Some(Account {
        uuid: Some(uuid),
        ..Default::default()
    })
}

// --------------------------------------------------------- estado reportado

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
struct ProfileView {
    id: String,
    label: String,
    color: String,
    workdir: String,
    code_dir: String,
    desktop_dir: String,
    code_logged_in: bool,
    code_account: Option<Account>,
    desktop_logged_in: bool,
    desktop_account: Option<Account>,
    is_default_code: bool,
    is_active_desktop: bool,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
struct StateView {
    profiles_root: String,
    portable_dir: String,
    profiles: Vec<ProfileView>,
    home_claude_dir: String,
    home_claude_exists: bool,
    desktop_user_data: String,
    desktop_managed: bool,
    desktop_running: bool,
    default_code_env: Option<String>,
    package_dir: Option<String>,
    package_version: Option<String>,
    portable_ready: bool,
    cli_found: bool,
    needs_setup: bool,
    hint: Option<String>,
}

fn profile_code_dir(cfg: &Config, id: &str) -> PathBuf {
    PathBuf::from(&cfg.profiles_root).join(id).join("code")
}

fn profile_desktop_dir(cfg: &Config, id: &str) -> PathBuf {
    PathBuf::from(&cfg.profiles_root).join(id).join("desktop")
}

fn desktop_running() -> bool {
    run_hidden("tasklist", &["/NH", "/FI", "IMAGENAME eq Claude.exe"])
        .map(|s| s.to_lowercase().contains("claude.exe"))
        .unwrap_or(false)
}

fn env_default_code() -> Option<String> {
    let out = run_hidden(
        "reg",
        &["query", "HKCU\\Environment", "/v", "CLAUDE_CONFIG_DIR"],
    )
    .ok()?;
    let line = out
        .lines()
        .find(|l| l.to_uppercase().contains("CLAUDE_CONFIG_DIR"))?;
    let value = line
        .split_whitespace()
        .skip(2)
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn package_info() -> (Option<String>, Option<String>) {
    let script = "$p = Get-AppxPackage *Claude* | Select-Object -First 1; \
                  if ($p) { \"$($p.InstallLocation)|$($p.Version)\" }";
    match powershell(script) {
        Ok(s) => {
            let s = s.trim().to_string();
            if s.is_empty() {
                (None, None)
            } else {
                let mut it = s.splitn(2, '|');
                let loc = it.next().map(|x| x.to_string()).filter(|x| !x.is_empty());
                let ver = it.next().map(|x| x.to_string()).filter(|x| !x.is_empty());
                (loc, ver)
            }
        }
        Err(_) => (None, None),
    }
}

fn cli_found() -> bool {
    powershell("(Get-Command claude -ErrorAction SilentlyContinue).Source")
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false)
}

// ------------------------------------------------------------- comandos API

#[tauri::command]
fn get_state() -> Result<StateView, String> {
    let cfg = load_config();
    let dud = desktop_user_data();
    let managed = is_reparse(&dud);
    let env_default = env_default_code();

    let profiles: Vec<ProfileView> = cfg
        .profiles
        .iter()
        .map(|p| {
            let code_dir = profile_code_dir(&cfg, &p.id);
            let desk_dir = profile_desktop_dir(&cfg, &p.id);
            let is_default_code = env_default
                .as_ref()
                .map(|d| {
                    d.trim_end_matches('\\').to_lowercase()
                        == code_dir
                            .to_string_lossy()
                            .trim_end_matches('\\')
                            .to_lowercase()
                })
                .unwrap_or(false);
            ProfileView {
                id: p.id.clone(),
                label: p.label.clone(),
                color: p.color.clone(),
                workdir: if p.workdir.trim().is_empty() {
                    home().to_string_lossy().into()
                } else {
                    p.workdir.clone()
                },
                code_logged_in: code_dir.join(".credentials.json").exists(),
                code_account: read_code_account(&code_dir),
                desktop_logged_in: desk_dir.join("Local State").exists(),
                desktop_account: read_desktop_account(&desk_dir),
                is_default_code,
                is_active_desktop: managed && same_path(&dud, &desk_dir),
                code_dir: code_dir.to_string_lossy().into(),
                desktop_dir: desk_dir.to_string_lossy().into(),
            }
        })
        .collect();

    let (package_dir, package_version) = package_info();
    let portable_exe = PathBuf::from(&cfg.portable_dir).join("app").join("Claude.exe");
    let (needs_setup, hint) = next_step(&cfg, &profiles, managed);

    Ok(StateView {
        profiles_root: cfg.profiles_root.clone(),
        portable_dir: cfg.portable_dir.clone(),
        profiles,
        home_claude_dir: home_claude_dir().to_string_lossy().into(),
        home_claude_exists: home_claude_dir().exists(),
        desktop_user_data: dud.to_string_lossy().into(),
        desktop_managed: managed,
        desktop_running: desktop_running(),
        default_code_env: env_default,
        package_dir,
        package_version,
        portable_ready: portable_exe.exists(),
        cli_found: cli_found(),
        needs_setup,
        hint,
    })
}

/// Calcula el unico mensaje que vale la pena mostrar arriba.
fn next_step(cfg: &Config, profiles: &[ProfileView], managed: bool) -> (bool, Option<String>) {
    if cfg.profiles.is_empty() {
        return (
            true,
            Some("Configura el equipo en un paso: se crea un perfil con tu sesion actual y otro vacio para la segunda cuenta.".into()),
        );
    }
    if !managed && desktop_user_data().exists() {
        return (
            false,
            Some("Claude Desktop todavia guarda la sesion en %APPDATA%\\Claude. Pulsa \"Usar\" en el perfil que corresponda a la cuenta abierta ahora.".into()),
        );
    }
    if let Some(p) = profiles.iter().find(|p| !p.code_logged_in && !p.desktop_logged_in) {
        return (
            false,
            Some(format!(
                "El perfil \"{}\" esta vacio: pulsa \"Usar\" y despues abre Claude Desktop o una terminal para iniciar sesion con la otra cuenta.",
                p.label
            )),
        );
    }
    if !profiles.iter().any(|p| p.is_default_code) {
        return (
            false,
            Some("Ningun perfil esta marcado como activo. Pulsa \"Usar\" en el que quieras.".into()),
        );
    }
    (false, None)
}

#[tauri::command]
fn add_profile(label: String, color: String) -> Result<String, String> {
    let label = label.trim().to_string();
    if label.is_empty() {
        return Err("El perfil necesita un nombre.".into());
    }
    let mut cfg = load_config();
    let base = slugify(&label);
    let mut id = base.clone();
    let mut n = 2;
    while cfg.profiles.iter().any(|p| p.id == id) {
        id = format!("{}-{}", base, n);
        n += 1;
    }
    fs::create_dir_all(profile_code_dir(&cfg, &id)).map_err(e)?;
    fs::create_dir_all(profile_desktop_dir(&cfg, &id)).map_err(e)?;
    cfg.profiles.push(Profile {
        id: id.clone(),
        label,
        color,
        workdir: String::new(),
    });
    save_config(&cfg)?;
    write_launcher(&cfg, &id)?;
    Ok(id)
}

#[tauri::command]
fn update_profile(id: String, label: String, color: String, workdir: String) -> Result<(), String> {
    let mut cfg = load_config();
    {
        let p = cfg
            .profiles
            .iter_mut()
            .find(|p| p.id == id)
            .ok_or("Perfil no encontrado")?;
        p.label = label;
        p.color = color;
        p.workdir = workdir;
    }
    save_config(&cfg)?;
    write_launcher(&cfg, &id)?;
    Ok(())
}

#[tauri::command]
fn delete_profile(id: String, delete_files: bool) -> Result<(), String> {
    let mut cfg = load_config();
    if same_path(&desktop_user_data(), &profile_desktop_dir(&cfg, &id)) {
        return Err(
            "Este perfil esta activo en Claude Desktop. Cambia de perfil antes de borrarlo.".into(),
        );
    }
    if delete_files {
        let dir = PathBuf::from(&cfg.profiles_root).join(&id);
        if dir.exists() {
            fs::remove_dir_all(&dir).map_err(e)?;
        }
    }
    cfg.profiles.retain(|p| p.id != id);
    save_config(&cfg)
}

#[tauri::command]
fn set_profiles_root(path: String) -> Result<(), String> {
    let mut cfg = load_config();
    let p = PathBuf::from(path.trim());
    if p.as_os_str().is_empty() {
        return Err("Ruta vacia.".into());
    }
    fs::create_dir_all(&p).map_err(e)?;
    cfg.profiles_root = p.to_string_lossy().into();
    save_config(&cfg)
}

// --------------------------------------------------------------- Claude Code

fn write_launcher(cfg: &Config, id: &str) -> Result<(), String> {
    let p = cfg
        .profiles
        .iter()
        .find(|p| p.id == id)
        .ok_or("Perfil no encontrado")?;
    let code_dir = profile_code_dir(cfg, id);
    let workdir = if p.workdir.trim().is_empty() {
        home().to_string_lossy().to_string()
    } else {
        p.workdir.clone()
    };
    let dir = PathBuf::from(&cfg.profiles_root).join(id);
    fs::create_dir_all(&dir).map_err(e)?;
    let cmd = format!(
        "@echo off\r\n\
         title Claude Code - {label}\r\n\
         set \"CLAUDE_CONFIG_DIR={code}\"\r\n\
         cd /d \"{wd}\"\r\n\
         echo  Perfil: {label}\r\n\
         echo  CLAUDE_CONFIG_DIR=%CLAUDE_CONFIG_DIR%\r\n\
         echo.\r\n\
         claude %*\r\n",
        label = p.label,
        code = code_dir.to_string_lossy(),
        wd = workdir
    );
    fs::write(dir.join("claude-code.cmd"), cmd).map_err(e)?;
    Ok(())
}

#[tauri::command]
fn launch_code(id: String) -> Result<(), String> {
    let cfg = load_config();
    write_launcher(&cfg, &id)?;
    let p = cfg
        .profiles
        .iter()
        .find(|p| p.id == id)
        .ok_or("Perfil no encontrado")?;
    let launcher = PathBuf::from(&cfg.profiles_root)
        .join(&id)
        .join("claude-code.cmd");
    let launcher = launcher.to_string_lossy().to_string();
    let has_wt = run_hidden("where", &["wt.exe"]).is_ok();
    let res = if has_wt {
        Command::new("wt.exe")
            .args([
                "-w",
                "0",
                "new-tab",
                "--title",
                &format!("Claude - {}", p.label),
                "cmd",
                "/k",
                &launcher,
            ])
            .spawn()
    } else {
        Command::new("cmd")
            .args(["/C", "start", "", "cmd", "/k", &launcher])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
    };
    res.map(|_| ()).map_err(e)
}

#[tauri::command]
fn set_default_code(id: Option<String>) -> Result<(), String> {
    let cfg = load_config();
    match id {
        Some(id) if !id.is_empty() => {
            let dir = profile_code_dir(&cfg, &id);
            fs::create_dir_all(&dir).map_err(e)?;
            run_hidden("setx", &["CLAUDE_CONFIG_DIR", &dir.to_string_lossy()]).map(|_| ())
        }
        _ => {
            let _ = run_hidden(
                "reg",
                &["delete", "HKCU\\Environment", "/v", "CLAUDE_CONFIG_DIR", "/f"],
            );
            Ok(())
        }
    }
}

#[tauri::command]
fn copy_shared_config(id: String, from: Option<String>) -> Result<Vec<String>, String> {
    let cfg = load_config();
    let src = match from {
        Some(fid) if !fid.is_empty() => profile_code_dir(&cfg, &fid),
        _ => home_claude_dir(),
    };
    if !src.exists() {
        return Err(format!("No existe el origen: {}", src.display()));
    }
    let dst = profile_code_dir(&cfg, &id);
    fs::create_dir_all(&dst).map_err(e)?;
    let mut copied = vec![];
    for item in SHARED_ITEMS {
        let s = src.join(item);
        if !s.exists() {
            continue;
        }
        let d = dst.join(item);
        if s.is_dir() {
            copy_dir_all(&s, &d)?;
        } else {
            fs::copy(&s, &d).map_err(e)?;
        }
        copied.push(item.to_string());
    }
    Ok(copied)
}

#[tauri::command]
fn import_code_session(id: String) -> Result<String, String> {
    let cfg = load_config();
    let src = home_claude_dir();
    let dst = profile_code_dir(&cfg, &id);
    fs::create_dir_all(&dst).map_err(e)?;
    let cred = src.join(".credentials.json");
    if !cred.exists() {
        return Err("No hay credenciales en ~/.claude para importar.".into());
    }
    fs::copy(&cred, dst.join(".credentials.json")).map_err(e)?;
    let hj = home().join(".claude.json");
    if hj.exists() {
        fs::copy(&hj, dst.join(".claude.json")).map_err(e)?;
    }
    let copied = copy_shared_config(id, None)?;
    Ok(format!(
        "Sesion actual copiada al perfil ({} elementos compartidos).",
        copied.len()
    ))
}

// ------------------------------------------------------------ Claude Desktop

#[tauri::command]
fn claim_desktop(id: String) -> Result<String, String> {
    if desktop_running() {
        return Err("Cierra Claude Desktop antes de mover su perfil.".into());
    }
    let cfg = load_config();
    let dud = desktop_user_data();
    let target = profile_desktop_dir(&cfg, &id);

    if is_reparse(&dud) {
        return Err(
            "La carpeta ya esta gestionada. Usa \"Activar\" para cambiar de perfil.".into(),
        );
    }
    let had_data = dud.exists();
    if had_data {
        let non_empty = fs::read_dir(&target)
            .map(|mut r| r.next().is_some())
            .unwrap_or(false);
        if non_empty {
            return Err(
                "El perfil de destino ya tiene datos. Elige otro perfil o borra sus datos primero."
                    .into(),
            );
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(e)?;
        }
        let _ = fs::remove_dir(&target);
        fs::rename(&dud, &target).map_err(|err| {
            format!(
                "No se pudo mover {} a {}: {}",
                dud.display(),
                target.display(),
                err
            )
        })?;
    }
    make_junction(&dud, &target)?;
    Ok(if had_data {
        "Sesion actual movida al perfil y enlazada.".into()
    } else {
        "Carpeta enlazada al perfil. Al abrir Claude tendras que iniciar sesion.".into()
    })
}

#[tauri::command]
fn activate_desktop(id: String) -> Result<(), String> {
    if desktop_running() {
        return Err("Cierra Claude Desktop antes de cambiar de perfil.".into());
    }
    let cfg = load_config();
    let dud = desktop_user_data();
    let target = profile_desktop_dir(&cfg, &id);
    fs::create_dir_all(&target).map_err(e)?;

    if is_reparse(&dud) {
        fs::remove_dir(&dud).map_err(e)?;
    } else if dud.exists() {
        return Err(
            "%APPDATA%\\Claude todavia es una carpeta real. Usa \"Adoptar sesion actual\" en un perfil primero."
                .into(),
        );
    }
    make_junction(&dud, &target)
}

#[tauri::command]
fn unmanage_desktop() -> Result<String, String> {
    if desktop_running() {
        return Err("Cierra Claude Desktop primero.".into());
    }
    let cfg = load_config();
    let dud = desktop_user_data();
    if !is_reparse(&dud) {
        return Err("La carpeta no esta gestionada.".into());
    }
    let active = cfg
        .profiles
        .iter()
        .find(|p| same_path(&dud, &profile_desktop_dir(&cfg, &p.id)))
        .map(|p| profile_desktop_dir(&cfg, &p.id));
    fs::remove_dir(&dud).map_err(e)?;
    match active {
        Some(src) => {
            fs::rename(&src, &dud).map_err(e)?;
            fs::create_dir_all(&src).map_err(e)?;
            Ok("Perfil devuelto a %APPDATA%\\Claude. Claude Desktop vuelve a su modo normal.".into())
        }
        None => Ok("Enlace eliminado.".into()),
    }
}

#[tauri::command]
fn close_desktop() -> Result<(), String> {
    let _ = run_hidden("taskkill", &["/IM", "Claude.exe", "/F"]);
    Ok(())
}

#[tauri::command]
fn launch_desktop() -> Result<(), String> {
    Command::new("cmd")
        .args([
            "/C",
            "start",
            "",
            "shell:AppsFolder\\Claude_pzs8sxrjxfjjc!Claude",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map(|_| ())
        .map_err(e)
}

#[tauri::command]
async fn prepare_portable() -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let cfg = load_config();
        let (pkg, ver) = package_info();
        let pkg = pkg.ok_or("No se encontro el paquete de Claude Desktop (Get-AppxPackage).")?;
        let dest = PathBuf::from(&cfg.portable_dir);
        fs::create_dir_all(&dest).map_err(e)?;
        let dest_s = dest.to_string_lossy().to_string();
        let out = Command::new("robocopy")
            .args([
                pkg.as_str(),
                dest_s.as_str(),
                "/MIR",
                "/NFL",
                "/NDL",
                "/NJH",
                "/NJS",
                "/NP",
                "/R:1",
                "/W:1",
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map_err(e)?;
        let code = out.status.code().unwrap_or(-1);
        if code >= 8 {
            return Err(format!(
                "robocopy fallo (codigo {}). Revisa permisos sobre {}.",
                code, dest_s
            ));
        }
        let exe = dest.join("app").join("Claude.exe");
        if !exe.exists() {
            return Err(format!("La copia no contiene {}", exe.display()));
        }
        Ok(format!(
            "Copia portable lista en {} (version {}).",
            dest_s,
            ver.unwrap_or_else(|| "?".into())
        ))
    })
    .await
    .map_err(e)?
}

#[tauri::command]
fn launch_instance(id: String, portable: bool) -> Result<(), String> {
    let cfg = load_config();
    let user_data = profile_desktop_dir(&cfg, &id);
    let exe = if portable {
        PathBuf::from(&cfg.portable_dir).join("app").join("Claude.exe")
    } else {
        let (pkg, _) = package_info();
        PathBuf::from(pkg.ok_or("No se encontro el paquete instalado.")?)
            .join("app")
            .join("Claude.exe")
    };
    if !exe.exists() {
        return Err(format!("No existe {}", exe.display()));
    }
    fs::create_dir_all(&user_data).map_err(e)?;
    Command::new(&exe)
        .arg(format!("--user-data-dir={}", user_data.to_string_lossy()))
        .spawn()
        .map(|_| ())
        .map_err(|err| format!("No se pudo lanzar {}: {}", exe.display(), err))
}

// ------------------------------------------------- acciones de alto nivel

/// Deja el equipo listo sin preguntar nada: crea un perfil con la sesion que
/// ya existe y otro vacio para la segunda cuenta.
#[tauri::command]
fn auto_setup() -> Result<String, String> {
    let cfg = load_config();
    if !cfg.profiles.is_empty() {
        return Err("Ya hay perfiles configurados.".into());
    }
    let mut steps: Vec<String> = vec![];
    let mut pending: Option<String> = None;
    let first = add_profile("Personal".into(), default_color())?;

    if home_claude_dir().join(".credentials.json").exists() {
        import_code_session(first.clone())?;
        steps.push("sesion de Claude Code copiada".into());
    }
    if !is_reparse(&desktop_user_data()) && desktop_user_data().exists() {
        if desktop_running() {
            // No se puede mover la carpeta con la app abierta: se deja para despues.
            pending = Some(
                "Falta el escritorio: cierra Claude Desktop y pulsa \"Usar\" en Personal.".into(),
            );
        } else {
            claim_desktop(first.clone())?;
            steps.push("sesion de Claude Desktop adoptada".into());
        }
    }
    set_default_code(Some(first.clone()))?;

    let second = add_profile("Trabajo".into(), "#6fa8dc".into())?;
    let _ = copy_shared_config(second, Some(first));
    steps.push("perfil \"Trabajo\" creado, vacio".into());

    let mut msg = format!("Listo: {}.", steps.join(", "));
    if let Some(p) = pending {
        msg.push(' ');
        msg.push_str(&p);
    }
    Ok(msg)
}

/// Un solo boton por perfil: lo deja activo en el escritorio y en la CLI.
#[tauri::command]
fn use_profile(id: String, close_running: bool) -> Result<String, String> {
    let cfg = load_config();
    if !cfg.profiles.iter().any(|p| p.id == id) {
        return Err("Perfil no encontrado".into());
    }
    let dud = desktop_user_data();
    let mut steps: Vec<String> = vec![];

    // La carpeta del escritorio aun no esta gestionada: la adopta este perfil
    // si todavia no tiene datos propios.
    let needs_claim = !is_reparse(&dud) && dud.exists();
    let target_empty = fs::read_dir(profile_desktop_dir(&cfg, &id))
        .map(|mut r| r.next().is_none())
        .unwrap_or(true);

    if needs_claim && !target_empty {
        steps.push(
            "Claude Desktop sigue sin gestionar: este perfil ya tiene datos propios".into(),
        );
    } else {
        if desktop_running() {
            if !close_running {
                return Err("__NEEDS_CLOSE__".into());
            }
            close_desktop()?;
            std::thread::sleep(std::time::Duration::from_millis(900));
        }
        if needs_claim {
            claim_desktop(id.clone())?;
            steps.push("la sesion abierta del escritorio queda en este perfil".into());
        } else {
            activate_desktop(id.clone())?;
            let has_session = profile_desktop_dir(&cfg, &id).join("Local State").exists();
            steps.push(if has_session {
                "Claude Desktop apunta a este perfil".into()
            } else {
                "Claude Desktop apunta a este perfil (pedira iniciar sesion)".into()
            });
        }
    }

    set_default_code(Some(id.clone()))?;
    let code_ok = profile_code_dir(&cfg, &id).join(".credentials.json").exists();
    steps.push(if code_ok {
        "la CLI usa este perfil".into()
    } else {
        "la CLI usa este perfil (ejecuta /login la primera vez)".into()
    });

    Ok(steps.join("; ") + ".")
}

// -------------------------------------------------------------- utilidades

#[tauri::command]
fn open_path(path: String) -> Result<(), String> {
    let p = PathBuf::from(&path);
    if !p.exists() {
        fs::create_dir_all(&p).map_err(e)?;
    }
    Command::new("explorer")
        .arg(&path)
        .spawn()
        .map(|_| ())
        .map_err(e)
}

#[tauri::command]
fn pick_folder(current: String) -> Result<Option<String>, String> {
    let script = format!(
        "Add-Type -AssemblyName System.Windows.Forms | Out-Null; \
         $d = New-Object System.Windows.Forms.FolderBrowserDialog; \
         $d.SelectedPath = '{}'; \
         if ($d.ShowDialog() -eq 'OK') {{ $d.SelectedPath }}",
        current.replace('\'', "''")
    );
    let out = run_hidden(
        "powershell",
        &["-NoProfile", "-STA", "-Command", &script],
    )?;
    let out = out.trim().to_string();
    Ok(if out.is_empty() { None } else { Some(out) })
}

#[tauri::command]
fn create_shortcut(id: String) -> Result<String, String> {
    let cfg = load_config();
    let p = cfg
        .profiles
        .iter()
        .find(|p| p.id == id)
        .ok_or("Perfil no encontrado")?;
    write_launcher(&cfg, &id)?;
    let launcher = PathBuf::from(&cfg.profiles_root)
        .join(&id)
        .join("claude-code.cmd");
    let lnk = home()
        .join("Desktop")
        .join(format!("Claude Code - {}.lnk", p.label));
    let script = format!(
        "$w = New-Object -ComObject WScript.Shell; \
         $s = $w.CreateShortcut('{lnk}'); \
         $s.TargetPath = '{target}'; \
         $s.WorkingDirectory = '{wd}'; \
         $s.Description = 'Claude Code con el perfil {label}'; \
         $s.Save()",
        lnk = lnk.to_string_lossy().replace('\'', "''"),
        target = launcher.to_string_lossy().replace('\'', "''"),
        wd = home().to_string_lossy().replace('\'', "''"),
        label = p.label.replace('\'', "''")
    );
    powershell(&script)?;
    Ok(format!("Acceso directo creado en {}", lnk.display()))
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            get_state,
            auto_setup,
            use_profile,
            add_profile,
            update_profile,
            delete_profile,
            set_profiles_root,
            launch_code,
            set_default_code,
            copy_shared_config,
            import_code_session,
            claim_desktop,
            activate_desktop,
            unmanage_desktop,
            close_desktop,
            launch_desktop,
            prepare_portable,
            launch_instance,
            open_path,
            pick_folder,
            create_shortcut
        ])
        .run(tauri::generate_context!())
        .expect("error al iniciar Claude Profiles");
}
