//! `SIMCORE_WINDOW_SCRIPT`: scripted input for tests and for windows the OS will not let anyone capture or click
//! (a locked screen). Steps are separated by `;` and run one after the other, each once the previous step's frame
//! has been drawn:
//!
//! `wait:MS`, `click:X,Y`, `scroll:X,Y,DX,DY`, `drag:X,Y,DX,DY` (a live finger drag with release velocity),
//! `type:TEXT`, `key:backspace|enter`, `toggle` (Cmd+R), `dark` (Cmd+D), `snap:PATH` (PNG of the window as
//! presented), `quit`. Points are device points, like the headless `--click` steps.

use onesim_render::DragPhase;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, PartialEq)]
pub enum Step {
    Wait(Duration),
    Click(f64, f64),
    Scroll(f64, f64, f64, f64),
    Drag(f64, f64, f64, f64),
    DragPhase(DragPhase, f64, f64),
    Type(String),
    Key(String),
    Toggle,
    Dark,
    Snap(PathBuf),
    Quit,
}

fn nums<const N: usize>(s: &str) -> Result<[f64; N], String> {
    let v: Vec<f64> = s.split(',').map(|p| p.trim().parse().map_err(|_| format!("bad number in {s:?}"))).collect::<Result<_, _>>()?;
    v.try_into().map_err(|_| format!("expected {N} numbers, got {s:?}"))
}

pub fn parse(src: &str) -> Result<Vec<Step>, String> {
    src.split(';')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| {
            let (name, arg) = s.split_once(':').unwrap_or((s, ""));
            Ok(match name {
                "wait" => Step::Wait(Duration::from_millis(arg.parse().map_err(|_| format!("bad wait {arg:?}"))?)),
                "click" => nums::<2>(arg).map(|[x, y]| Step::Click(x, y))?,
                "scroll" => nums::<4>(arg).map(|[x, y, dx, dy]| Step::Scroll(x, y, dx, dy))?,
                "drag" => nums::<4>(arg).map(|[x, y, dx, dy]| Step::Drag(x, y, dx, dy))?,
                "drag-begin" => nums::<2>(arg).map(|[x, y]| Step::DragPhase(DragPhase::Begin, x, y))?,
                "drag-move" => nums::<2>(arg).map(|[x, y]| Step::DragPhase(DragPhase::Move, x, y))?,
                "drag-end" => nums::<2>(arg).map(|[x, y]| Step::DragPhase(DragPhase::End, x, y))?,
                "type" => Step::Type(arg.to_string()),
                "key" => Step::Key(arg.to_string()),
                "toggle" => Step::Toggle,
                "dark" => Step::Dark,
                "snap" => Step::Snap(arg.into()),
                "quit" => Step::Quit,
                other => return Err(format!("unknown script step {other:?}")),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_step() {
        let got = parse("wait:200; click:10,20;scroll:1,2,0,50;drag:5,6,0,-40;type:hi there;key:enter;toggle;dark;snap:/tmp/a.png;quit").unwrap();
        assert_eq!(got.len(), 10);
        assert_eq!(got[0], Step::Wait(Duration::from_millis(200)));
        assert_eq!(got[1], Step::Click(10.0, 20.0));
        assert_eq!(got[4], Step::Type("hi there".into()));
        assert_eq!(got[8], Step::Snap("/tmp/a.png".into()));
    }

    #[test]
    fn parses_separate_drag_phases() {
        assert_eq!(
            parse("drag-begin:10,20;drag-move:30,40;drag-end:50,60").unwrap(),
            vec![Step::DragPhase(DragPhase::Begin, 10.0, 20.0), Step::DragPhase(DragPhase::Move, 30.0, 40.0), Step::DragPhase(DragPhase::End, 50.0, 60.0)]
        );
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse("click:1").is_err());
        assert!(parse("fly").is_err());
        assert!(parse("wait:soon").is_err());
    }
}
