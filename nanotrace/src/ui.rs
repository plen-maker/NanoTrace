//! The Ctrl+Alt+0 status window: local vs latest vault version, the latest
//! commit and its message, and a manual "Frissítés" for when the automatic
//! background sync didn't run. Visual language borrowed from benchpeek's
//! Workspace UI (monochrome, Inter, rounded cards), drawn with the painter
//! rather than default egui widgets so it looks the same on macOS and GNOME.

use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

use anyhow::anyhow;
use eframe::egui::{
    self, text::LayoutJob, Align2, Color32, CornerRadius, FontData, FontDefinitions, FontFamily,
    FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2,
};

use crate::sync::{self, Commit, Report, State};

const BG: Color32 = Color32::from_rgb(0x0E, 0x10, 0x11);
const CARD: Color32 = Color32::from_rgb(0x17, 0x1A, 0x1C);
const RAISED: Color32 = Color32::from_rgb(0x20, 0x24, 0x26);
const TEXT: Color32 = Color32::from_rgb(0xF2, 0xF2, 0xF0);
const TEXT2: Color32 = Color32::from_rgb(0xA2, 0xA6, 0xA8);
const TEXT3: Color32 = Color32::from_rgb(0x72, 0x78, 0x7B);
const PRIMARY: Color32 = Color32::from_rgb(0xEC, 0xED, 0xEB);
const ON_PRIMARY: Color32 = Color32::from_rgb(0x0D, 0x0F, 0x10);
const GREEN: Color32 = Color32::from_rgb(0x5C, 0xC8, 0x8A);
const AMBER: Color32 = Color32::from_rgb(0xE8, 0xB5, 0x4A);
const RED: Color32 = Color32::from_rgb(0xE5, 0x6B, 0x5F);

const IC_BOX: char = '\u{e061}';
const IC_MONITOR: char = '\u{e11d}';
const IC_HISTORY: char = '\u{e1f5}';
const IC_REFRESH: char = '\u{e145}';
const IC_X: char = '\u{e1b2}';

const W: f32 = 440.0;
const H: f32 = 392.0;
const PAD: f32 = 20.0;
/// macOS draws its traffic-light buttons over the content (full-size
/// content view); keep the header text clear of them.
const MAC_INSET: f32 = if cfg!(target_os = "macos") { 72.0 } else { 0.0 };

enum Msg {
    Latest(Result<Commit, String>),
    Synced(Result<(Commit, Option<Report>), String>),
}

struct App {
    state: State,
    latest: Option<Result<Commit, String>>,
    busy: bool,
    result: Option<(Color32, String)>,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    ctx: egui::Context,
    shot: Option<(std::path::PathBuf, u32)>,
}

fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let add = |f: &mut FontDefinitions, name: &str, bytes: &'static [u8]| {
        f.font_data.insert(name.into(), Arc::new(FontData::from_static(bytes)));
    };
    add(&mut fonts, "inter", include_bytes!("../assets/Inter-Regular.ttf"));
    add(&mut fonts, "inter_medium", include_bytes!("../assets/Inter-Medium.ttf"));
    add(&mut fonts, "inter_semibold", include_bytes!("../assets/Inter-SemiBold.ttf"));
    add(&mut fonts, "mono", include_bytes!("../assets/JetBrainsMono-Regular.ttf"));
    add(&mut fonts, "icons", include_bytes!("../assets/lucide.ttf"));
    fonts.families.insert(FontFamily::Proportional, vec!["inter".into(), "icons".into()]);
    fonts.families.insert(FontFamily::Monospace, vec!["mono".into(), "inter".into()]);
    for name in ["inter_medium", "inter_semibold", "icons"] {
        fonts.families.insert(FontFamily::Name(name.into()), vec![name.into(), "inter".into()]);
    }
    ctx.set_fonts(fonts);
}

