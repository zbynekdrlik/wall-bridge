//! Spout receiver: reads Arena's shared texture (DX11 legacy shared handle) and detects
//! new pictures by sampling 16 probe boxes, as the MVP did.
#![cfg(windows)]

use std::ffi::CString;
use wb_core::sender::SenderSeen;
use wb_core::spout_wire::{self, TextureInfo, INFO_LEN, SAMPLES};
use windows::core::{Error, Result, PCSTR, PCWSTR};
use windows::Win32::Foundation::{CloseHandle, E_FAIL, HANDLE, WAIT_ABANDONED, WAIT_OBJECT_0};
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::Win32::Graphics::Dxgi::DXGI_ERROR_WAS_STILL_DRAWING;
use windows::Win32::System::Memory::*;
use windows::Win32::System::Threading::*;

/// Reads a whole named file mapping of `len` bytes (0 = the whole mapping).
fn read_mapping(name: &str, len: usize) -> Option<Vec<u8>> {
    let c = CString::new(name).ok()?;
    unsafe {
        let h = OpenFileMappingA(FILE_MAP_READ.0, false, PCSTR(c.as_ptr() as _)).ok()?;
        let view = MapViewOfFile(h, FILE_MAP_READ, 0, 0, len);
        let out = if view.Value.is_null() {
            None
        } else {
            let size = if len > 0 {
                len
            } else {
                let mut mbi = MEMORY_BASIC_INFORMATION::default();
                VirtualQuery(
                    Some(view.Value),
                    &mut mbi,
                    std::mem::size_of::<MEMORY_BASIC_INFORMATION>(),
                );
                mbi.RegionSize
            };
            let bytes = std::slice::from_raw_parts(view.Value as *const u8, size).to_vec();
            let _ = UnmapViewOfFile(view);
            Some(bytes)
        };
        let _ = CloseHandle(h);
        out
    }
}

/// Names in Spout's `SpoutSenderNames` list.
pub fn list_senders() -> Vec<String> {
    read_mapping("SpoutSenderNames", 0)
        .map(|b| spout_wire::parse_names(&b))
        .unwrap_or_default()
}

/// The sender's texture info, if it has published one.
pub fn read_info(name: &str) -> Option<TextureInfo> {
    read_mapping(name, INFO_LEN).and_then(|b| spout_wire::parse_info(&b))
}

/// Result of one probe of the sender.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Probe {
    /// Arena held the mutex longer than the probe may wait.
    LockTimeout,
    /// The GPU has not finished sampling the probe boxes yet; ask again shortly.
    Busy,
    /// Same picture as last time.
    Same,
    /// A new picture was copied into the destination.
    New,
}

struct Open {
    handle: u32,
    tex: ID3D11Texture2D,
    width: u32,
    height: u32,
    mutex: Option<HANDLE>,
}

pub struct Reader {
    name: String,
    device: ID3D11Device,
    stage: ID3D11Texture2D,
    open: Option<Open>,
    prev_sig: Vec<u8>,
    /// Probe boxes are queued on the GPU and not read back yet.
    pending: bool,
}

