#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
use rust_embed::RustEmbed;
use serde_json::{json, Value};
use std::borrow::Cow;
use std::cell::RefCell;
use std::io::BufRead;
use std::path::{Component, Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::rc::Rc;
use tao::dpi::LogicalSize;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
use tao::window::WindowBuilder;
use wry::http::Response;
use wry::WebViewBuilder;

#[derive(RustEmbed)]
#[folder = "ui/"]
struct Ui;

enum Ev {
    Msg(String),
    Js(String),
}

#[derive(Default)]
struct State {
    project: Option<PathBuf>,
    player: Option<Child>,
    stdin: Option<ChildStdin>,
    gen: Arc<std::sync::atomic::AtomicU64>,
}

/// Latest preview frame (version, JPEG bytes) streamed from the player.
type Frame = Arc<Mutex<(u64, Vec<u8>)>>;

type Shared = Rc<RefCell<State>>;

fn main() {
    #[cfg(target_os = "linux")]
    {
        // WebKitGTK's DMA-BUF renderer shows a blank page on some drivers.
        if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        }
    }
    let event_loop = EventLoopBuilder::<Ev>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    let window = WindowBuilder::new()
        .with_title("ZengEngine")
        .with_inner_size(LogicalSize::new(1440.0, 900.0))
        .with_min_inner_size(LogicalSize::new(1100.0, 640.0))
        .build(&event_loop)
        .unwrap_or_else(|e| die(e));
    #[cfg(target_os = "macos")]
    install_menu();

    let p = proxy.clone();
    let frame: Frame = Arc::new(Mutex::new((0, Vec::new())));
    let frame_proto = frame.clone();
    let view = WebViewBuilder::new()
        .with_custom_protocol("zeng".into(), move |_, req| {
            let path = req.uri().path().trim_start_matches('/');
            let path = if path.is_empty() { "index.html" } else { path };
            if path == "frame.jpg" {
                let since: u64 = req.uri().query().and_then(|q| q.strip_prefix("since=")).and_then(|v| v.parse().ok()).unwrap_or(0);
                let g = frame_proto.lock().unwrap();
                if g.1.is_empty() || g.0 == since {
                    return Response::builder().status(204).body(Cow::Borrowed(&b""[..])).unwrap();
                }
                return Response::builder()
                    .header("Content-Type", "image/jpeg")
                    .header("X-Frame", g.0.to_string())
                    .header("Access-Control-Expose-Headers", "X-Frame")
                    .body(Cow::Owned(g.1.clone()))
                    .unwrap();
            }
            match Ui::get(path) {
                Some(f) => {
                    let mime = match path.rsplit('.').next().unwrap_or("") {
                        "html" => "text/html; charset=utf-8",
                        "js" => "text/javascript; charset=utf-8",
                        "css" => "text/css; charset=utf-8",
                        "svg" => "image/svg+xml",
                        "png" => "image/png",
                        "gif" => "image/gif",
                        "mp3" => "audio/mpeg",
                        _ => "application/octet-stream",
                    };
                    Response::builder().header("Content-Type", mime).body(Cow::Owned(f.data.into_owned())).unwrap()
                }
                None => Response::builder().status(404).body(Cow::Borrowed(&b"not found"[..])).unwrap(),
            }
        })
        .with_ipc_handler(move |req| {
            let _ = p.send_event(Ev::Msg(req.body().clone()));
        })
        .with_url(if cfg!(windows) { "http://zeng.localhost/index.html" } else { "zeng://localhost/index.html" });
    // Linux has no plain window handle for a GTK window; attach to its box instead.
    #[cfg(target_os = "linux")]
    let view = {
        use tao::platform::unix::WindowExtUnix;
        use wry::WebViewBuilderExtUnix;
        match window.default_vbox() {
            Some(vbox) => view.build_gtk(vbox),
            None => Err(wry::Error::MessageSender),
        }
        .unwrap_or_else(|e| die(e))
    };
    #[cfg(not(target_os = "linux"))]
    let view = view.build(&window).unwrap_or_else(|e| die(e));

    let state: Shared = Rc::new(RefCell::new(State::default()));
    event_loop.run(move |event, _, flow| {
        *flow = ControlFlow::Wait;
        match event {
            Event::UserEvent(Ev::Js(code)) => {
                let _ = view.evaluate_script(&code);
            }
            Event::UserEvent(Ev::Msg(m)) => {
                if let Ok(v) = serde_json::from_str::<Value>(&m) {
                    let id = v["id"].clone();
                    let reply = match handle(&v, &state, &proxy, &frame) {
                        Ok(data) => json!({"id": id, "ok": true, "data": data}),
                        Err(e) => json!({"id": id, "ok": false, "error": e}),
                    };
                    if !id.is_null() {
                        let _ = view.evaluate_script(&format!("zengReply({})", reply));
                    }
                }
            }
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
                if let Some(mut c) = state.borrow_mut().player.take() {
                    let _ = c.kill();
                }
                *flow = ControlFlow::Exit;
            }
            _ => {}
        }
    });
}

