//! The fullscreen layer-shell overlay window: input wiring, capture plumbing,
//! and live refresh from Hyprland events.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::{Rc, Weak};

use gtk4 as gtk;
use gtk::gdk;
use gtk::glib;
use gtk::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use crate::capture::{self, CaptureHandle};
use crate::ipc;
use crate::layout::RingParams;
use crate::model::Snapshot;
use crate::ui::ring::RingView;

/// Refresh interval for the ring-focused workspace's thumbnails.
const RECAPTURE_INTERVAL_MS: u64 = 150;

struct Overlay {
    window: gtk::ApplicationWindow,
    ring: RingView,
    capture: Option<CaptureHandle>,
    /// Captures requested but not yet answered; used as backpressure so the
    /// periodic refresh never outruns the sequential capture worker.
    pending: Cell<usize>,
    /// Delivered textures, kept across snapshot refreshes to avoid flicker.
    textures: RefCell<HashMap<u64, (gdk::Texture, bool)>>,
}

pub fn build(
    app: &gtk::Application,
    snapshot: &Snapshot,
    params: RingParams,
) -> gtk::ApplicationWindow {
    let window = gtk::ApplicationWindow::new(app);
    window.init_layer_shell();
    window.set_layer(Layer::Overlay);
    window.set_namespace(Some("hyprPanopticon"));
    window.set_keyboard_mode(KeyboardMode::Exclusive);
    for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
        window.set_anchor(edge, true);
    }
    // Cover the whole output, ignoring other layers' exclusive zones (bars).
    window.set_exclusive_zone(-1);
    if let Some(monitor) = find_gdk_monitor(&snapshot.monitor_name) {
        window.set_monitor(Some(&monitor));
    }

    apply_css();
    window.add_css_class("panopticon");

    let ring = RingView::default();
    ring.set_params(params);
    window.set_child(Some(&ring));

    // Capture worker; on failure previews stay schematic.
    let capture_worker = match capture::spawn() {
        Ok(pair) => Some(pair),
        Err(e) => {
            eprintln!("hyprPanopticon: live thumbnails disabled: {e:#}");
            None
        }
    };

    let overlay = Rc::new(Overlay {
        window: window.clone(),
        ring: ring.clone(),
        capture: capture_worker.as_ref().map(|(handle, _)| handle.clone()),
        pending: Cell::new(0),
        textures: RefCell::new(HashMap::new()),
    });

    if let Some((_, results)) = capture_worker {
        spawn_result_consumer(results, Rc::downgrade(&overlay));
    }

    overlay.apply_snapshot(snapshot);
    overlay.request_captures(all_addrs(snapshot));

    spawn_event_refresh(&overlay);
    start_recapture_timer(&overlay);
    wire_input(&overlay);

    // Keep the Overlay alive for as long as the window exists.
    let anchor = RefCell::new(Some(overlay));
    window.connect_destroy(move |_| {
        anchor.borrow_mut().take();
    });

    window
}

impl Overlay {
    fn apply_snapshot(&self, snapshot: &Snapshot) {
        self.ring.set_snapshot(snapshot);

        // Re-apply already-delivered textures to the fresh previews.
        let textures = self.textures.borrow();
        for preview in self.ring.all_previews() {
            for addr in preview.window_addrs() {
                if let Some((texture, y_invert)) = textures.get(&addr) {
                    preview.set_texture(addr, texture.clone(), *y_invert);
                }
            }
        }
        drop(textures);

        // Click a preview: switch to that workspace (or toggle the special)
        // and close.
        for preview in self.ring.all_previews() {
            let gesture = gtk::GestureClick::new();
            let win = self.window.clone();
            let ws_id = preview.ws_id();
            let ws_name = preview.ws_name();
            gesture.connect_released(move |_, _, _, _| {
                switch_and_close(&win, ws_id, &ws_name);
            });
            preview.add_controller(gesture);
        }
    }

    fn request_captures(&self, addrs: Vec<u64>) {
        if let Some(capture) = &self.capture {
            self.pending.set(self.pending.get() + addrs.len());
            for addr in addrs {
                capture.request(addr);
            }
        }
    }

    fn deliver(&self, addr: u64, texture: gdk::Texture, y_invert: bool) {
        self.textures
            .borrow_mut()
            .insert(addr, (texture.clone(), y_invert));
        for preview in self.ring.all_previews() {
            if preview.set_texture(addr, texture.clone(), y_invert) {
                break;
            }
        }
    }

    fn refresh(&self) {
        match ipc::snapshot::take() {
            Ok(snapshot) => {
                // Drop textures of windows that no longer exist.
                let alive: HashSet<u64> = all_addrs(&snapshot).into_iter().collect();
                self.textures.borrow_mut().retain(|a, _| alive.contains(a));

                self.apply_snapshot(&snapshot);
                let missing: Vec<u64> = {
                    let textures = self.textures.borrow();
                    alive
                        .iter()
                        .copied()
                        .filter(|a| !textures.contains_key(a))
                        .collect()
                };
                self.request_captures(missing);
            }
            Err(e) => eprintln!("hyprPanopticon: snapshot refresh failed: {e:#}"),
        }
    }
}

