//! Data model shared between IPC, capture, and UI. All of it lives on the GTK
//! main thread; worker threads only ever see plain data (`u64` addresses,
//! `glib::Bytes`), never these structs or any GObject.

use gtk4 as gtk;
use gtk::gdk;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    /// Intersect with `other`; returns None when the intersection is empty.
    pub fn clip(&self, other: &Rect) -> Option<Rect> {
        let x1 = self.x.max(other.x);
        let y1 = self.y.max(other.y);
        let x2 = (self.x + self.w).min(other.x + other.w);
        let y2 = (self.y + self.h).min(other.y + other.h);
        if x2 > x1 && y2 > y1 {
            Some(Rect { x: x1, y: y1, w: x2 - x1, h: y2 - y1 })
        } else {
            None
        }
    }
}

#[derive(Debug, Clone)]
pub struct WindowThumb {
    pub addr: u64,
    /// Window rect in monitor-relative logical coordinates (client.at minus
    /// monitor position). May extend past the monitor viewport with scrolling
    /// layouts; the preview clips it.
    pub rect: Rect,
    pub class: String,
    pub title: String,
    /// Captured pixels; None until the capture worker delivers them.
    pub texture: Option<gdk::Texture>,
    pub y_invert: bool,
    /// Stacking hint: focus history (lower = more recently focused, drawn last).
    pub focus_order: usize,
}

#[derive(Debug, Clone)]
pub struct WorkspaceModel {
    pub id: i32,
    pub name: String,
    pub windows: Vec<WindowThumb>,
}

#[derive(Debug, Clone, Copy)]
pub struct MonitorModel {
    /// Position in the global layout (logical).
    pub x: f64,
    pub y: f64,
    /// Logical size (transformed size divided by scale).
    pub w: f64,
    pub h: f64,
    pub scale: f64,
}

#[derive(Debug, Clone)]
pub struct Snapshot {
    pub monitor: MonitorModel,
    pub monitor_name: String,
    /// Normal workspaces on the focused monitor, sorted by id.
    pub workspaces: Vec<WorkspaceModel>,
    /// Currently active workspace id (initial ring focus).
    pub active_workspace: i32,
}