fn die(e: impl std::fmt::Display) -> ! {
    eprintln!("ZengEngine could not start: {e}");
    if cfg!(target_os = "linux") {
        eprintln!("ZengEngine needs WebKitGTK 4.1 (package libwebkit2gtk-4.1-0) and GTK 3.");
    }
    if cfg!(windows) {
        eprintln!("ZengEngine needs the Microsoft Edge WebView2 Runtime.");
    }
    std::process::exit(1)
}

#[cfg(target_os = "macos")]
fn install_menu() {
    use muda::{Menu, PredefinedMenuItem, Submenu};
    let menu = Menu::new();
    let app = Submenu::new("ZengEngine", true);
    let _ = app.append_items(&[
        &PredefinedMenuItem::about(None, None),
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::hide(None),
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::quit(None),
    ]);
    let edit = Submenu::new("Edit", true);
    let _ = edit.append_items(&[
        &PredefinedMenuItem::undo(None),
        &PredefinedMenuItem::redo(None),
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::cut(None),
        &PredefinedMenuItem::copy(None),
        &PredefinedMenuItem::paste(None),
        &PredefinedMenuItem::select_all(None),
    ]);
    let _ = menu.append_items(&[&app, &edit]);
    menu.init_for_nsapp();
    std::mem::forget(menu);
}

fn home() -> PathBuf {
    std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from).unwrap_or_default()
}

fn exe_dir() -> PathBuf {
    std::env::current_exe().ok().and_then(|p| p.parent().map(|p| p.to_path_buf())).unwrap_or_default()
}

/// Finds a bundled resource (templates, player) both installed and in a dev checkout.
fn find_resource(name: &str) -> Option<PathBuf> {
    let e = exe_dir();
    let cands = [e.join(name), e.join("../Resources").join(name), e.join("..").join(name), e.join("../..").join(name)];
    cands.into_iter().find(|p| p.exists())
}

fn player_path() -> Option<PathBuf> {
    let exe = if cfg!(windows) { "zeng-player.exe" } else { "zeng-player" };
    find_resource(exe)
}

fn recent_file() -> PathBuf {
    home().join(".zengengine").join("recent.json")
}

