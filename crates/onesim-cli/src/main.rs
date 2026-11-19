use anyhow::{bail, Context, Result};
use clap::{ArgMatches, CommandFactory, FromArgMatches, Parser, Subcommand};
use onesim_host::Host;
use onesim_protocol::{
    Appearance, CreateSessionParams, DynamicTypeSize, IosVersion, KeyboardEvent, PointerEvent, PointerKind, UiNode, Viewport,
};

#[derive(Parser)]
#[command(name = "onesim", version, about = "Run, inspect, and automate mobile UI")]
struct Cli {
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
    /// Run an Expo / React Native app served by Metro headlessly (same options as `run`).
    Expo {
        /// Metro dev server (default http://localhost:8081).
        #[arg(long)]
        url: Option<String>,
        /// Accepted for symmetry with other runners: there is no window, every run is headless.
        #[arg(long)]
        headless: bool,
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
    #[arg(long = "drag", value_name = "X,Y,DX,DY")]
    drags: Vec<String>,
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
}

/// One interaction, in command-line order.
#[derive(Debug, PartialEq)]
enum Step {
    Click(String),
    Type(String),
    Key(String),
    Scroll(String),
    Drag(String),
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
    all.sort_by_key(|(i, _)| *i);
    all.into_iter().map(|(_, s)| s).collect()
}

fn resolve_label(tree: &UiNode, spec: &str) -> Result<(f64, f64)> {
    onesim_render::resolve_label(tree, spec).map_err(anyhow::Error::msg)
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
            run(&matches, "run", CreateSessionParams { app_path, root_view, ..params(framework) }, opts, None)
        }
        Cmd::Expo { url, appearance, opts, .. } => {
            let appearance = match appearance.as_deref() {
                None | Some("light") => None,
                Some("dark") => Some(Appearance::Dark),
                Some(a) => bail!("unknown --appearance {a:?} (light, dark)"),
            };
            run(&matches, "expo", CreateSessionParams { metro_url: url, ..params("react-native") }, opts, appearance)
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
    }
}

/// Create a session from the shared options, drive it in command-line order, then shut down.
fn run(matches: &ArgMatches, name: &str, mut p: CreateSessionParams, o: Opts, appearance: Option<Appearance>) -> Result<()> {
    let steps = steps(matches.subcommand_matches(name).expect("subcommand"));
    p.viewport = match (o.width, o.height) {
        (None, None) => None,
        (w, h) => Some(Viewport { width: w.unwrap_or(402.0), height: h.unwrap_or(874.0) }),
    };
    p.scale = o.scale;
    p.appearance = Some(appearance.unwrap_or(if o.dark { Appearance::Dark } else { Appearance::Light }));
    p.dynamic_type_size = o
        .dynamic_type
        .map(|d| DynamicTypeSize::parse(&d).with_context(|| format!("unknown --dynamic-type {d:?}")))
        .transpose()?;
    p.device = o.device;
    p.os_version = o
        .ios
        .map(|v| IosVersion::parse(&v).with_context(|| format!("unknown --ios {v:?} (26.5, 27.2)")))
        .transpose()?;
    let mut host = Host::new();
    let id = host.create_session(p)?;
    let result = drive(&mut host, &id, o.screenshot, &steps, o.inspect);
    if result.is_err() {
        for e in host.errors(&id).unwrap_or_default() {
            eprintln!("[{}] {}", e.source, e.message);
        }
    }
    host.shutdown();
    result
}

fn drive(host: &mut Host, id: &str, shot: Option<String>, steps: &[Step], inspect: bool) -> Result<()> {
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
                let [x, y, dx, dy] = parse_nums::<4>(s).with_context(|| format!("--drag wants X,Y,DX,DY, got {s:?}"))?;
                let (hit, _) = host.pointer(id, PointerEvent { kind: PointerKind::Drag, x, y, dx, dy })?;
                if hit.is_none() {
                    bail!("nothing to drag at {x},{y}");
                }
            }
            Step::Scroll(s) => {
                let [x, y, dx, dy] = parse_nums::<4>(s).with_context(|| format!("--scroll wants X,Y,DX,DY, got {s:?}"))?;
                let (hit, _) = host.pointer(id, PointerEvent { kind: PointerKind::Scroll, x, y, dx, dy })?;
                if hit.is_none() {
                    bail!("nothing scrollable at {x},{y}");
                }
            }
        }
    }
    if let Some(path) = shot {
        let (w, h, png) = host.screenshot(id)?;
        std::fs::write(&path, png).with_context(|| format!("writing {path}"))?;
        eprintln!("wrote {path} ({w}x{h})");
    }
    if inspect {
        println!("{}", serde_json::to_string_pretty(&host.inspect(id)?.1)?);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_keep_command_line_order() {
        let m = Cli::command().get_matches_from([
            "onesim", "run", "app", "--click", "New todo", "--type", "hi", "--key", "enter", "--click", "Add",
            "--scroll", "1,2,0,50", "--type", "x",
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
