use anyhow::{bail, Context, Result};
use clap::{ArgMatches, CommandFactory, FromArgMatches, Parser, Subcommand};
use simcore_host::Host;
use simcore_protocol::{
    Appearance, CreateSessionParams, KeyboardEvent, PointerEvent, PointerKind, UiNode, Viewport,
};

#[derive(Parser)]
#[command(name = "simcore", version, about = "Headless SwiftUI preview runtime")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
#[allow(clippy::large_enum_variant)]
enum Cmd {
    /// Serve the JSON-lines external API on stdin/stdout (logs go to stderr).
    Serve,
    /// Build and run an app headlessly, applying --click/--type/--key/--scroll in command-line
    /// order, then optionally screenshotting and inspecting.
    Run {
        /// A .swift file or directory (or a prebuilt guest executable).
        app_path: String,
        #[arg(long)]
        root_view: Option<String>,
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
    },
}

/// One interaction, in command-line order.
#[derive(Debug, PartialEq)]
enum Step {
    Click(String),
    Type(String),
    Key(String),
    Scroll(String),
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
    all.sort_by_key(|(i, _)| *i);
    all.into_iter().map(|(_, s)| s).collect()
}

fn find<'a>(n: &'a UiNode, pred: &dyn Fn(&UiNode) -> bool) -> Option<&'a UiNode> {
    if pred(n) {
        return Some(n);
    }
    n.children.iter().find_map(|c| find(c, pred))
}

/// Point to click for a label. Syntax: `Label` (button / toggle switch / text field placeholder),
/// `Label#increment` or `Label#decrement` (stepper whose label is `Label`).
fn resolve_label(tree: &UiNode, spec: &str) -> Result<(f64, f64)> {
    let center = |n: &UiNode| (n.frame.x + n.frame.width / 2.0, n.frame.y + n.frame.height / 2.0);
    if let Some((label, part)) = spec.rsplit_once('#') {
        if matches!(part, "increment" | "decrement") {
            let st = find(tree, &|n| n.role.as_deref() == Some("stepper") && n.label.as_deref() == Some(label))
                .with_context(|| format!("no stepper labelled {label:?}"))?;
            let b = find(st, &|n| n.id.ends_with(&format!("#{part}"))).context("stepper half not found")?;
            return Ok(center(b));
        }
    }
    let n = find(tree, &|n| {
        matches!(n.role.as_deref(), Some("button" | "toggle" | "textfield")) && n.label.as_deref() == Some(spec)
    })
    .with_context(|| format!("no button, toggle or text field labelled {spec:?}"))?;
    Ok(center(n))
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
        Cmd::Serve => {
            let stdin = std::io::stdin();
            Host::new().serve(stdin.lock(), std::io::stdout().lock())?;
            Ok(())
        }
        Cmd::Run { app_path, root_view, screenshot, inspect, device, width, height, scale, dark, .. } => {
            let run = matches.subcommand_matches("run").expect("run subcommand");
            let steps = steps(run);
            let viewport = match (width, height) {
                (None, None) => None,
                (w, h) => Some(Viewport { width: w.unwrap_or(402.0), height: h.unwrap_or(874.0) }),
            };
            let mut host = Host::new();
            let id = host.create_session(CreateSessionParams {
                framework: "swiftui".into(),
                platform: "ios".into(),
                app_path,
                root_view,
                viewport,
                scale,
                appearance: Some(if dark { Appearance::Dark } else { Appearance::Light }),
                device,
                prebuilt: false,
            })?;
            let result = drive(&mut host, &id, screenshot, &steps, inspect);
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
            "simcore", "run", "app", "--click", "New todo", "--type", "hi", "--key", "enter", "--click", "Add",
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
