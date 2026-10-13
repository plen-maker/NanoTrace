//! Login-time background process: syncs once the network is up, then every
//! 30 minutes. On macOS it also owns the Ctrl+Alt+0 hotkey (on GNOME/Wayland
//! an app can't grab global keys, so install.sh registers a GNOME custom
//! shortcut that launches the window directly instead).

use std::time::Duration;

fn sync_loop() {
    // Right after login the network may not be up yet: retry for ~5 minutes.
    for attempt in 0..10 {
        match crate::sync::run(false) {
            Ok(_) => break,
            Err(e) => {
                eprintln!("nanotrace: sync failed (attempt {}): {e:#}", attempt + 1);
                std::thread::sleep(Duration::from_secs(30));
            }
        }
    }
    loop {
        std::thread::sleep(Duration::from_secs(30 * 60));
        if let Err(e) = crate::sync::run(false) {
            eprintln!("nanotrace: sync failed: {e:#}");
        }
    }
}

pub fn run() -> anyhow::Result<()> {
    let worker = std::thread::spawn(sync_loop);
    #[cfg(target_os = "macos")]
    {
        let _ = worker;
        mac::hotkey_loop()
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = worker.join();
        Ok(())
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use global_hotkey::hotkey::{Code, HotKey, Modifiers};
    use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
    use winit::application::ApplicationHandler;
    use winit::event::WindowEvent;
    use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
    use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
    use winit::window::WindowId;

    struct Idle;
    impl ApplicationHandler for Idle {
        fn resumed(&mut self, _: &ActiveEventLoop) {}
        fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
    }

    /// The Carbon hotkey needs a running macOS run loop on the main thread;
    /// a window-less winit loop (no Dock icon) provides it.
    pub fn hotkey_loop() -> anyhow::Result<()> {
        let event_loop = EventLoop::builder().with_activation_policy(ActivationPolicy::Accessory).build()?;
        event_loop.set_control_flow(ControlFlow::Wait);
        let manager = GlobalHotKeyManager::new()?;
        let hotkey = HotKey::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::Digit0);
        manager.register(hotkey)?;
        let id = hotkey.id();
        GlobalHotKeyEvent::set_event_handler(Some(move |e: GlobalHotKeyEvent| {
            if e.id == id && e.state == HotKeyState::Pressed {
                if let Ok(exe) = std::env::current_exe() {
                    let _ = std::process::Command::new(exe).spawn();
                }
            }
        }));
        event_loop.run_app(&mut Idle)?;
        drop(manager);
        Ok(())
    }
}