fn read_recent() -> Vec<String> {
    std::fs::read_to_string(recent_file()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

fn remember(path: &Path) {
    let mut r = read_recent();
    let s = path.to_string_lossy().to_string();
    r.retain(|x| *x != s && Path::new(x).join("project.json").exists());
    r.insert(0, s);
    r.truncate(8);
    let _ = std::fs::create_dir_all(recent_file().parent().unwrap());
    let _ = std::fs::write(recent_file(), serde_json::to_string(&r).unwrap_or_default());
}

/// Joins a project-relative path, refusing anything that escapes the project folder.
fn safe_join(root: &Path, rel: &str) -> Result<PathBuf, String> {
    let rel = Path::new(rel);
    if rel.is_absolute() || rel.components().any(|c| matches!(c, Component::ParentDir | Component::Prefix(_))) {
        return Err("path is outside the project".into());
    }
    Ok(root.join(rel))
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)? {
        let e = e?;
        let name = e.file_name();
        if name == ".git" || name == ".DS_Store" {
            continue;
        }
        let dst = to.join(&name);
        if e.path().is_dir() {
            copy_dir(&e.path(), &dst)?;
        } else {
            std::fs::copy(e.path(), dst)?;
        }
    }
    Ok(())
}

fn project_of(state: &Shared) -> Result<PathBuf, String> {
    state.borrow().project.clone().ok_or_else(|| "no project is open".to_string())
}

fn list_dir(root: &Path, rel: &str) -> Result<Value, String> {
    let dir = safe_join(root, rel)?;
    let mut out = vec![];
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            out.push(json!({"name": name, "dir": e.path().is_dir()}));
        }
    }
    Ok(Value::Array(out))
}

fn open_project(dir: PathBuf, state: &Shared) -> Result<Value, String> {
    if !dir.join("project.json").exists() {
        return Err("That folder is not a ZengEngine project (no project.json).".into());
    }
    remember(&dir);
    state.borrow_mut().project = Some(dir.clone());
    Ok(json!({"dir": dir.to_string_lossy()}))
}