fn font(size: f32, family: &str) -> FontId {
    match family {
        "inter" => FontId::proportional(size),
        "mono" => FontId::monospace(size),
        f => FontId::new(size, FontFamily::Name(f.into())),
    }
}

impl App {
    fn new(ctx: egui::Context) -> Self {
        install_fonts(&ctx);
        let (tx, rx) = channel();
        let shot = std::env::var_os("NANOTRACE_SHOT").map(|p| (p.into(), 0));
        let mut app = App { state: sync::load_state(), latest: None, busy: false, result: None, tx, rx, ctx, shot };
        app.check();
        app
    }

    fn spawn(&mut self, f: impl FnOnce() -> Msg + Send + 'static) {
        self.busy = true;
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        std::thread::spawn(move || {
            let _ = tx.send(f());
            ctx.request_repaint();
        });
    }

    fn check(&mut self) {
        self.spawn(|| Msg::Latest(sync::token().and_then(|t| sync::latest(&t)).map_err(|e| format!("{e:#}"))));
    }

    fn refresh(&mut self) {
        self.result = None;
        self.spawn(|| Msg::Synced(sync::run(true).map_err(|e| format!("{e:#}"))));
    }

    fn poll(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            self.busy = false;
            match msg {
                Msg::Latest(r) => self.latest = Some(r),
                Msg::Synced(r) => {
                    self.state = sync::load_state();
                    match r {
                        Ok((c, report)) => {
                            self.latest = Some(Ok(c));
                            let r = report.unwrap_or_default();
                            let text = if r.projects.is_empty() {
                                "Ezen a gépen nincs a vaultban szereplő projekt".to_string()
                            } else if r.updated.is_empty() && r.kept.is_empty() {
                                "Szinkronizálva · nem volt változás".to_string()
                            } else if r.kept.is_empty() {
                                format!("Szinkronizálva · {} fájl frissült", r.updated.len())
                            } else {
                                format!("{} fájl frissült · {} helyben újabb, megtartva", r.updated.len(), r.kept.len())
                            };
                            self.result = Some((TEXT2, text));
                        }
                        Err(e) => self.result = Some((RED, e)),
                    }
                }
            }
        }
    }

    fn screenshot_harness(&mut self, ctx: &egui::Context) {
        let Some((path, frames)) = &mut self.shot else { return };
        for ev in ctx.input(|i| i.raw.events.clone()) {
            if let egui::Event::Screenshot { image, .. } = ev {
                let _ = write_png(path, &image);
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        if self.latest.is_some() && !self.busy {
            *frames += 1;
            if *frames == 20 {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
            }
        }
        ctx.request_repaint();
    }
}

fn short(sha: &str) -> &str {
    &sha[..sha.len().min(7)]
}

fn ago(secs: u64) -> String {
    let d = sync::now_secs().saturating_sub(secs);
    match d {
        0..=59 => "szinkronizálva épp most".into(),
        60..=3599 => format!("szinkronizálva {} perce", d / 60),
        3600..=86399 => format!("szinkronizálva {} órája", d / 3600),
        _ => format!("szinkronizálva {} napja", d / 86400),
    }
}

fn pretty_date(iso: &str) -> String {
    // "2026-10-13T14:33:29Z" -> "10.13. 14:33 UTC"
    match (iso.get(5..7), iso.get(8..10), iso.get(11..16)) {
        (Some(m), Some(d), Some(t)) => format!("{m}.{d}. {t} UTC"),
        _ => iso.to_string(),
    }
}

fn text(p: &egui::Painter, pos: Pos2, anchor: Align2, s: &str, f: FontId, c: Color32) -> Rect {
    p.text(pos, anchor, s, f, c)
}

/// Text wrapped to `width`, cut after `rows` lines with an ellipsis.
fn wrapped(p: &egui::Painter, pos: Pos2, s: &str, f: FontId, c: Color32, width: f32, rows: usize) -> f32 {
    let mut job = LayoutJob::simple(s.to_string(), f, c, width);
    job.wrap.max_rows = rows;
    job.wrap.overflow_character = Some('…');
    let g = p.layout_job(job);
    let h = g.size().y;
    p.galley(pos, g, c);
    h
}

fn card(p: &egui::Painter, r: Rect) {
    p.rect(r, CornerRadius::same(12), CARD, Stroke::new(1.0, Color32::from_white_alpha(18)), StrokeKind::Inside);
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.poll();
        let ctx = ui.ctx().clone();
        self.screenshot_harness(&ctx);
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        let full = ui.max_rect();
        let p = ui.painter().clone();
        let round = if cfg!(target_os = "macos") { 0 } else { 14 };
        p.rect(full, CornerRadius::same(round), BG, Stroke::new(1.0, Color32::from_white_alpha(26)), StrokeKind::Inside);

        // Header: drag handle for the whole undecorated window.
        let header = Rect::from_min_size(full.min, Vec2::new(full.width(), 64.0));
        if ui.interact(header, ui.id().with("drag"), Sense::drag()).drag_started() {
            ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        let hx = full.left() + PAD + MAC_INSET;
        let logo = Rect::from_min_size(Pos2::new(hx, full.top() + 16.0), Vec2::splat(32.0));
        p.rect_filled(logo, CornerRadius::same(9), RAISED);
        text(&p, logo.center(), Align2::CENTER_CENTER, &IC_BOX.to_string(), font(17.0, "icons"), TEXT);
        text(&p, Pos2::new(logo.right() + 11.0, logo.top() + 8.0), Align2::LEFT_CENTER, "NanoTrace", font(15.0, "inter_semibold"), TEXT);
        text(&p, Pos2::new(logo.right() + 11.0, logo.top() + 25.0), Align2::LEFT_CENTER, "Claude memória-vault", font(12.0, "inter"), TEXT3);

        let mut right = full.right() - PAD;
        if !cfg!(target_os = "macos") {
            let close = Rect::from_center_size(Pos2::new(right - 12.0, logo.center().y), Vec2::splat(26.0));
            let resp = ui.interact(close, ui.id().with("close"), Sense::click());
            if resp.hovered() {
                p.rect_filled(close, CornerRadius::same(7), RAISED);
            }
            text(&p, close.center(), Align2::CENTER_CENTER, &IC_X.to_string(), font(15.0, "icons"), if resp.hovered() { TEXT } else { TEXT3 });
            if resp.clicked() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            right = close.left() - 10.0;
        }

        let synced = self.state.synced.clone();
        let (accent, status) = match &self.latest {
            None => (TEXT3, "Ellenőrzés…"),
            Some(Err(_)) => (RED, "Nem elérhető"),
            Some(Ok(l)) if synced.as_ref().is_some_and(|s| s.sha == l.sha) => (GREEN, "Naprakész"),
            Some(Ok(_)) => (AMBER, "Frissítés elérhető"),
        };
        let galley = p.layout_no_wrap(status.to_string(), font(12.0, "inter_medium"), accent);
        let pill = Rect::from_min_size(
            Pos2::new(right - galley.size().x - 30.0, logo.center().y - 12.0),
            Vec2::new(galley.size().x + 30.0, 24.0),
        );
        p.rect_filled(pill, CornerRadius::same(12), accent.gamma_multiply(0.14));
        p.circle_filled(Pos2::new(pill.left() + 13.0, pill.center().y), 3.5, accent);
        p.galley(Pos2::new(pill.left() + 22.0, pill.center().y - galley.size().y / 2.0), galley, accent);

        // Version cards.
        let top = full.top() + 76.0;
        let cw = (full.width() - 2.0 * PAD - 12.0) / 2.0;
        let cards = [
            (IC_MONITOR, "EZEN A GÉPEN", synced.as_ref().map(|c| (c.version, c.sha.clone(), self.state.synced_at.map(ago).unwrap_or_default()))),
            (IC_BOX, "GITHUBON", match &self.latest {
                Some(Ok(l)) => Some((l.version, l.sha.clone(), pretty_date(&l.date))),
                _ => None,
            }),
        ];
        for (i, (icon, label, data)) in cards.into_iter().enumerate() {
            let r = Rect::from_min_size(Pos2::new(full.left() + PAD + i as f32 * (cw + 12.0), top), Vec2::new(cw, 100.0));
            card(&p, r);
            text(&p, Pos2::new(r.left() + 16.0, r.top() + 20.0), Align2::LEFT_CENTER, &icon.to_string(), font(13.0, "icons"), TEXT3);
            text(&p, Pos2::new(r.left() + 36.0, r.top() + 20.0), Align2::LEFT_CENTER, label, font(11.0, "inter_medium"), TEXT3);
            match data {
                Some((v, sha, when)) => {
                    let vr = text(&p, Pos2::new(r.left() + 15.0, r.top() + 52.0), Align2::LEFT_CENTER, &format!("v{v}"), font(28.0, "inter_semibold"), TEXT);
                    text(&p, Pos2::new(vr.right() + 8.0, r.top() + 55.0), Align2::LEFT_CENTER, short(&sha), font(12.0, "mono"), TEXT3);
                    text(&p, Pos2::new(r.left() + 16.0, r.top() + 81.0), Align2::LEFT_CENTER, &when, font(12.0, "inter"), TEXT2);
                }
                None => {
                    let msg = if i == 0 { "még nem szinkronizált" } else if self.latest.is_none() { "lekérdezés…" } else { "nem érem el" };
                    text(&p, Pos2::new(r.left() + 16.0, r.top() + 60.0), Align2::LEFT_CENTER, msg, font(13.0, "inter"), TEXT2);
                }
            }
        }

        // Latest commit.
        let cr = Rect::from_min_max(Pos2::new(full.left() + PAD, top + 112.0), Pos2::new(full.right() - PAD, full.bottom() - 74.0));
        card(&p, cr);
        text(&p, Pos2::new(cr.left() + 16.0, cr.top() + 20.0), Align2::LEFT_CENTER, &IC_HISTORY.to_string(), font(13.0, "icons"), TEXT3);
        text(&p, Pos2::new(cr.left() + 36.0, cr.top() + 20.0), Align2::LEFT_CENTER, "LEGUTÓBBI COMMIT", font(11.0, "inter_medium"), TEXT3);
        let inner_w = cr.width() - 32.0;
        match &self.latest {
            Some(Ok(l)) => {
                let mut lines = l.message.lines();
                let title = lines.next().unwrap_or("").trim();
                let body: Vec<&str> = lines.filter(|s| !s.trim().is_empty() && !s.starts_with("Co-Authored-By:")).collect();
                let th = wrapped(&p, Pos2::new(cr.left() + 16.0, cr.top() + 36.0), title, font(14.0, "inter_medium"), TEXT, inner_w, 2);
                if !body.is_empty() {
                    let rows = ((cr.bottom() - 14.0 - (cr.top() + 44.0 + th)) / 17.0).floor().max(1.0) as usize;
                    wrapped(&p, Pos2::new(cr.left() + 16.0, cr.top() + 44.0 + th), &body.join(" "), font(12.0, "inter"), TEXT2, inner_w, rows);
                }
            }
            Some(Err(e)) => {
                wrapped(&p, Pos2::new(cr.left() + 16.0, cr.top() + 36.0), e, font(12.5, "inter"), RED, inner_w, 4);
            }
            None => {
                text(&p, Pos2::new(cr.left() + 16.0, cr.top() + 48.0), Align2::LEFT_CENTER, "lekérdezés…", font(13.0, "inter"), TEXT2);
            }
        }

        // Footer: result / hint on the left, the refresh button on the right.
        let fy = full.bottom() - 37.0;
        let btn = Rect::from_min_size(Pos2::new(full.right() - PAD - 128.0, fy - 18.0), Vec2::new(128.0, 36.0));
        let resp = ui.interact(btn, ui.id().with("refresh"), Sense::click());
        let enabled = !self.busy;
        let fill = if !enabled { RAISED } else if resp.hovered() { Color32::WHITE } else { PRIMARY };
        p.rect_filled(btn, CornerRadius::same(10), fill);
        let fg = if enabled { ON_PRIMARY } else { TEXT2 };
        if self.busy {
            ui.put(Rect::from_center_size(Pos2::new(btn.left() + 22.0, btn.center().y), Vec2::splat(14.0)), egui::Spinner::new().size(14.0).color(fg));
        } else {
            text(&p, Pos2::new(btn.left() + 22.0, btn.center().y), Align2::CENTER_CENTER, &IC_REFRESH.to_string(), font(14.0, "icons"), fg);
        }
        text(&p, Pos2::new(btn.left() + 38.0, btn.center().y), Align2::LEFT_CENTER, "Frissítés", font(13.5, "inter_semibold"), fg);
        if enabled && resp.clicked() {
            self.refresh();
        }
        if resp.hovered() && enabled {
            ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
        }

        let (c, msg) = match (&self.result, &self.state.last_error) {
            (Some((c, m)), _) => (*c, m.clone()),
            (None, Some(e)) => (RED, format!("Az automatikus szinkron hibázott: {e}")),
            (None, None) => (TEXT3, "Ctrl+Alt+0 megnyit · Esc bezár".to_string()),
        };
        let mut job = LayoutJob::simple(msg, font(12.0, "inter"), c, btn.left() - full.left() - PAD - 14.0);
        job.wrap.max_rows = 2;
        job.wrap.overflow_character = Some('…');
        let g = p.layout_job(job);
        p.galley(Pos2::new(full.left() + PAD, btn.center().y - g.size().y / 2.0), g, c);
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }
}

/// Minimal RGBA PNG writer for the NANOTRACE_SHOT dev aid.
fn write_png(path: &std::path::Path, img: &egui::ColorImage) -> std::io::Result<()> {
    use std::io::Write;
    fn crc(data: &[u8]) -> u32 {
        let mut c = 0xffff_ffffu32;
        for &b in data {
            c ^= b as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
            }
        }
        !c
    }
    let chunk = |out: &mut Vec<u8>, tag: &[u8], data: &[u8]| {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let mut td = tag.to_vec();
        td.extend_from_slice(data);
        out.extend_from_slice(&td);
        out.extend_from_slice(&crc(&td).to_be_bytes());
    };
    let [w, h] = img.size;
    let mut raw = Vec::with_capacity((w * 4 + 1) * h);
    for y in 0..h {
        raw.push(0);
        for px in &img.pixels[y * w..(y + 1) * w] {
            raw.extend_from_slice(&px.to_srgba_unmultiplied());
        }
    }
    let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    z.write_all(&raw)?;
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&(w as u32).to_be_bytes());
    ihdr.extend_from_slice(&(h as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &z.finish()?);
    chunk(&mut out, b"IEND", &[]);
    std::fs::write(path, out)
}

pub fn run() -> anyhow::Result<()> {
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("NanoTrace")
        .with_app_id("nanotrace")
        .with_inner_size([W, H])
        .with_resizable(false)
        .with_transparent(true);
    viewport = if cfg!(target_os = "macos") {
        viewport.with_fullsize_content_view(true).with_titlebar_shown(false).with_title_shown(false)
    } else {
        viewport.with_decorations(false)
    };
    let opts = eframe::NativeOptions { viewport, ..Default::default() };
    let result = eframe::run_native(
        "NanoTrace",
        opts,
        Box::new(|cc| {
            crate::single::listen(cc.egui_ctx.clone());
            Ok(Box::new(App::new(cc.egui_ctx.clone())))
        }),
    );
    crate::single::cleanup();
    result.map_err(|e| anyhow!("{e}"))
}