impl Reader {
    pub fn new(device: &ID3D11Device, name: &str) -> Result<Reader> {
        let desc = D3D11_TEXTURE2D_DESC {
            Width: 8 * SAMPLES,
            Height: 8,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_STAGING,
            BindFlags: 0,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
            MiscFlags: 0,
        };
        let mut stage = None;
        unsafe { device.CreateTexture2D(&desc, None, Some(&mut stage))? };
        Ok(Reader {
            name: name.to_string(),
            device: device.clone(),
            stage: stage.ok_or_else(|| Error::from(E_FAIL))?,
            open: None,
            prev_sig: vec![],
            pending: false,
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// What the sender looks like right now (for `SenderWatch::tick`).
    pub fn seen(&self) -> Option<SenderSeen> {
        let i = read_info(&self.name)?;
        Some(SenderSeen {
            handle: i.handle,
            width: i.width,
            height: i.height,
            listed: list_senders().iter().any(|n| n == &self.name),
        })
    }

    /// Opens the shared texture behind `handle` (replacing any open one).
    pub fn open(&mut self, handle: u32) -> Result<()> {
        self.close();
        let mut tex: Option<ID3D11Texture2D> = None;
        unsafe {
            self.device
                .OpenSharedResource(HANDLE(handle as usize as *mut _), &mut tex)?;
        }
        let tex = tex.ok_or_else(|| Error::from(E_FAIL))?;
        let mut d = D3D11_TEXTURE2D_DESC::default();
        unsafe { tex.GetDesc(&mut d) };
        let mutex_name: Vec<u16> = format!("{}_SpoutAccessMutex", self.name)
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let mutex = unsafe {
            OpenMutexW(
                SYNCHRONIZATION_SYNCHRONIZE,
                false,
                PCWSTR(mutex_name.as_ptr()),
            )
        }
        .ok();
        self.open = Some(Open {
            handle,
            tex,
            width: d.Width,
            height: d.Height,
            mutex,
        });
        self.prev_sig.clear();
        self.pending = false;
        Ok(())
    }

    pub fn close(&mut self) {
        if let Some(o) = self.open.take() {
            if let Some(m) = o.mutex {
                unsafe {
                    let _ = CloseHandle(m);
                }
            }
        }
    }

    /// Handle of the open texture.
    pub fn open_handle(&self) -> Option<u32> {
        self.open.as_ref().map(|o| o.handle)
    }

    /// Size of the open texture.
    pub fn size(&self) -> Option<(u32, u32)> {
        self.open.as_ref().map(|o| (o.width, o.height))
    }

    /// Probes the sender once and never blocks on the GPU: it queues the probe-box copy,
    /// and reads it back only when the GPU is done (`Busy` until then). Waits for Arena's
    /// mutex at most `max_wait_ms`. On a new picture, `dst()` names the texture to copy it
    /// into (while Arena's mutex is held); a picture larger than `dst` is cropped top-left.
    pub fn poll(
        &mut self,
        ctx: &ID3D11DeviceContext,
        max_wait_ms: u32,
        dst: impl FnOnce() -> ID3D11Texture2D,
    ) -> Result<Probe> {
        let Some(o) = &self.open else {
            return Ok(Probe::Same);
        };
        let lock = |m: Option<HANDLE>| -> bool {
            match m {
                Some(m) => {
                    let r = unsafe { WaitForSingleObject(m, max_wait_ms) };
                    r == WAIT_OBJECT_0 || r == WAIT_ABANDONED
                }
                None => true,
            }
        };
        let unlock = |m: Option<HANDLE>| {
            if let Some(m) = m {
                unsafe {
                    let _ = ReleaseMutex(m);
                }
            }
        };

        if !self.pending {
            if !lock(o.mutex) {
                return Ok(Probe::LockTimeout);
            }
            for (k, (x, y)) in spout_wire::sample_boxes(o.width, o.height)
                .into_iter()
                .enumerate()
            {
                let b = D3D11_BOX {
                    left: x,
                    top: y,
                    front: 0,
                    right: (x + 8).min(o.width),
                    bottom: (y + 8).min(o.height),
                    back: 1,
                };
                unsafe {
                    ctx.CopySubresourceRegion(
                        &self.stage,
                        0,
                        k as u32 * 8,
                        0,
                        0,
                        &o.tex,
                        0,
                        Some(&b),
                    )
                };
            }
            unsafe { ctx.Flush() };
            unlock(o.mutex);
            self.pending = true;
        }

        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        match unsafe {
            ctx.Map(
                &self.stage,
                0,
                D3D11_MAP_READ,
                D3D11_MAP_FLAG_DO_NOT_WAIT.0 as u32,
                Some(&mut mapped),
            )
        } {
            Ok(()) => {}
            Err(e) if e.code() == DXGI_ERROR_WAS_STILL_DRAWING => return Ok(Probe::Busy),
            Err(e) => {
                self.pending = false;
                return Err(e);
            }
        }
        let row = (8 * SAMPLES * 4) as usize;
        let mut sig = Vec::with_capacity(row * 8);
        for r in 0..8 {
            unsafe {
                let p = (mapped.pData as *const u8).add(r * mapped.RowPitch as usize);
                sig.extend_from_slice(std::slice::from_raw_parts(p, row));
            }
        }
        unsafe { ctx.Unmap(&self.stage, 0) };
        self.pending = false;
        if sig == self.prev_sig {
            return Ok(Probe::Same);
        }

        if !lock(o.mutex) {
            // prev_sig stays: the next probe sees the change again.
            return Ok(Probe::LockTimeout);
        }
        let dst = dst();
        unsafe {
            let mut dd = D3D11_TEXTURE2D_DESC::default();
            dst.GetDesc(&mut dd);
            if dd.Width == o.width && dd.Height == o.height {
                ctx.CopyResource(&dst, &o.tex);
            } else {
                let b = D3D11_BOX {
                    left: 0,
                    top: 0,
                    front: 0,
                    right: o.width.min(dd.Width),
                    bottom: o.height.min(dd.Height),
                    back: 1,
                };
                ctx.CopySubresourceRegion(&dst, 0, 0, 0, 0, &o.tex, 0, Some(&b));
            }
            ctx.Flush();
        }
        unlock(o.mutex);
        self.prev_sig = sig;
        Ok(Probe::New)
    }
}

impl Drop for Reader {
    fn drop(&mut self) {
        self.close();
    }
}