fn handle(v: &Value, state: &Shared, proxy: &EventLoopProxy<Ev>, frame: &Frame) -> Result<Value, String> {
    let cmd = v["cmd"].as_str().unwrap_or("");
    let arg = |k: &str| v[k].as_str().unwrap_or("").to_string();
    match cmd {
        "home" => {
            let templates: Vec<Value> = find_resource("templates")
                .and_then(|t| std::fs::read_dir(t).ok())
                .map(|rd| {
                    let mut v: Vec<_> = rd
                        .flatten()
                        .filter(|e| e.path().join("project.json").exists())
                        .map(|e| {
                            let meta: Value = std::fs::read_to_string(e.path().join("template.json"))
                                .ok()
                                .and_then(|s| serde_json::from_str(&s).ok())
                                .unwrap_or(json!({}));
                            json!({"id": e.file_name().to_string_lossy(), "title": meta["title"], "blurb": meta["blurb"], "order": meta["order"]})
                        })
                        .collect();
                    v.sort_by_key(|t| t["order"].as_i64().unwrap_or(99));
                    v
                })
                .unwrap_or_default();
            let recent: Vec<String> = read_recent().into_iter().filter(|p| Path::new(p).join("project.json").exists()).collect();
            Ok(json!({"recent": recent, "templates": templates, "player": player_path().is_some(),
                "auto": std::env::var("ZENG_OPEN").ok(), "autoplay": std::env::var("ZENG_AUTOPLAY").is_ok(), "autoexport": std::env::var("ZENG_EXPORT_TO").is_ok()}))
        }
        "new_project" => {
            let tpl = find_resource("templates").ok_or("templates folder not found")?.join(arg("template"));
            let name = arg("name");
            let name: String = name.chars().filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-' || *c == '_').collect();
            if name.trim().is_empty() {
                return Err("Give your game a name.".into());
            }
            let base = rfd::FileDialog::new().set_title("Where should the game folder go?").set_directory(home()).pick_folder();
            let Some(base) = base else { return Ok(Value::Null) };
            let dest = base.join(name.trim());
            if dest.exists() {
                return Err(format!("{} already exists. Pick another name.", dest.display()));
            }
            copy_dir(&tpl, &dest).map_err(|e| e.to_string())?;
            let _ = std::fs::remove_file(dest.join("template.json"));
            if let Ok(s) = std::fs::read_to_string(dest.join("project.json")) {
                if let Ok(mut p) = serde_json::from_str::<Value>(&s) {
                    p["name"] = json!(name.trim());
                    let _ = std::fs::write(dest.join("project.json"), serde_json::to_string_pretty(&p).unwrap());
                }
            }
            open_project(dest, state)
        }
        "open_project" => {
            let dir = rfd::FileDialog::new().set_title("Open a ZengEngine game folder").set_directory(home()).pick_folder();
            match dir {
                Some(d) => open_project(d, state),
                None => Ok(Value::Null),
            }
        }
        "open_path" => open_project(PathBuf::from(arg("path")), state),
        "read" => {
            let root = project_of(state)?;
            std::fs::read_to_string(safe_join(&root, &arg("rel"))?).map(Value::String).map_err(|e| e.to_string())
        }
        "write" => {
            let root = project_of(state)?;
            let p = safe_join(&root, &arg("rel"))?;
            if let Some(d) = p.parent() {
                std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
            }
            std::fs::write(p, arg("text")).map(|_| Value::Null).map_err(|e| e.to_string())
        }
        "exists" => {
            let root = project_of(state)?;
            Ok(json!(safe_join(&root, &arg("rel"))?.exists()))
        }
        "list" => list_dir(&project_of(state)?, &arg("rel")),
        "delete" => {
            let root = project_of(state)?;
            let p = safe_join(&root, &arg("rel"))?;
            let r = if p.is_dir() { std::fs::remove_dir_all(p) } else { std::fs::remove_file(p) };
            r.map(|_| Value::Null).map_err(|e| e.to_string())
        }
        "import_assets" => {
            let root = project_of(state)?;
            let files = rfd::FileDialog::new()
                .set_title("Add models, pictures or sounds")
                .add_filter("Game assets", &["glb", "gltf", "png", "jpg", "jpeg", "wav", "mp3", "ogg", "flac"])
                .pick_files();
            let mut added = vec![];
            for f in files.unwrap_or_default() {
                let ext = f.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
                let sub = match ext.as_str() {
                    "glb" | "gltf" => "models",
                    "png" | "jpg" | "jpeg" => "textures",
                    _ => "sounds",
                };
                let dir = root.join("assets").join(sub);
                std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                let name = f.file_name().unwrap().to_string_lossy().to_string();
                std::fs::copy(&f, dir.join(&name)).map_err(|e| e.to_string())?;
                added.push(format!("{sub}/{name}"));
            }
            Ok(json!(added))
        }
        "play" => {
            let root = project_of(state)?;
            let player = player_path().ok_or("zeng-player was not found next to the editor")?;
            stop_player(state, frame);
            let stage = v["stage"].as_bool().unwrap_or(false);
            let mut cmd = Command::new(player);
            cmd.arg(&root).stdout(Stdio::piped()).stderr(Stdio::null());
            if stage {
                cmd.args(["--stage", "640x480"]).stdin(Stdio::piped());
            }
            let mut child = cmd.spawn().map_err(|e| format!("could not start the game: {e}"))?;
            let out = child.stdout.take().unwrap();
            state.borrow_mut().stdin = child.stdin.take();
            state.borrow_mut().player = Some(child);
            let gen = state.borrow().gen.clone();
            let my_gen = gen.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
            let p = proxy.clone();
            let frame = frame.clone();
            std::thread::spawn(move || {
                let mut r = std::io::BufReader::new(out);
                let mut line = Vec::new();
                loop {
                    line.clear();
                    if r.read_until(b'\n', &mut line).unwrap_or(0) == 0 {
                        break;
                    }
                    let text = String::from_utf8_lossy(&line).trim_end().to_string();
                    let ev = if let Some(n) = text.strip_prefix("ZENGFRAME ") {
                        let mut buf = vec![0u8; n.parse().unwrap_or(0)];
                        if r.read_exact(&mut buf).is_err() {
                            break;
                        }
                        let mut g = frame.lock().unwrap();
                        g.0 += 1;
                        g.1 = buf;
                        continue;
                    } else if let Some(rest) = text.strip_prefix("ZENGERR\t") {
                        let (id, msg) = rest.split_once('\t').unwrap_or(("", rest));
                        json!({"type": "error", "id": id, "msg": msg})
                    } else if text == "ZENGRELOAD" {
                        json!({"type": "reloaded"})
                    } else {
                        continue;
                    };
                    let _ = p.send_event(Ev::Js(format!("zengEvent({ev})")));
                }
                // A newer run may already have replaced this one; only the current run reports "stopped".
                if gen.load(std::sync::atomic::Ordering::SeqCst) == my_gen {
                    let _ = p.send_event(Ev::Js("zengEvent({\"type\":\"stopped\"})".into()));
                }
            });
            Ok(Value::Null)
        }
        "stage_input" => {
            if let Some(si) = state.borrow_mut().stdin.as_mut() {
                let lines = v["lines"].as_array().map(|a| a.iter().filter_map(|l| l.as_str()).collect::<Vec<_>>().join("\n")).unwrap_or_default();
                if !lines.is_empty() {
                    let _ = si.write_all(format!("{lines}\n").as_bytes());
                    let _ = si.flush();
                }
            }
            Ok(Value::Null)
        }
        "stop" => {
            stop_player(state, frame);
            Ok(Value::Null)
        }
        "reveal" => {
            let root = project_of(state)?;
            let opener = if cfg!(target_os = "macos") { "open" } else if cfg!(windows) { "explorer" } else { "xdg-open" };
            let _ = Command::new(opener).arg(root).spawn();
            Ok(Value::Null)
        }
        "export" => export(state),
        _ => Err(format!("unknown command {cmd}")),
    }
}

