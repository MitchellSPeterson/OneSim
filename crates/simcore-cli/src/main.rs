use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use simcore_host::Host;
use simcore_protocol::{Appearance, CreateSessionParams, PointerEvent, PointerKind, UiNode, Viewport};

#[derive(Parser)]
#[command(name = "simcore", version, about = "Headless SwiftUI preview runtime")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Serve the JSON-lines external API on stdin/stdout (logs go to stderr).
    Serve,
    /// Build and run an app headlessly, optionally clicking, screenshotting and inspecting.
    Run {
        /// A .swift file or directory (or a prebuilt guest executable).
        app_path: String,
        #[arg(long)]
        root_view: Option<String>,
        /// Write a PNG screenshot (after any clicks).
        #[arg(long)]
        screenshot: Option<String>,
        /// Click a button by label, or at "X,Y" points. Repeatable.
        #[arg(long = "click", value_name = "LABEL_OR_X,Y")]
        clicks: Vec<String>,
        /// Print the UI tree as JSON on stdout.
        #[arg(long)]
        inspect: bool,
        #[arg(long, default_value_t = 393.0)]
        width: f64,
        #[arg(long, default_value_t = 852.0)]
        height: f64,
        #[arg(long, default_value_t = 2.0)]
        scale: f64,
        #[arg(long)]
        dark: bool,
    },
}

fn find_button<'a>(n: &'a UiNode, label: &str) -> Option<&'a UiNode> {
    if n.role.as_deref() == Some("button") && n.label.as_deref() == Some(label) {
        return Some(n);
    }
    n.children.iter().find_map(|c| find_button(c, label))
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn"))
        .target(env_logger::Target::Stderr)
        .init();
    match Cli::parse().cmd {
        Cmd::Serve => {
            let stdin = std::io::stdin();
            Host::new().serve(stdin.lock(), std::io::stdout().lock())?;
            Ok(())
        }
        Cmd::Run { app_path, root_view, screenshot, clicks, inspect, width, height, scale, dark } => {
            let mut host = Host::new();
            let id = host.create_session(CreateSessionParams {
                framework: "swiftui".into(),
                platform: "ios".into(),
                app_path,
                root_view,
                viewport: Some(Viewport { width, height }),
                scale: Some(scale),
                appearance: Some(if dark { Appearance::Dark } else { Appearance::Light }),
                prebuilt: false,
            })?;
            let result = drive(&mut host, &id, screenshot, &clicks, inspect);
            if result.is_err() {
                for e in host.errors(&id).unwrap_or_default() {
                    eprintln!("[{}] {}", e.source, e.message);
                }
            }
            host.shutdown();
            result
        }
    }
}

fn drive(host: &mut Host, id: &str, shot: Option<String>, clicks: &[String], inspect: bool) -> Result<()> {
    host.start_session(id).context("start_session")?;
    for c in clicks {
        let (x, y) = match c.split_once(',').and_then(|(x, y)| Some((x.trim().parse().ok()?, y.trim().parse().ok()?))) {
            Some(p) => p,
            None => {
                let f = find_button(&host.inspect(id)?.1, c).with_context(|| format!("no button labelled {c:?}"))?.frame;
                (f.x + f.width / 2.0, f.y + f.height / 2.0)
            }
        };
        let (hit, acted) = host.pointer(id, PointerEvent { kind: PointerKind::Click, x, y })?;
        log::info!("click {x},{y} -> hit={hit:?} action={acted}");
        if !acted {
            bail!("click at {x},{y} hit no actionable node");
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
