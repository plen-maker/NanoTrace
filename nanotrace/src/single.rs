//! One status window at a time: a second launch (the hotkey pressed again)
//! just asks the open window to come to the front, then exits.

use std::io::Write;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;

fn sock_path() -> PathBuf {
    let dir = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    let user = std::env::var("USER").unwrap_or_default();
    dir.join(format!("nanotrace-{user}.sock"))
}

pub fn focus_existing() -> bool {
    match UnixStream::connect(sock_path()) {
        Ok(mut s) => s.write_all(b"focus").is_ok(),
        Err(_) => false,
    }
}

pub fn listen(ctx: eframe::egui::Context) {
    let path = sock_path();
    let _ = std::fs::remove_file(&path);
    let Ok(listener) = UnixListener::bind(&path) else { return };
    std::thread::spawn(move || {
        for _ in listener.incoming().flatten() {
            ctx.send_viewport_cmd(eframe::egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(eframe::egui::ViewportCommand::Focus);
            ctx.request_repaint();
        }
    });
}

pub fn cleanup() {
    let _ = std::fs::remove_file(sock_path());
}
