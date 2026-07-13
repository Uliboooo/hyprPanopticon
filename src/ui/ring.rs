//! RingView: lays WorkspacePreview children out on a circle. The focused
//! preview is largest and sits at the top; focus changes animate the ring.

use gtk4 as gtk;
use gtk::gdk;
use gtk::glib;
use gtk::graphene;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

use crate::layout::{self, RingParams};
use crate::model::Snapshot;
use crate::ui::preview::WorkspacePreview;

const ANIM_DURATION_US: i64 = 200_000;

pub struct Anim {
    pub start_pos: f64,
    pub target_pos: f64,
    pub start_time: Option<i64>,
}

mod imp {
    use super::*;
    use std::cell::{Cell, RefCell};

    #[derive(Default)]
    pub struct RingView {
        pub previews: RefCell<Vec<WorkspacePreview>>,
        pub focus_idx: Cell<usize>,
        /// Continuous focus position driven by the animation.
        pub focus_pos: Cell<f64>,
        pub anim: RefCell<Option<Anim>>,
        pub tick_id: RefCell<Option<gtk::TickCallbackId>>,
        pub aspect: Cell<f64>,
        pub params: RefCell<RingParams>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for RingView {
        const NAME: &'static str = "PanopticonRingView";
        type Type = super::RingView;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for RingView {
        fn dispose(&self) {
            if let Some(id) = self.tick_id.borrow_mut().take() {
                id.remove();
            }
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for RingView {
        fn request_mode(&self) -> gtk::SizeRequestMode {
            gtk::SizeRequestMode::ConstantSize
        }

        fn measure(&self, _orientation: gtk::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            (0, 0, -1, -1)
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            // Translucent scrim over the whole output, behind the previews.
            let widget = self.obj();
            let bounds = graphene::Rect::new(
                0.0,
                0.0,
                widget.width() as f32,
                widget.height() as f32,
            );
            snapshot.append_color(&gdk::RGBA::new(0.03, 0.03, 0.05, 0.55), &bounds);
            self.parent_snapshot(snapshot);
        }

        fn size_allocate(&self, width: i32, height: i32, _baseline: i32) {
            let previews = self.previews.borrow();
            let n = previews.len();
            if n == 0 {
                return;
            }
            let placements = layout::compute(
                n,
                self.focus_pos.get(),
                width as f64,
                height as f64,
                self.aspect.get().max(0.1),
                &self.params.borrow(),
            );
            for (preview, pl) in previews.iter().zip(placements.iter()) {
                let alloc = gtk::Allocation::new(
                    pl.x.round() as i32,
                    pl.y.round() as i32,
                    (pl.width.round() as i32).max(1),
                    (pl.height.round() as i32).max(1),
                );
                preview.size_allocate(&alloc, -1);
            }
        }
    }
}

glib::wrapper! {
    pub struct RingView(ObjectSubclass<imp::RingView>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for RingView {
    fn default() -> Self {
        glib::Object::builder().build()
    }
}

impl RingView {
    pub fn set_snapshot(&self, snapshot: &Snapshot) {
        let imp = self.imp();
        // Keep the user's ring focus across live refreshes when possible.
        let prev_focus_ws = imp
            .previews
            .borrow()
            .get(imp.focus_idx.get())
            .map(|p| p.ws_id());
        for old in imp.previews.borrow_mut().drain(..) {
            old.unparent();
        }
        let viewport = (snapshot.monitor.w, snapshot.monitor.h);
        imp.aspect.set(snapshot.monitor.w / snapshot.monitor.h.max(1.0));

        let mut previews = Vec::with_capacity(snapshot.workspaces.len());
        for ws in &snapshot.workspaces {
            let preview = WorkspacePreview::new(ws.clone(), viewport);
            preview.set_parent(self);
            previews.push(preview);
        }
        let focus = prev_focus_ws
            .and_then(|id| snapshot.workspaces.iter().position(|w| w.id == id))
            .or_else(|| {
                snapshot
                    .workspaces
                    .iter()
                    .position(|w| w.id == snapshot.active_workspace)
            })
            .unwrap_or(0);
        *imp.previews.borrow_mut() = previews;
        *imp.anim.borrow_mut() = None;
        imp.focus_idx.set(focus);
        imp.focus_pos.set(focus as f64);
        self.apply_focus_decorations();
        self.restack();
        self.queue_allocate();
    }

    pub fn previews(&self) -> Vec<WorkspacePreview> {
        self.imp().previews.borrow().clone()
    }

    pub fn focused_ws_id(&self) -> Option<i32> {
        let imp = self.imp();
        imp.previews.borrow().get(imp.focus_idx.get()).map(|p| p.ws_id())
    }

    pub fn rotate(&self, delta: i32) {
        let n = self.imp().previews.borrow().len();
        if n == 0 {
            return;
        }
        let idx = (self.imp().focus_idx.get() as i64 + delta as i64).rem_euclid(n as i64);
        self.set_focus(idx as usize);
    }

    pub fn set_focus(&self, idx: usize) {
        let imp = self.imp();
        let n = imp.previews.borrow().len();
        if n == 0 || idx >= n || idx == imp.focus_idx.get() {
            return;
        }
        imp.focus_idx.set(idx);
        self.apply_focus_decorations();
        self.restack();

        let current = imp.focus_pos.get();
        let target = current + layout::shortest_step(current, idx, n);
        *imp.anim.borrow_mut() = Some(Anim {
            start_pos: current,
            target_pos: target,
            start_time: None,
        });
        self.ensure_tick_callback();
    }

    fn ensure_tick_callback(&self) {
        let imp = self.imp();
        if imp.tick_id.borrow().is_some() {
            return;
        }
        let id = self.add_tick_callback(|widget, clock| {
            let imp = widget.imp();
            let now = clock.frame_time();
            let done;
            {
                let mut anim = imp.anim.borrow_mut();
                if let Some(anim) = anim.as_mut() {
                    let start = *anim.start_time.get_or_insert(now);
                    let t = ((now - start) as f64 / ANIM_DURATION_US as f64).clamp(0.0, 1.0);
                    let ease = 1.0 - (1.0 - t).powi(3);
                    let pos = anim.start_pos + (anim.target_pos - anim.start_pos) * ease;
                    imp.focus_pos.set(pos);
                    done = t >= 1.0;
                } else {
                    done = true;
                }
            }
            widget.queue_allocate();
            if done {
                // Keep the position wrapped so it never grows unbounded.
                let n = imp.previews.borrow().len().max(1) as f64;
                imp.focus_pos.set(imp.focus_pos.get().rem_euclid(n));
                *imp.anim.borrow_mut() = None;
                imp.tick_id.borrow_mut().take();
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
        *imp.tick_id.borrow_mut() = Some(id);
    }

    fn apply_focus_decorations(&self) {
        let imp = self.imp();
        let focus = imp.focus_idx.get();
        for (i, preview) in imp.previews.borrow().iter().enumerate() {
            preview.set_ring_focused(i == focus);
        }
    }

    /// Reorder GTK sibling order so previews paint (and pick) back-to-front
    /// with the focused preview on top, based on scales at the focus target.
    fn restack(&self) {
        let imp = self.imp();
        let previews = imp.previews.borrow();
        let n = previews.len();
        if n == 0 {
            return;
        }
        let placements = layout::compute(
            n,
            imp.focus_idx.get() as f64,
            1000.0,
            1000.0,
            1.0,
            &imp.params.borrow(),
        );
        let mut order: Vec<usize> = (0..n).collect();
        order.sort_by(|&a, &b| {
            placements[a]
                .scale
                .partial_cmp(&placements[b].scale)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let mut prev: Option<WorkspacePreview> = None;
        for &i in &order {
            let child = &previews[i];
            child.insert_after(self, prev.as_ref());
            prev = Some(child.clone());
        }
    }
}
