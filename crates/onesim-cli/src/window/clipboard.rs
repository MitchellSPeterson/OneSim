//! The host machine's clipboard, through the platform's own command-line tools (no extra dependency).
//! The window keeps a SwiftUI app's pasteboard in step with it (protocol section W).
use std::io::Write;
use std::process::{Command, Stdio};

/// (program, args) pairs to try in order.
#[cfg(target_os = "macos")]
const READ: &[(&str, &[&str])] = &[("pbpaste", &[])];
#[cfg(target_os = "macos")]
const WRITE: &[(&str, &[&str])] = &[("pbcopy", &[])];
#[cfg(target_os = "windows")]
const READ: &[(&str, &[&str])] = &[("powershell", &["-NoProfile", "-Command", "Get-Clipboard -Raw"])];
#[cfg(target_os = "windows")]
const WRITE: &[(&str, &[&str])] = &[("powershell", &["-NoProfile", "-Command", "[Console]::In.ReadToEnd() | Set-Clipboard"])];
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
const READ: &[(&str, &[&str])] = &[("wl-paste", &["--no-newline"]), ("xclip", &["-selection", "clipboard", "-o"]), ("xsel", &["-b", "-o"])];
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
const WRITE: &[(&str, &[&str])] = &[("wl-copy", &[]), ("xclip", &["-selection", "clipboard", "-i"]), ("xsel", &["-b", "-i"])];

/// The clipboard's text, or `None` when it holds none or no tool is available.
pub fn read() -> Option<String> {
    READ.iter().find_map(|(p, a)| {
        let out = Command::new(p).args(*a).stderr(Stdio::null()).output().ok().filter(|o| o.status.success())?;
        let t = String::from_utf8(out.stdout).ok()?;
        let t = if cfg!(target_os = "windows") { t.trim_end_matches(['\r', '\n']).to_string() } else { t };
        (!t.is_empty()).then_some(t)
    })
}

/// Puts `text` on the clipboard; false when no tool took it.
pub fn write(text: &str) -> bool {
    WRITE.iter().any(|(p, a)| {
        let Ok(mut child) = Command::new(p).args(*a).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).spawn() else {
            return false;
        };
        let wrote = child.stdin.take().is_some_and(|mut i| i.write_all(text.as_bytes()).is_ok());
        child.wait().is_ok_and(|s| s.success()) && wrote
    })
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    #[test]
    fn round_trips_through_the_system_clipboard() {
        let before = super::read();
        assert!(super::write("onesim clipboard test ✓"));
        assert_eq!(super::read().as_deref(), Some("onesim clipboard test ✓"));
        if let Some(b) = before {
            super::write(&b);
        }
    }
}
