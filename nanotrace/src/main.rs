mod daemon;
mod single;
mod sync;
mod ui;

fn main() -> anyhow::Result<()> {
    match std::env::args().nth(1).as_deref() {
        Some("--daemon") => daemon::run(),
        Some("--sync") => {
            let (c, report) = sync::run(true)?;
            println!("vault v{} {} - {}", c.version, &c.sha[..7], c.message.lines().next().unwrap_or(""));
            let r = report.unwrap_or_default();
            for p in &r.projects {
                println!("project: {p}");
            }
            for f in &r.updated {
                println!("updated: {f}");
            }
            for f in &r.kept {
                println!("kept (newer locally): {f}");
            }
            Ok(())
        }
        Some("--help" | "-h") => {
            println!("nanotrace            status window (Ctrl+Alt+0)\nnanotrace --daemon   background sync at login + every 30 min\nnanotrace --sync     sync once and print what changed");
            Ok(())
        }
        _ => {
            if single::focus_existing() {
                return Ok(());
            }
            ui::run()
        }
    }
}
