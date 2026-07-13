//! WorkspacePreview: a widget rendering one workspace's windows, either as
//! captured textures (when available) or schematic colored rectangles.

use gtk4 as gtk;
use gtk::gdk;
use gtk::glib;
use gtk::graphene;
use gtk::gsk;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

use crate::model::{Rect, WorkspaceModel};

const CORNER_RADIUS: f32 = 12.0;

mod imp {
    use super::*;
    use std::cell::{Cell, RefCell};

    #[derive(Default)]
    pub struct WorkspacePreview {
        pub model: RefCell<Option<WorkspaceModel>>,
        /// Monitor viewport size in logical px; window rects are clipped to it.
        pub viewport: Cell<(f64, f64)>,
        pub ring_focused: Cell<bool>,
        /// 1-based hotkey number shown in the badge of special previews.
        pub special_index: Cell<Option<usize>>,
        /// Show the workspace name/index badge on normal previews too.
        pub show_label: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for WorkspacePreview {
        const NAME: &'static str = "PanopticonWorkspacePreview";
        type Type = super::WorkspacePreview;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for WorkspacePreview {}

    impl WidgetImpl for WorkspacePreview {
        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let widget = self.obj();
            let w = widget.width() as f32;
            let h = widget.height() as f32;
            if w <= 0.0 || h <= 0.0 {
                return;
            }
            let bounds = graphene::Rect::new(0.0, 0.0, w, h);
            let rounded = gsk::RoundedRect::from_rect(bounds, CORNER_RADIUS);

            snapshot.push_rounded_clip(&rounded);
            snapshot.append_color(&gdk::RGBA::new(0.12, 0.12, 0.15, 0.92), &bounds);

            let (vw, vh) = self.viewport.get();
            if let Some(model) = self.model.borrow().as_ref() {
                if vw > 0.0 && vh > 0.0 {
                    let viewport = Rect { x: 0.0, y: 0.0, w: vw, h: vh };
                    let sx = w as f64 / vw;
                    let sy = h as f64 / vh;
                    for win in &model.windows {
                        let Some(clipped) = win.rect.clip(&viewport) else {
                            continue;
                        };
                        let dest = graphene::Rect::new(
                            (clipped.x * sx) as f32,
                            (clipped.y * sy) as f32,
                            (clipped.w * sx) as f32,
                            (clipped.h * sy) as f32,
                        );
                        match &win.texture {
                            Some(texture) => {
                                append_window_texture(snapshot, texture, win, &clipped, &dest)
                            }
                            None => {
                                let color = class_color(&win.class);
                                snapshot.append_color(&color, &dest);
                                let edge = gdk::RGBA::new(1.0, 1.0, 1.0, 0.25);
                                append_frame(snapshot, &dest, 1.0, &edge);
                            }
                        }
                    }
                }
            }
            snapshot.pop();

            // Badge on a dark pill, bottom-center. Special previews always
            // carry their digit hotkey; normal workspaces show their name
            // only when the show_workspace_index option is on.
            let badge = self.model.borrow().as_ref().and_then(|model| {
                match self.special_index.get() {
                    Some(i) => Some(format!("{i}: {}", model.name)),
                    None if self.show_label.get() => Some(model.name.clone()),
                    None => None,
                }
            });
            if let Some(text) = badge {
                let layout = widget.create_pango_layout(Some(&text));
                let (tw, th) = layout.pixel_size();
                let (tw, th) = (tw as f32, th as f32);
                let tx = (w - tw) / 2.0;
                let ty = h - th - 8.0;
                let pill = graphene::Rect::new(tx - 8.0, ty - 3.0, tw + 16.0, th + 6.0);
                let rounded = gsk::RoundedRect::from_rect(pill, pill.height() / 2.0);
                snapshot.push_rounded_clip(&rounded);
                snapshot.append_color(&gdk::RGBA::new(0.0, 0.0, 0.0, 0.65), &pill);
                snapshot.pop();
                snapshot.save();
                snapshot.translate(&graphene::Point::new(tx, ty));
                snapshot.append_layout(&layout, &gdk::RGBA::new(1.0, 1.0, 1.0, 0.95));
                snapshot.restore();
            }

            // Border: accent when ring-focused, amber for specials, subtle otherwise.
            let is_special = self.model.borrow().as_ref().map(|m| m.id < 0).unwrap_or(false);
            let (bw, color) = if self.ring_focused.get() {
                (3.0, gdk::RGBA::new(0.55, 0.75, 1.0, 1.0))
            } else if is_special {
                (1.5, gdk::RGBA::new(1.0, 0.75, 0.35, 0.7))
            } else {
                (1.0, gdk::RGBA::new(1.0, 1.0, 1.0, 0.18))
            };
            snapshot.append_border(&rounded, &[bw; 4], &[color; 4]);
        }
    }

