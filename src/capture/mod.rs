//! Capture worker thread and the bridge into the GTK main loop.
//!
//! Worker side only handles plain `Send` data (addresses in, byte buffers
//! out); textures are built on the GTK side with `frame_to_texture`.

pub mod buffer;
pub mod engine;

use gtk4 as gtk;
use gtk::gdk;
use gtk::glib;
use wayland_client::protocol::wl_shm;

pub use engine::FrameData;

pub struct CaptureResult {
    pub addr: u64,
    /// None when the capture failed (window gone, minimized, …).
    pub frame: Option<FrameData>,
}

#[derive(Clone)]
pub struct CaptureHandle {
    tx: std::sync::mpsc::Sender<u64>,
}

impl CaptureHandle {
    pub fn request(&self, addr: u64) {
        let _ = self.tx.send(addr);
    }
}

/// Spawn the capture worker. Returns a handle for requesting captures and the
/// receiver delivering results; consume it on the main loop with
/// `glib::spawn_future_local`. The worker exits when the handle is dropped.
pub fn spawn() -> anyhow::Result<(CaptureHandle, async_channel::Receiver<CaptureResult>)> {
    let (req_tx, req_rx) = std::sync::mpsc::channel::<u64>();
    let (res_tx, res_rx) = async_channel::unbounded::<CaptureResult>();

    let mut engine = engine::Engine::new()?;
    std::thread::Builder::new()
        .name("capture-worker".into())
        .spawn(move || {
            while let Ok(addr) = req_rx.recv() {
                let frame = match engine.capture(addr) {
                    Ok(frame) => Some(frame),
                    Err(e) => {
                        eprintln!("hyprPanopticon: {e:#}");
                        None
                    }
                };
                if res_tx.send_blocking(CaptureResult { addr, frame }).is_err() {
                    break;
                }
            }
        })?;

    Ok((CaptureHandle { tx: req_tx }, res_rx))
}

/// Build a GPU-uploadable texture from captured pixels. Must run on the GTK
/// main thread. Returns None for pixel formats we don't support.
pub fn frame_to_texture(frame: &FrameData) -> Option<gdk::Texture> {
    use gdk::MemoryFormat;
    let format = match frame.format {
        wl_shm::Format::Argb8888 | wl_shm::Format::Xrgb8888 => {
            MemoryFormat::B8g8r8a8Premultiplied
        }
        wl_shm::Format::Abgr8888 | wl_shm::Format::Xbgr8888 => {
            MemoryFormat::R8g8b8a8Premultiplied
        }
        _ => return None,
    };
    let bytes = glib::Bytes::from(&frame.bytes);
    Some(
        gdk::MemoryTexture::new(
            frame.width as i32,
            frame.height as i32,
            format,
            &bytes,
            frame.stride as usize,
        )
        .into(),
    )
}
