//! wl_shm buffer backed by a memfd, plus readback into a plain byte vector.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::os::fd::AsFd;

use anyhow::{Context, Result};
use wayland_client::protocol::wl_buffer::WlBuffer;
use wayland_client::protocol::wl_shm::{self, WlShm};
use wayland_client::protocol::wl_shm_pool::WlShmPool;
use wayland_client::QueueHandle;

use super::engine::State;

pub struct ShmBuffer {
    file: File,
    pool: WlShmPool,
    pub buffer: WlBuffer,
    size: usize,
}

impl ShmBuffer {
    pub fn new(
        shm: &WlShm,
        qh: &QueueHandle<State>,
        width: u32,
        height: u32,
        stride: u32,
        format: wl_shm::Format,
    ) -> Result<Self> {
        let size = stride as usize * height as usize;
        let mfd = memfd::MemfdOptions::default()
            .close_on_exec(true)
            .create("hyprpanopticon-shm")
            .context("memfd_create")?;
        mfd.as_file().set_len(size as u64).context("memfd set_len")?;
        let file = mfd.into_file();
        let pool = shm.create_pool(file.as_fd(), size as i32, qh, ());
        let buffer = pool.create_buffer(
            0,
            width as i32,
            height as i32,
            stride as i32,
            format,
            qh,
            (),
        );
        Ok(Self { file, pool, buffer, size })
    }

    pub fn read_bytes(&mut self) -> Result<Vec<u8>> {
        let mut bytes = vec![0u8; self.size];
        self.file.seek(SeekFrom::Start(0))?;
        self.file.read_exact(&mut bytes).context("memfd readback")?;
        Ok(bytes)
    }

    pub fn destroy(self) {
        self.buffer.destroy();
        self.pool.destroy();
    }
}