fn all_addrs(snapshot: &Snapshot) -> Vec<u64> {
    snapshot
        .workspaces
        .iter()
        .chain(snapshot.specials.iter())
        .flat_map(|w| w.windows.iter().map(|win| win.addr))
        .collect()
}

fn spawn_result_consumer(
    results: async_channel::Receiver<capture::CaptureResult>,
    overlay: Weak<Overlay>,
) {
    glib::spawn_future_local(async move {
        while let Ok(result) = results.recv().await {
            let Some(overlay) = overlay.upgrade() else { break };
            overlay.pending.set(overlay.pending.get().saturating_sub(1));
            if let Some(frame) = result.frame {
                if let Some(texture) = capture::frame_to_texture(&frame) {
                    overlay.deliver(result.addr, texture, frame.y_invert);
                }
            }
        }
    });
}

fn spawn_event_refresh(overlay: &Rc<Overlay>) {
    let events = ipc::events::spawn();
    let overlay = Rc::downgrade(overlay);
    glib::spawn_future_local(async move {
        while let Ok(()) = events.recv().await {
            // Coalesce event bursts into one refresh.
            while events.try_recv().is_ok() {}
            let Some(overlay) = overlay.upgrade() else { break };
            if !overlay.window.is_visible() {
                break;
            }
            overlay.refresh();
        }
    });
}

/// Periodically re-capture the windows of the ring-focused workspace so the
/// big preview stays live while the overlay is open.
fn start_recapture_timer(overlay: &Rc<Overlay>) {
    let weak = Rc::downgrade(overlay);
    glib::timeout_add_local(
        std::time::Duration::from_millis(RECAPTURE_INTERVAL_MS),
        move || {
            let Some(overlay) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            if !overlay.window.is_visible() {
                return glib::ControlFlow::Break;
            }
            // Wait until the worker drained the previous batch.
            if overlay.pending.get() > 0 {
                return glib::ControlFlow::Continue;
            }
            let focused = overlay.ring.focused_ws_id();
            if let Some(preview) = overlay
                .ring
                .previews()
                .into_iter()
                .find(|p| Some(p.ws_id()) == focused)
            {
                overlay.request_captures(preview.window_addrs());
            }
            glib::ControlFlow::Continue
        },
    );
}

fn wire_input(overlay: &Rc<Overlay>) {
    let window = &overlay.window;
    let ring = &overlay.ring;

    // Keyboard: rotate / commit / close.
    let keys = gtk::EventControllerKey::new();
    {
        let ring = ring.clone();
        let win = window.clone();
        keys.connect_key_pressed(move |_, key, _, _| {
            match key {
                gdk::Key::Escape | gdk::Key::q => win.close(),
                gdk::Key::Left | gdk::Key::Up | gdk::Key::h | gdk::Key::k => ring.rotate(-1),
                gdk::Key::Right | gdk::Key::Down | gdk::Key::l | gdk::Key::j => ring.rotate(1),
                gdk::Key::Return | gdk::Key::KP_Enter | gdk::Key::space => {
                    if let Some(id) = ring.focused_ws_id() {
                        switch_and_close(&win, id, "");
                    }
                }
                // Digit keys (main row and keypad) toggle the numbered
                // special workspace.
                key => {
                    let digit = key.to_unicode().and_then(|c| c.to_digit(10));
                    match digit.and_then(|d| ring.special_at(d as usize)) {
                        Some(preview) => {
                            switch_and_close(&win, preview.ws_id(), &preview.ws_name())
                        }
                        None => return glib::Propagation::Proceed,
                    }
                }
            }
            glib::Propagation::Stop
        });
    }
    window.add_controller(keys);

    // Scroll: rotate the ring one step per detent.
    let scroll = gtk::EventControllerScroll::new(
        gtk::EventControllerScrollFlags::VERTICAL | gtk::EventControllerScrollFlags::DISCRETE,
    );
    {
        let ring = ring.clone();
        scroll.connect_scroll(move |_, _dx, dy| {
            if dy > 0.0 {
                ring.rotate(1);
            } else if dy < 0.0 {
                ring.rotate(-1);
            }
            glib::Propagation::Stop
        });
    }
    window.add_controller(scroll);
}

fn switch_and_close(window: &gtk::ApplicationWindow, ws_id: i32, ws_name: &str) {
    let result = if ws_id < 0 {
        ipc::toggle_special(ws_name)
    } else {
        ipc::switch_workspace(ws_id)
    };
    if let Err(e) = result {
        eprintln!("hyprPanopticon: workspace dispatch failed: {e:#}");
    }
    window.close();
}

fn find_gdk_monitor(connector: &str) -> Option<gdk::Monitor> {
    let display = gdk::Display::default()?;
    let monitors = display.monitors();
    for i in 0..monitors.n_items() {
        let monitor = monitors.item(i)?.downcast::<gdk::Monitor>().ok()?;
        if monitor.connector().as_deref() == Some(connector) {
            return Some(monitor);
        }
    }
    None
}

fn apply_css() {
    let provider = gtk::CssProvider::new();
    // Fully transparent window; the scrim is drawn by RingView so the dimming
    // level lives in one place.
    provider.load_from_string("window.panopticon { background-color: transparent; }");
    if let Some(display) = gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}
