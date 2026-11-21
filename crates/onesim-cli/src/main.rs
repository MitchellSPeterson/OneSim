use anyhow::{bail, Context, Result};
use clap::{ArgMatches, CommandFactory, FromArgMatches, Parser, Subcommand};
use onesim_host::Host;
#[cfg(feature = "window")]
mod window;

use onesim_protocol::{
    Appearance, CreateSessionParams, DynamicTypeSize, IosVersion, KeyboardEvent, PointerEvent, PointerKind, UiNode, Viewport,
};

#[derive(Parser)]
#[command(name = "onesim", version, about = "Run, inspect, and automate mobile UI")]
struct Cli {
    /// Screenshot backend: auto (GPU, else CPU), gpu (or fail), cpu; default CPU. Overrides SIMCORE_RENDERER.
    #[arg(long, global = true, value_name = "auto|gpu|cpu")]
    renderer: Option<String>,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
#[allow(clippy::large_enum_variant)]
enum Cmd {
    /// Serve the JSON-lines external API on stdin/stdout (logs go to stderr), or with `--listen` the
    /// v2 JSON-RPC API over WebSocket (docs/api.md). No authentication: keep it on loopback.
    Serve {
        /// Address for the WebSocket API, e.g. 127.0.0.1:3030.
        #[arg(long, value_name = "ADDR")]
        listen: Option<String>,
    },
    /// Build and run an app headlessly, applying --click/--type/--key/--scroll in command-line
    /// order, then optionally screenshotting and inspecting.
    Run {
        /// A .swift file or directory, a .kt file or directory (Jetpack Compose, Android), or a prebuilt guest executable.
        app_path: String,
        #[arg(long)]
        root_view: Option<String>,
        #[command(flatten)]
        opts: Opts,
    },
    /// Open an app in a native window (build with `--features window`): GPU first with automatic CPU
    /// fallback, rendering only on input, resize or guest updates. Cmd+R / F5 switches GPU and CPU live,
    /// Cmd+D toggles dark mode. `SIMCORE_FRAME_STATS=1` prints per-second frame statistics.
    Window {
        /// A .swift file or directory, a .kt file or directory (Jetpack Compose, Android), or a prebuilt guest executable.
        app_path: String,
        #[arg(long)]
        root_view: Option<String>,
        #[command(flatten)]
        opts: Opts,
    },
    /// Run an Expo / React Native app served by Metro headlessly (same options as `run`).
    Expo {
        /// Metro dev server (default http://localhost:8081).
        #[arg(long)]
        url: Option<String>,
        /// Target platform (defaults to iOS).
        #[arg(long, default_value = "ios", value_parser = ["ios", "android"])]
        platform: String,
        /// Accepted for symmetry with other runners: headless is the default.
        #[arg(long, conflicts_with = "window")]
        headless: bool,
        /// Open a native window instead of running headlessly (build with `--features window`).
        #[arg(long)]
        window: bool,
        /// light (default) or dark; same as --dark.
        #[arg(long, value_name = "light|dark")]
        appearance: Option<String>,
        #[command(flatten)]
        opts: Opts,
    },
}

/// Options shared by `run` and `expo`.
#[derive(clap::Args)]
struct Opts {
    /// Write a PNG screenshot (after all interactions).
    #[arg(long)]
    screenshot: Option<String>,
    /// Click "X,Y" (points) or a control by label: button, navigation link, toggle (its switch),
    /// text field (placeholder; focuses it), or a stepper half as "LABEL#increment" / "LABEL#decrement"
    /// where LABEL is the stepper's current label. Repeatable.
    #[arg(long = "click", value_name = "LABEL_OR_X,Y")]
    clicks: Vec<String>,
    /// Type text into the focused text field. Repeatable.
    #[arg(long = "type", value_name = "TEXT")]
    texts: Vec<String>,
    /// Press a key in the focused text field: backspace or enter. Repeatable.
    #[arg(long = "key", value_name = "backspace|enter")]
    keys: Vec<String>,
    /// Scroll the scroller under (X,Y) by (DX,DY) points; positive DY reveals later content. Repeatable.
    #[arg(long = "scroll", value_name = "X,Y,DX,DY")]
    scrolls: Vec<String>,
    /// Drag a finger from (X,Y) by (DX,DY) points as one gesture: scrolls a scroller (content follows
    /// the finger) or swipes a sheet (down past the dismiss line dismisses, up expands). Repeatable.
    /// `LABEL,DX,DY` drags from a control or text by its label.
    #[arg(long = "drag", value_name = "X,Y,DX,DY")]
    drags: Vec<String>,
    /// Hold a finger still for a second at "X,Y" or on a control or text by label (SwiftUI long presses). Repeatable.
    #[arg(long = "long-press", value_name = "LABEL_OR_X,Y")]
    long_presses: Vec<String>,
    /// Pinch two fingers centred on "X,Y" or a labelled view, 100 pt apart, to SCALE times that distance. Repeatable.
    #[arg(long = "pinch", value_name = "LABEL_OR_X,Y,SCALE")]
    pinches: Vec<String>,
    /// Turn two fingers centred on "X,Y" or a labelled view by DEGREES (clockwise). Repeatable.
    #[arg(long = "rotate", value_name = "LABEL_OR_X,Y,DEGREES")]
    rotations: Vec<String>,
    /// Advance SwiftUI animations, transitions, and timelines by MS milliseconds. Repeatable.
    #[arg(long = "wait", value_name = "MS")]
    waits: Vec<String>,
    /// Move a SwiftUI app to a scene phase (through inactive, as iOS does). Repeatable.
    #[arg(long = "scene-phase", value_name = "active|inactive|background")]
    scene_phases: Vec<String>,
    /// Open a URL in a SwiftUI app (`onOpenURL`); a backgrounded app comes to the front first. Repeatable.
    #[arg(long = "open-url", value_name = "URL")]
    open_urls: Vec<String>,
    /// Launch a SwiftUI app to open this URL (it reaches `onOpenURL` after the first frame).
    #[arg(long = "launch-url", value_name = "URL")]
    launch_url: Option<String>,
    /// Print the UI tree as JSON on stdout.
    #[arg(long)]
    inspect: bool,
    /// Device profile (default iphone-17-pro: 402x874 pt @3x).
    #[arg(long)]
    device: Option<String>,
    /// Override the device viewport width (points).
    #[arg(long)]
    width: Option<f64>,
    /// Override the device viewport height (points).
    #[arg(long)]
    height: Option<f64>,
    /// Override the device pixel scale.
    #[arg(long)]
    scale: Option<f64>,
    #[arg(long)]
    dark: bool,
    /// Dynamic Type size: xSmall ... large (default) ... xxxLarge, accessibility1 ... accessibility5.
    #[arg(long = "dynamic-type", value_name = "SIZE")]
    dynamic_type: Option<String>,
    /// Simulated iOS release: 26.5 or 27.2 (default).
    #[arg(long = "ios", value_name = "VERSION")]
    ios: Option<String>,
    /// SwiftUI: text on the pasteboard at launch (what `PasteButton` pastes).
    #[arg(long, value_name = "TEXT")]
    pasteboard: Option<String>,
}

/// One interaction, in command-line order.
#[derive(Debug, PartialEq)]
enum Step {
    Click(String),
    Type(String),
    Key(String),
    Scroll(String),
    Drag(String),
    LongPress(String),
    Pinch(String),
    Rotate(String),
    Wait(String),
    ScenePhase(String),
    OpenUrl(String),
}

/// Collect the repeatable interaction flags in the order they were given.
fn steps(m: &ArgMatches) -> Vec<Step> {
    let mut all: Vec<(usize, Step)> = vec![];
    let mut add = |id: &str, f: fn(String) -> Step| {
        if let (Some(idx), Some(vals)) = (m.indices_of(id), m.get_many::<String>(id)) {
            all.extend(idx.zip(vals).map(|(i, v)| (i, f(v.clone()))));
        }
    };
    add("clicks", Step::Click);
    add("texts", Step::Type);
    add("keys", Step::Key);
    add("scrolls", Step::Scroll);
    add("drags", Step::Drag);
    add("long_presses", Step::LongPress);
    add("pinches", Step::Pinch);
    add("rotations", Step::Rotate);
    add("waits", Step::Wait);
    add("scene_phases", Step::ScenePhase);
    add("open_urls", Step::OpenUrl);
    all.sort_by_key(|(i, _)| *i);
    all.into_iter().map(|(_, s)| s).collect()
}

fn resolve_label(tree: &UiNode, spec: &str) -> Result<(f64, f64)> {
    onesim_render::resolve_label(tree, spec).map_err(anyhow::Error::msg)
}

/// "X,Y" in points, or the centre of a control or text by label.
fn point(host: &mut Host, id: &str, spec: &str) -> Result<(f64, f64)> {
    match parse_nums::<2>(spec) {
        Some([x, y]) => Ok((x, y)),
        None => resolve_label(&host.inspect(id)?.1, spec),
    }
}

fn parse_nums<const N: usize>(s: &str) -> Option<[f64; N]> {
    let v: Vec<f64> = s.split(',').map(|p| p.trim().parse().ok()).collect::<Option<_>>()?;
    v.try_into().ok()
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn"))
        .target(env_logger::Target::Stderr)
        .init();
    let matches = Cli::command().get_matches();
    let cli = Cli::from_arg_matches(&matches)?;
    if let Some(r) = &cli.renderer {
        r.parse::<onesim_render::BackendPref>().map_err(anyhow::Error::msg).context("--renderer")?;
        // Hosts (also the ones the WebSocket server creates per session) read the preference from the
        // environment; set it once, before any thread exists.
        std::env::set_var("SIMCORE_RENDERER", r);
    }
    let report = cli.renderer.is_some() || std::env::var_os("SIMCORE_RENDERER").is_some();
    match cli.cmd {
        Cmd::Serve { listen: Some(addr) } => {
            let listener = std::net::TcpListener::bind(&addr).with_context(|| format!("listen on {addr}"))?;
            eprintln!("listening on ws://{}/", listener.local_addr()?);
            onesim_host::serve_ws(listener, onesim_host::Server::new())?;
            Ok(())
        }
        Cmd::Serve { listen: None } => {
            let stdin = std::io::stdin();
            Host::new().serve(stdin.lock(), std::io::stdout().lock())?;
            Ok(())
        }
        Cmd::Run { app_path, root_view, opts } => {
            let framework = if is_kotlin(&app_path) { "compose" } else { "swiftui" };
            run(&matches, "run", CreateSessionParams { app_path, root_view, ..params(framework) }, opts, None, report)
        }
        Cmd::Window { app_path, root_view, opts } => {
            let framework = if is_kotlin(&app_path) { "compose" } else { "swiftui" };
            let title = format!("OneSim - {}", std::path::Path::new(&app_path).file_name().map_or(app_path.clone(), |n| n.to_string_lossy().into_owned()));
            open_window(&matches, "window", CreateSessionParams { app_path, root_view, ..params(framework) }, opts, None, title)
        }
        Cmd::Expo { url, platform, appearance, opts, window, .. } => {
            let appearance = match appearance.as_deref() {
                None | Some("light") => None,
                Some("dark") => Some(Appearance::Dark),
                Some(a) => bail!("unknown --appearance {a:?} (light, dark)"),
            };
            let p = CreateSessionParams { metro_url: url, platform, ..params("react-native") };
            if window {
                let title = format!("OneSim - expo {}", p.metro_url.as_deref().unwrap_or("http://localhost:8081"));
                return open_window(&matches, "expo", p, opts, appearance, title);
            }
            run(&matches, "expo", p, opts, appearance, report)
        }
    }
}

/// A `.kt` file, or a directory holding Kotlin sources (searched two levels deep): a Jetpack Compose app.
fn is_kotlin(path: &str) -> bool {
    fn walk(dir: &std::path::Path, depth: u32) -> bool {
        std::fs::read_dir(dir).into_iter().flatten().flatten().any(|e| {
            let p = e.path();
            p.extension().is_some_and(|x| x == "kt") || (depth > 0 && p.is_dir() && walk(&p, depth - 1))
        })
    }
    let p = std::path::Path::new(path);
    if p.is_dir() { walk(p, 2) } else { p.extension().is_some_and(|x| x == "kt") }
}

fn params(framework: &str) -> CreateSessionParams {
    CreateSessionParams {
        framework: framework.into(),
        platform: "ios".into(),
        app_path: String::new(),
        root_view: None,
        viewport: None,
        scale: None,
        appearance: None,
        dynamic_type_size: None,
        device: None,
        os_version: None,
        prebuilt: false,
        metro_url: None,
        pasteboard: None,
        launch_url: None,
    }
}

/// A not yet started session from the shared options, and the interaction steps to apply once it runs.
fn prepare(matches: &ArgMatches, name: &str, mut p: CreateSessionParams, o: &mut Opts, appearance: Option<Appearance>) -> Result<(Host, String, Vec<Step>)> {
    let steps = steps(matches.subcommand_matches(name).expect("subcommand"));
    let defaults = if p.platform == "android" || p.framework == "compose" {
        onesim_host::device(onesim_render::DEFAULT_ANDROID_DEVICE).expect("default Android device")
    } else {
        onesim_host::device(onesim_render::DEFAULT_DEVICE).expect("default iOS device")
    };
    let viewport = o.device.as_deref().and_then(onesim_host::device).unwrap_or(defaults).viewport;
    p.viewport = match (o.width, o.height) {
        (None, None) => None,
        (w, h) => Some(Viewport { width: w.unwrap_or(viewport.width), height: h.unwrap_or(viewport.height) }),
    };
    p.scale = o.scale;
    p.appearance = Some(appearance.unwrap_or(if o.dark { Appearance::Dark } else { Appearance::Light }));
    p.dynamic_type_size = o
        .dynamic_type
        .take()
        .map(|d| DynamicTypeSize::parse(&d).with_context(|| format!("unknown --dynamic-type {d:?}")))
        .transpose()?;
    p.device = o.device.take();
    p.pasteboard = o.pasteboard.take();
    p.launch_url = o.launch_url.take();
    p.os_version = o
        .ios
        .take()
        .map(|v| IosVersion::parse(&v).with_context(|| format!("unknown --ios {v:?} (26.5, 27.2)")))
        .transpose()?;
    let mut host = Host::new();
    let id = host.create_session(p)?;
    Ok((host, id, steps))
}

/// Create a session from the shared options, drive it in command-line order, then shut down.
fn run(matches: &ArgMatches, name: &str, p: CreateSessionParams, mut o: Opts, appearance: Option<Appearance>, report: bool) -> Result<()> {
    let (mut host, id, steps) = prepare(matches, name, p, &mut o, appearance)?;
    let result = drive(&mut host, &id, o.screenshot, &steps, o.inspect, report);
    if result.is_err() {
        for e in host.errors(&id).unwrap_or_default() {
            eprintln!("[{}] {}", e.source, e.message);
        }
    }
    host.shutdown();
    result
}

/// Start the session, apply any `--click`/`--type`/... steps, then hand it to the native window.
#[cfg(feature = "window")]
fn open_window(matches: &ArgMatches, name: &str, p: CreateSessionParams, mut o: Opts, appearance: Option<Appearance>, title: String) -> Result<()> {
    // `--renderer` was exported as SIMCORE_RENDERER in `main`; unset or `auto` means GPU first in a window.
    let pref = onesim_render::BackendPref::from_env().map_err(anyhow::Error::msg)?;
    let (rn, dark) = (p.framework == "react-native", o.dark || appearance == Some(Appearance::Dark));
    // The window follows the host clipboard (SwiftUI) unless `--pasteboard` fixed the app's pasteboard.
    let clipboard = p.framework == "swiftui" && o.pasteboard.is_none();
    let (mut host, id, steps) = prepare(matches, name, p, &mut o, appearance)?;
    // `drive` starts the session and applies the steps; the window then takes over the running session.
    if let Err(e) = drive(&mut host, &id, None, &steps, false, false) {
        host.shutdown();
        return Err(e);
    }
    let stats = std::env::var_os("SIMCORE_FRAME_STATS").is_some_and(|v| v != "0");
    window::run(host, id, window::Config { pref, rn, title, stats, dark, clipboard })
}

#[cfg(not(feature = "window"))]
fn open_window(_: &ArgMatches, _: &str, _: CreateSessionParams, _: Opts, _: Option<Appearance>, _: String) -> Result<()> {
    bail!("this build has no native window: rebuild with `cargo build -p onesim-cli --features window`")
}

fn drive(host: &mut Host, id: &str, shot: Option<String>, steps: &[Step], inspect: bool, report: bool) -> Result<()> {
    host.start_session(id).context("start_session")?;
    for step in steps {
        match step {
            Step::Click(c) => {
                let (x, y) = match parse_nums::<2>(c) {
                    Some([x, y]) => (x, y),
                    None => resolve_label(&host.inspect(id)?.1, c)?,
                };
                let (hit, acted) = host.pointer(id, PointerEvent::new(PointerKind::Click, x, y))?;
                log::info!("click {x},{y} -> hit={hit:?} action={acted}");
                if hit.is_none() {
                    bail!("click at {x},{y} hit no actionable node");
                }
            }
            Step::Type(t) => host.keyboard(id, KeyboardEvent::Text { text: t.clone() }).context("--type")?,
            Step::Key(k) => host.keyboard(id, KeyboardEvent::Key { key: k.clone() }).context("--key")?,
            Step::Drag(s) => {
                let bad = || format!("--drag wants X,Y,DX,DY or LABEL,DX,DY, got {s:?}");
                let (x, y, dx, dy) = match parse_nums::<4>(s) {
                    Some([x, y, dx, dy]) => (x, y, dx, dy),
                    None => {
                        let mut it = s.rsplitn(3, ',');
                        let (dy, dx, label) = (it.next(), it.next(), it.next().with_context(bad)?);
                        let [dx, dy] = parse_nums::<2>(&format!("{},{}", dx.unwrap_or(""), dy.unwrap_or(""))).with_context(bad)?;
                        let (x, y) = resolve_label(&host.inspect(id)?.1, label)?;
                        (x, y, dx, dy)
                    }
                };
                let (hit, _) = host.pointer(id, PointerEvent::moved(PointerKind::Drag, x, y, dx, dy))?;
                if hit.is_none() {
                    bail!("nothing to drag at {x},{y}");
                }
            }
            Step::LongPress(s) => {
                let (x, y) = point(host, id, s)?;
                if host.pointer(id, PointerEvent::new(PointerKind::LongPress, x, y))?.0.is_none() {
                    bail!("nothing to long-press at {x},{y}");
                }
            }
            Step::Pinch(s) | Step::Rotate(s) => {
                let rotate = matches!(step, Step::Rotate(_));
                let (at, amount) = s.rsplit_once(',').with_context(|| format!("wants LABEL_OR_X,Y,AMOUNT, got {s:?}"))?;
                let amount: f64 = amount.trim().parse().with_context(|| format!("bad amount in {s:?}"))?;
                let (x, y) = point(host, id, at)?;
                let ev = PointerEvent::new(if rotate { PointerKind::Rotate } else { PointerKind::Pinch }, x, y);
                let ev = if rotate { PointerEvent { degrees: Some(amount), ..ev } } else { PointerEvent { scale: Some(amount), ..ev } };
                if host.pointer(id, ev)?.0.is_none() {
                    bail!("no gesture view at {x},{y}");
                }
            }
            Step::Scroll(s) => {
                let [x, y, dx, dy] = parse_nums::<4>(s).with_context(|| format!("--scroll wants X,Y,DX,DY, got {s:?}"))?;
                let (hit, _) = host.pointer(id, PointerEvent::moved(PointerKind::Scroll, x, y, dx, dy))?;
                if hit.is_none() {
                    bail!("nothing scrollable at {x},{y}");
                }
            }
            Step::ScenePhase(s) => {
                let phase: onesim_protocol::ScenePhase = s.parse().map_err(anyhow::Error::msg)?;
                host.set_scene_phase(id, phase).context("--scene-phase")?;
            }
            Step::OpenUrl(u) => host.open_url(id, u).context("--open-url")?,
            Step::Wait(s) => {
                let ms: f64 = s.trim().parse().with_context(|| format!("--wait wants milliseconds, got {s:?}"))?;
                host.wait(id, ms).context("--wait")?;
            }
        }
    }
    if let Some(path) = shot {
        let (w, h, png) = host.screenshot(id)?;
        std::fs::write(&path, png).with_context(|| format!("writing {path}"))?;
        eprintln!("wrote {path} ({w}x{h})");
        if report {
            eprintln!("{}", renderer_line(&host.renderer_diagnostics()?));
        }
    }
    if inspect {
        println!("{}", serde_json::to_string_pretty(&host.inspect(id)?.1)?);
    }
    Ok(())
}

/// `gpu (Apple M1 Pro (Metal))` or `cpu (fallback: <reason>)`.
fn renderer_line(d: &onesim_render::Diagnostics) -> String {
    match &d.fallback_reason {
        Some(r) => format!("renderer: {} (fallback: {r})", d.backend),
        None => format!("renderer: {} ({})", d.backend, d.adapter),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renderer_line_formats() {
        let mut d = onesim_render::Diagnostics { backend: "gpu".into(), adapter: "Apple M1 Pro (Metal)".into(), ..Default::default() };
        assert_eq!(renderer_line(&d), "renderer: gpu (Apple M1 Pro (Metal))");
        d.backend = "cpu".into();
        d.fallback_reason = Some("no adapter".into());
        assert_eq!(renderer_line(&d), "renderer: cpu (fallback: no adapter)");
    }

    #[test]
    fn steps_keep_command_line_order() {
        let m = Cli::command().get_matches_from([
            "onesim", "run", "app", "--click", "New todo", "--type", "hi", "--key", "enter", "--click", "Add",
            "--scroll", "1,2,0,50", "--scene-phase", "background", "--open-url", "app://x", "--type", "x",
        ]);
        let got = steps(m.subcommand_matches("run").unwrap());
        assert_eq!(
            got,
            vec![
                Step::Click("New todo".into()),
                Step::Type("hi".into()),
                Step::Key("enter".into()),
                Step::Click("Add".into()),
                Step::Scroll("1,2,0,50".into()),
                Step::ScenePhase("background".into()),
                Step::OpenUrl("app://x".into()),
                Step::Type("x".into()),
            ]
        );
    }

    #[test]
    fn numbers() {
        assert_eq!(parse_nums::<2>("1, 2.5"), Some([1.0, 2.5]));
        assert_eq!(parse_nums::<2>("Typography"), None);
        assert_eq!(parse_nums::<4>("1,2,3"), None);
    }
}
