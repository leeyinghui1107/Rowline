//! `wl-dpi-probe` -- report the display environment.
//!
//! Enumerates every monitor, its size and its scale factor, and prints the
//! viewport the game would use on it.  This is the first thing to run when
//! validating the display adaptation on a new machine (see
//! `docs/09_DISPLAY_DPI_ADAPTATION.md` section 7, the test checklist).
//!
//! ```text
//! wl-dpi-probe
//! ```

use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::event_loop::EventLoop;
use winit::monitor::MonitorHandle;
use winit::window::WindowId;

fn describe(el: &ActiveEventLoop, m: &MonitorHandle) {
    let name = m.name().map(|n| n.to_string()).unwrap_or_default();
    let size = m.size();
    let scale = m.scale_factor() as f32;
    let pos = m.position();
    println!(
        "  monitor {:<28} {}x{} px  scale {:.2}  at ({}, {})",
        name, size.width, size.height, scale, pos.x, pos.y
    );
    // The viewport the game would use at that scale.  The board needs at
    // least 444x372 logical pixels before any clamping kicks in.
    let (need_w, need_h) = rowline_core::layout::min_client_size();
    let logical_w = size.width as f32 / scale;
    let logical_h = size.height as f32 / scale;
    println!(
        "      -> logical client area {:.0} x {:.0}  (board needs >= {} x {}: {})",
        logical_w,
        logical_h,
        need_w,
        need_h,
        if logical_w >= need_w as f32 && logical_h >= need_h as f32 {
            "ok"
        } else {
            "TOO SMALL"
        }
    );
    let _ = el;
}

fn main() {
    let event_loop = match EventLoop::new() {
        Ok(el) => el,
        Err(e) => {
            eprintln!("wl-dpi-probe: cannot create the event loop: {e}");
            std::process::exit(2);
        }
    };
    let mut monitors: Vec<MonitorHandle> = Vec::new();
    event_loop
        .run_app(&mut Probe {
            monitors: &mut monitors,
        })
        .ok();
}

struct Probe<'a> {
    monitors: &'a mut Vec<MonitorHandle>,
}

impl winit::application::ApplicationHandler for Probe<'_> {
    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        _event: WindowEvent,
    ) {
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        println!("wl-dpi-probe: display environment");
        for m in event_loop.available_monitors() {
            describe(event_loop, &m);
            self.monitors.push(m);
        }
        if let Some(primary) = event_loop.primary_monitor() {
            describe(event_loop, &primary);
        }
        println!("\n  {} monitor(s)", self.monitors.len());
        event_loop.exit();
    }
}