fn stop_player(state: &Shared, frame: &Frame) {
    let mut st = state.borrow_mut();
    st.stdin = None;
    if let Some(mut c) = st.player.take() {
        let _ = c.kill();
        let _ = c.wait();
    }
    frame.lock().unwrap().1.clear();
}

fn export(state: &Shared) -> Result<Value, String> {
    let root = project_of(state)?;
    let player = player_path().ok_or("zeng-player was not found next to the editor")?;
    let meta: Value = serde_json::from_str(&std::fs::read_to_string(root.join("project.json")).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let name = meta["name"].as_str().unwrap_or("Game").to_string();
    let safe: String = name.chars().filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-').collect();
    let safe = if safe.trim().is_empty() { "Game".to_string() } else { safe.trim().to_string() };
    // ZENG_EXPORT_TO skips the folder dialog (used by automated checks).
    let picked = match std::env::var_os("ZENG_EXPORT_TO") {
        Some(p) => Some(PathBuf::from(p)),
        None => rfd::FileDialog::new().set_title("Choose where to save your finished game").set_directory(home()).pick_folder(),
    };
    let Some(out) = picked else { return Ok(Value::Null) };
    let exe_name = if cfg!(windows) { format!("{safe}.exe") } else { safe.clone() };
    let result;
    if cfg!(target_os = "macos") {
        let app = out.join(format!("{safe}.app"));
        let _ = std::fs::remove_dir_all(&app);
        let macos = app.join("Contents/MacOS");
        std::fs::create_dir_all(&macos).map_err(|e| e.to_string())?;
        std::fs::copy(&player, macos.join(&exe_name)).map_err(|e| e.to_string())?;
        copy_dir(&root, &app.join("Contents/Resources/project")).map_err(|e| e.to_string())?;
        let plist = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict>\n<key>CFBundleName</key><string>{safe}</string>\n<key>CFBundleExecutable</key><string>{exe_name}</string>\n<key>CFBundleIdentifier</key><string>app.zengengine.game.{id}</string>\n<key>CFBundlePackageType</key><string>APPL</string>\n<key>CFBundleVersion</key><string>1</string>\n<key>NSHighResolutionCapable</key><true/>\n</dict></plist>\n",
            id = safe.to_lowercase().replace(' ', "-")
        );
        std::fs::write(app.join("Contents/Info.plist"), plist).map_err(|e| e.to_string())?;
        result = app;
    } else {
        let dir = out.join(&safe);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        std::fs::copy(&player, dir.join(&exe_name)).map_err(|e| e.to_string())?;
        copy_dir(&root, &dir.join("project")).map_err(|e| e.to_string())?;
        result = dir;
    }
    Ok(json!({"path": result.to_string_lossy()}))
}