    fn append_frame(snapshot: &gtk::Snapshot, rect: &graphene::Rect, width: f32, color: &gdk::RGBA) {
        let rr = gsk::RoundedRect::from_rect(*rect, 2.0);
        snapshot.append_border(&rr, &[width; 4], &[*color; 4]);
    }

    fn append_window_texture(
        snapshot: &gtk::Snapshot,
        texture: &gdk::Texture,
        win: &crate::model::WindowThumb,
        clipped: &Rect,
        dest: &graphene::Rect,
    ) {
        // The texture covers the full window rect; if the window sticks out of
        // the viewport only a sub-rect is visible. Draw the full texture scaled
        // to the full window's destination size, clipped to `dest`.
        let full = &win.rect;
        let scale_x = dest.width() as f64 / clipped.w;
        let scale_y = dest.height() as f64 / clipped.h;
        let full_dest = graphene::Rect::new(
            (dest.x() as f64 - (clipped.x - full.x) * scale_x) as f32,
            (dest.y() as f64 - (clipped.y - full.y) * scale_y) as f32,
            (full.w * scale_x) as f32,
            (full.h * scale_y) as f32,
        );
        snapshot.push_clip(dest);
        if win.y_invert {
            snapshot.save();
            snapshot.translate(&graphene::Point::new(0.0, full_dest.y() * 2.0 + full_dest.height()));
            snapshot.scale(1.0, -1.0);
            snapshot.append_texture(texture, &full_dest);
            snapshot.restore();
        } else {
            snapshot.append_texture(texture, &full_dest);
        }
        snapshot.pop();
    }

    /// Deterministic pastel color per window class for schematic rendering.
    fn class_color(class: &str) -> gdk::RGBA {
        let mut hash: u32 = 2166136261;
        for b in class.bytes() {
            hash ^= b as u32;
            hash = hash.wrapping_mul(16777619);
        }
        let hue = (hash % 360) as f32;
        let (r, g, b) = hsv_to_rgb(hue, 0.45, 0.65);
        gdk::RGBA::new(r, g, b, 0.95)
    }

    fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (f32, f32, f32) {
        let c = v * s;
        let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
        let m = v - c;
        let (r, g, b) = match (h / 60.0) as u32 {
            0 => (c, x, 0.0),
            1 => (x, c, 0.0),
            2 => (0.0, c, x),
            3 => (0.0, x, c),
            4 => (x, 0.0, c),
            _ => (c, 0.0, x),
        };
        (r + m, g + m, b + m)
    }
}

glib::wrapper! {
    pub struct WorkspacePreview(ObjectSubclass<imp::WorkspacePreview>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl WorkspacePreview {
    pub fn new(model: WorkspaceModel, viewport: (f64, f64)) -> Self {
        let obj: Self = glib::Object::builder().build();
        obj.imp().viewport.set(viewport);
        obj.imp().model.replace(Some(model));
        obj
    }

    pub fn ws_id(&self) -> i32 {
        self.imp().model.borrow().as_ref().map(|m| m.id).unwrap_or(0)
    }

    pub fn ws_name(&self) -> String {
        self.imp()
            .model
            .borrow()
            .as_ref()
            .map(|m| m.name.clone())
            .unwrap_or_default()
    }

    pub fn set_special_index(&self, index: usize) {
        self.imp().special_index.set(Some(index));
        self.queue_draw();
    }

    pub fn set_show_label(&self, show: bool) {
        if self.imp().show_label.get() != show {
            self.imp().show_label.set(show);
            self.queue_draw();
        }
    }

    pub fn set_ring_focused(&self, focused: bool) {
        if self.imp().ring_focused.get() != focused {
            self.imp().ring_focused.set(focused);
            self.queue_draw();
        }
    }

    /// Deliver a captured texture for a window on this workspace.
    /// Returns true when the window belongs to this preview.
    pub fn set_texture(&self, addr: u64, texture: gdk::Texture, y_invert: bool) -> bool {
        let mut guard = self.imp().model.borrow_mut();
        let Some(model) = guard.as_mut() else { return false };
        let mut hit = false;
        for win in &mut model.windows {
            if win.addr == addr {
                win.texture = Some(texture.clone());
                win.y_invert = y_invert;
                hit = true;
            }
        }
        drop(guard);
        if hit {
            self.queue_draw();
        }
        hit
    }

    pub fn window_addrs(&self) -> Vec<u64> {
        self.imp()
            .model
            .borrow()
            .as_ref()
            .map(|m| m.windows.iter().map(|w| w.addr).collect())
            .unwrap_or_default()
    }
}
