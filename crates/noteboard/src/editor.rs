//! Launch the user's editor ($VISUAL, then $EDITOR, then nano).

use std::io;
use std::path::Path;
use std::process::Command;

fn editor_command() -> Vec<String> {
    let raw = ["VISUAL", "EDITOR"]
        .iter()
        .filter_map(|v| std::env::var(v).ok())
        .find(|v| !v.trim().is_empty())
        // nano shows its keys on screen, so it is a kinder default than vi.
        .unwrap_or_else(|| "nano".to_string());
    // Allow commands with arguments, e.g. `code --wait`.
    raw.split_whitespace().map(str::to_string).collect()
}

/// Open `path` in the editor and wait for it to exit.
pub fn open(path: &Path) -> io::Result<()> {
    let cmd = editor_command();
    let (program, args) = cmd
        .split_first()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no editor configured"))?;
    let status = Command::new(program).args(args).arg(path).status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "{} exited with {}",
            program, status
        )))
    }
}
