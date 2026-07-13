//! Capture engine: a private Wayland connection speaking
//! hyprland-toplevel-export-v1, fully decoupled from GTK's connection.
//! Captures are sequential: one frame in flight at a time.

use anyhow::{bail, Context, Result};
use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::{wl_buffer, wl_registry, wl_shm, wl_shm_pool};
use wayland_client::{delegate_noop, Connection, Dispatch, EventQueue, QueueHandle, WEnum};
use wayland_protocols_hyprland::toplevel_export::v1::client::hyprland_toplevel_export_frame_v1::{
    self, Flags, HyprlandToplevelExportFrameV1,
};
use wayland_protocols_hyprland::toplevel_export::v1::client::hyprland_toplevel_export_manager_v1::HyprlandToplevelExportManagerV1;

use super::buffer::ShmBuffer;

#[derive(Debug)]
pub struct FrameData {
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub format: wl_shm::Format,
    pub y_invert: bool,
    pub bytes: Vec<u8>,
}

#[derive(Default)]
pub struct State {
    buffer_info: Option<(wl_shm::Format, u32, u32, u32)>,
    y_invert: bool,
    buffer_done: bool,
    ready: bool,
    failed: bool,
}

impl Dispatch<HyprlandToplevelExportFrameV1, ()> for State {
    fn event(
        state: &mut Self,
        _: &HyprlandToplevelExportFrameV1,
        event: hyprland_toplevel_export_frame_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        use hyprland_toplevel_export_frame_v1::Event;
        match event {
            Event::Buffer { format: WEnum::Value(format), width, height, stride } => {
                state.buffer_info = Some((format, width, height, stride));
            }
            Event::Flags { flags: WEnum::Value(flags) } => {
                state.y_invert = flags.contains(Flags::YInvert);
            }
            Event::BufferDone => state.buffer_done = true,
            Event::Ready { .. } => state.ready = true,
            Event::Failed => state.failed = true,
            _ => {}
        }
    }
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

delegate_noop!(State: ignore wl_shm::WlShm);
delegate_noop!(State: ignore wl_shm_pool::WlShmPool);
delegate_noop!(State: ignore wl_buffer::WlBuffer);
delegate_noop!(State: ignore HyprlandToplevelExportManagerV1);

pub struct Engine {
    queue: EventQueue<State>,
    qh: QueueHandle<State>,
    manager: HyprlandToplevelExportManagerV1,
    shm: wl_shm::WlShm,
    state: State,
}

impl Engine {
    pub fn new() -> Result<Self> {
        let conn = Connection::connect_to_env().context("connect to Wayland display")?;
        let (globals, queue) =
            registry_queue_init::<State>(&conn).context("wayland registry init")?;
        let qh = queue.handle();
        let manager: HyprlandToplevelExportManagerV1 = globals
            .bind(&qh, 1..=2, ())
            .context("hyprland_toplevel_export_manager_v1 not offered (not running under Hyprland?)")?;
        let shm: wl_shm::WlShm = globals.bind(&qh, 1..=1, ()).context("wl_shm not offered")?;
        Ok(Self { queue, qh, manager, shm, state: State::default() })
    }

    /// Capture one toplevel identified by its Hyprland window address.
    pub fn capture(&mut self, addr: u64) -> Result<FrameData> {
        self.state = State::default();
        // Hyprland matches toplevels on the low 32 bits of the window address.
        let frame = self.manager.capture_toplevel(0, addr as u32, &self.qh, ());

        while !self.state.buffer_done {
            if self.state.failed {
                frame.destroy();
                bail!("capture failed for window {addr:#x}");
            }
            self.queue.blocking_dispatch(&mut self.state)?;
        }
        if self.state.failed {
            frame.destroy();
            bail!("capture failed for window {addr:#x}");
        }

        let (format, width, height, stride) = self
            .state
            .buffer_info
            .context("compositor offered no shm buffer format")?;

        let mut shm_buffer = ShmBuffer::new(&self.shm, &self.qh, width, height, stride, format)?;
        frame.copy(&shm_buffer.buffer, 1);

        while !self.state.ready {
            if self.state.failed {
                frame.destroy();
                shm_buffer.destroy();
                bail!("copy failed for window {addr:#x}");
            }
            self.queue.blocking_dispatch(&mut self.state)?;
        }

        let mut bytes = shm_buffer.read_bytes()?;
        frame.destroy();
        shm_buffer.destroy();

        // X-formats carry undefined alpha; force it opaque so the UI can treat
        // everything as premultiplied.
        if matches!(format, wl_shm::Format::Xrgb8888 | wl_shm::Format::Xbgr8888) {
            for px in bytes.chunks_exact_mut(4) {
                px[3] = 0xFF;
            }
        }

        Ok(FrameData {
            width,
            height,
            stride,
            format,
            y_invert: self.state.y_invert,
            bytes,
        })
    }
}
