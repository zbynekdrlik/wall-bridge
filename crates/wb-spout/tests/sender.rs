//! Runs on Windows (CI windows-latest, WARP device): an in-process Spout sender built from the
//! same shared-memory layout Arena publishes, read back through `Reader`.
#![cfg(windows)]

use std::ffi::CString;
use wb_core::spout_wire::{INFO_LEN, NAME_SLOT};
use wb_spout::{list_senders, Probe, Reader};
use windows::core::{Interface, PCSTR, PCWSTR};
use windows::Win32::Foundation::{CloseHandle, HANDLE, HMODULE, INVALID_HANDLE_VALUE};
use windows::Win32::Graphics::Direct3D::*;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::Win32::Graphics::Dxgi::IDXGIResource;
use windows::Win32::System::Memory::*;
use windows::Win32::System::Threading::CreateMutexW;

fn device() -> (ID3D11Device, ID3D11DeviceContext) {
    let mut d = None;
    let mut c = None;
    unsafe {
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_WARP,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut d),
            None,
            Some(&mut c),
        )
        .unwrap();
    }
    (d.unwrap(), c.unwrap())
}

fn texture(dev: &ID3D11Device, w: u32, h: u32, shared: bool) -> ID3D11Texture2D {
    let desc = D3D11_TEXTURE2D_DESC {
        Width: w,
        Height: h,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_DEFAULT,
        BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
        CPUAccessFlags: 0,
        MiscFlags: if shared {
            D3D11_RESOURCE_MISC_SHARED.0 as u32
        } else {
            0
        },
    };
    let mut t = None;
    unsafe { dev.CreateTexture2D(&desc, None, Some(&mut t)).unwrap() };
    t.unwrap()
}

fn fill(ctx: &ID3D11DeviceContext, tex: &ID3D11Texture2D, w: u32, h: u32, v: u8) {
    let data = vec![v; (w * h * 4) as usize];
    unsafe { ctx.UpdateSubresource(tex, 0, None, data.as_ptr() as _, w * 4, 0) };
    unsafe { ctx.Flush() };
}

/// A named mapping kept alive for the test; filled with `bytes`.
struct Mapping(HANDLE);

fn mapping(name: &str, bytes: &[u8]) -> Mapping {
    let c = CString::new(name).unwrap();
    unsafe {
        let h = CreateFileMappingA(
            INVALID_HANDLE_VALUE,
            None,
            PAGE_READWRITE,
            0,
            bytes.len() as u32,
            PCSTR(c.as_ptr() as _),
        )
        .unwrap();
        let v = MapViewOfFile(h, FILE_MAP_ALL_ACCESS, 0, 0, bytes.len());
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), v.Value as *mut u8, bytes.len());
        let _ = UnmapViewOfFile(v);
        Mapping(h)
    }
}

impl Drop for Mapping {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

struct TestSender {
    _info: Mapping,
    _names: Mapping,
    _mutex: HANDLE,
    tex: ID3D11Texture2D,
    handle: u32,
}

fn sender(dev: &ID3D11Device, name: &str, w: u32, h: u32) -> TestSender {
    let tex = texture(dev, w, h, true);
    let res: IDXGIResource = tex.cast().unwrap();
    let handle = unsafe { res.GetSharedHandle().unwrap() }.0 as usize as u32;
    let mut info = vec![0u8; INFO_LEN];
    info[0..4].copy_from_slice(&handle.to_le_bytes());
    info[4..8].copy_from_slice(&w.to_le_bytes());
    info[8..12].copy_from_slice(&h.to_le_bytes());
    info[12..16].copy_from_slice(&87u32.to_le_bytes());
    let mut names = vec![0u8; NAME_SLOT * 4];
    names[..name.len()].copy_from_slice(name.as_bytes());
    let mutex_name: Vec<u16> = format!("{name}_SpoutAccessMutex")
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let mutex = unsafe { CreateMutexW(None, false, PCWSTR(mutex_name.as_ptr())).unwrap() };
    TestSender {
        _info: mapping(name, &info),
        _names: mapping("SpoutSenderNames", &names),
        _mutex: mutex,
        tex,
        handle,
    }
}

#[test]
fn reads_new_and_same_pictures() {
    let (dev, ctx) = device();
    let s = sender(&dev, "wb-test-a", 640, 360);
    assert!(list_senders().contains(&"wb-test-a".to_string()));
    let mut r = Reader::new(&dev, "wb-test-a").unwrap();
    let seen = r.seen().unwrap();
    assert_eq!(seen.handle, s.handle);
    assert!(seen.listed);
    r.open(seen.handle).unwrap();
    let dst = texture(&dev, 640, 360, false);
    fill(&ctx, &s.tex, 640, 360, 10);
    assert_eq!(r.poll(&ctx, || dst.clone()).unwrap(), Probe::New);
    assert_eq!(r.poll(&ctx, || dst.clone()).unwrap(), Probe::Same);
    fill(&ctx, &s.tex, 640, 360, 200);
    assert_eq!(r.poll(&ctx, || dst.clone()).unwrap(), Probe::New);
}

#[test]
fn size_mismatch_reported() {
    let (dev, ctx) = device();
    let s = sender(&dev, "wb-test-b", 3840, 540);
    let mut r = Reader::new(&dev, "wb-test-b").unwrap();
    let seen = r.seen().unwrap();
    assert_eq!((seen.width, seen.height), (3840, 540));
    r.open(s.handle).unwrap();
    assert_eq!(r.size(), Some((3840, 540)));
    let dst = texture(&dev, 7680, 1080, false);
    fill(&ctx, &s.tex, 3840, 540, 77);
    assert_eq!(r.poll(&ctx, || dst.clone()).unwrap(), Probe::New);
}

#[test]
fn missing_sender_is_not_seen() {
    let (dev, _ctx) = device();
    let r = Reader::new(&dev, "wb-test-nobody").unwrap();
    assert!(r.seen().is_none());
}
