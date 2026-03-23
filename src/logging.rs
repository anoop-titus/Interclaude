use std::io::Write;
use std::sync::Mutex;
use std::sync::OnceLock;

static LOG_FILE: OnceLock<Mutex<Option<std::fs::File>>> = OnceLock::new();

/// Initialize the log file (call once at startup)
pub fn init() {
    let log_dir = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".interclaude");
    let _ = std::fs::create_dir_all(&log_dir);
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_dir.join("interclaude.log"))
        .ok();

    LOG_FILE.get_or_init(|| Mutex::new(file));
}

/// Log a message to ~/.interclaude/interclaude.log (safe during TUI)
pub fn log(msg: &str) {
    if let Some(lock) = LOG_FILE.get() {
        if let Ok(mut guard) = lock.lock() {
            if let Some(f) = guard.as_mut() {
                let ts = chrono::Local::now().format("%H:%M:%S");
                let _ = writeln!(f, "[{ts}] {msg}");
            }
        }
    }
}
