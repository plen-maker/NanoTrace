//! benchpeek: an open, extensible board-diagnostics cockpit.
//!
//! Two front ends share the crate: the monochrome Workspace UI (default) and
//! the original signal cockpit (`--classic`).

mod app;
mod files;
mod icons;
mod onboarding;
mod source;
mod theme;
mod ws;
mod yocto;

fn main() -> eframe::Result<()> {
    let classic = std::env::args().any(|a| a == "--classic");
    #[allow(unused_mut)]
    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_inner_size([1280.0, 800.0])
        .with_min_inner_size([960.0, 600.0])
        .with_title("benchpeek");
    // macOS: let the Workspace UI draw behind the titlebar, with only the
    // traffic lights floating over it - a unified toolbar, not a separate
    // OS titlebar strip above a second in-app one.
    //
    // Real NSVisualEffectView vibrancy (`with_transparent` + `window_vibrancy
    // ::apply_vibrancy`) was tried three times - applied in `Workspace::new`,
    // deferred to the first real frame, on both the `wgpu` and `glow`
    // backends, with the state forced to `Active`, confirmed `Ok(())` and
    // run from the user's own terminal (not just this tool's) - and never
    // rendered real blur, only an empty window (wgpu) or a flat grey one
    // (glow). Given up on it here; `ws::Workspace`'s sidebar/topbar paint
    // stays translucent (reads fine without real blur - see `theme::SIDEBAR`).
    #[cfg(target_os = "macos")]
    {
        viewport = viewport
            .with_fullsize_content_view(true)
            .with_title_shown(false)
            .with_titlebar_shown(false)
            .with_titlebar_buttons_shown(true);
    }
    let native_options = eframe::NativeOptions { viewport, ..Default::default() };
    if classic {
        eframe::run_native(
            "benchpeek",
            native_options,
            Box::new(|cc| {
                theme::apply(&cc.egui_ctx);
                Ok(Box::new(app::BenchpeekApp::default()))
            }),
        )
    } else {
        eframe::run_native(
            "benchpeek",
            native_options,
            Box::new(|cc| Ok(Box::new(ws::Workspace::new(cc)))),
        )
    }
}
