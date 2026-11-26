//! The announcer: says a line out loud with the computer's own voice (the
//! `say` command on a Mac, Windows' built-in speech, or spd-say / espeak on
//! Linux, whichever is there). Nothing is downloaded or sent anywhere.
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

static CURRENT: Mutex<Option<Child>> = Mutex::new(None);

/// Says `text`. Countdown numbers cut off whatever is being said, so they stay on the beat.
pub fn speak(text: &str) {
    let urgent = text == "Go!" || text.chars().all(|c| c.is_ascii_digit());
    let Ok(mut current) = CURRENT.lock() else { return };
    if let Some(child) = current.as_mut() {
        match child.try_wait() {
            Ok(None) if urgent => {
                let _ = child.kill();
            }
            Ok(None) => return, // still talking: skip this line rather than pile them up
            _ => {}
        }
    }
    *current = start(text).ok();
}

fn start(text: &str) -> std::io::Result<Child> {
    let clean: String = text.chars().filter(|c| c.is_alphanumeric() || " ,.!?'-".contains(*c)).collect();
    #[cfg(target_os = "macos")]
    let mut cmd = {
        let mut c = Command::new("say");
        c.args(["-r", "210", &clean]);
        c
    };
    #[cfg(target_os = "windows")]
    let mut cmd = {
        use std::os::windows::process::CommandExt;
        let script = format!(
            "Add-Type -AssemblyName System.Speech; $s = New-Object System.Speech.Synthesis.SpeechSynthesizer; $s.Rate = 2; $s.Speak('{}')",
            clean.replace('\'', "''")
        );
        let mut c = Command::new("powershell");
        c.args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &script]);
        c.creation_flags(0x0800_0000); // no console window
        c
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut cmd = {
        let found = |name: &str| std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(name).exists()));
        let mut c = if found("spd-say") {
            Command::new("spd-say")
        } else {
            Command::new("espeak")
        };
        c.arg(&clean);
        c
    };
    cmd.stdout(Stdio::null()).stderr(Stdio::null()).spawn()
}
