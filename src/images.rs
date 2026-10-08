use std::path::Path;
use windows::Win32::Foundation::GENERIC_READ;
use windows::Win32::Graphics::Gdi::{
    CreateDIBSection, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, HBITMAP,
};
use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_WICPixelFormat32bppBGRA, IWICImagingFactory, WICBitmapDitherTypeNone,
    WICBitmapPaletteTypeCustom, WICDecodeMetadataCacheOnDemand,
};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};

pub struct Frame {
    pub bitmap: HBITMAP,
    pub width: i32,
    pub height: i32,
}

impl Drop for Frame {
    fn drop(&mut self) {
        unsafe {
            let _ = windows::Win32::Graphics::Gdi::DeleteObject(self.bitmap.into());
        }
    }
}

pub fn load(path: &Path) -> Result<Frame, String> {
    let wide: Vec<u16> = std::os::windows::ffi::OsStrExt::encode_wide(path.as_os_str())
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        let factory: IWICImagingFactory =
            CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)
                .map_err(|error| error.to_string())?;
        let decoder = factory
            .CreateDecoderFromFilename(
                windows_core::PCWSTR(wide.as_ptr()),
                None,
                GENERIC_READ,
                WICDecodeMetadataCacheOnDemand,
            )
            .map_err(|error| error.to_string())?;
        let frame = decoder.GetFrame(0).map_err(|error| error.to_string())?;
        let converter = factory
            .CreateFormatConverter()
            .map_err(|error| error.to_string())?;
        converter
            .Initialize(
                &frame,
                &GUID_WICPixelFormat32bppBGRA,
                WICBitmapDitherTypeNone,
                None,
                0.0,
                WICBitmapPaletteTypeCustom,
            )
            .map_err(|error| error.to_string())?;
        let mut width = 0u32;
        let mut height = 0u32;
        converter
            .GetSize(&mut width, &mut height)
            .map_err(|error| error.to_string())?;
        if width == 0 || height == 0 || width > 16000 || height > 16000 {
            return Err("This image has a size Windows cannot show".into());
        }
        let stride = width.saturating_mul(4);
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32,
                biHeight: -(height as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: 0,
                biSizeImage: stride.saturating_mul(height),
                biXPelsPerMeter: 0,
                biYPelsPerMeter: 0,
                biClrUsed: 0,
                biClrImportant: 0,
            },
            bmiColors: [windows::Win32::Graphics::Gdi::RGBQUAD::default()],
        };
        let mut bits = std::ptr::null_mut();
        let bitmap = CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0)
            .map_err(|error| error.to_string())?;
        if bits.is_null() {
            let _ = windows::Win32::Graphics::Gdi::DeleteObject(bitmap.into());
            return Err("Windows could not allocate the image".into());
        }
        let size = stride.saturating_mul(height) as usize;
        let buffer = std::slice::from_raw_parts_mut(bits as *mut u8, size);
        if let Err(error) = converter.CopyPixels(std::ptr::null(), stride, buffer) {
            let _ = windows::Win32::Graphics::Gdi::DeleteObject(bitmap.into());
            return Err(error.to_string());
        }
        Ok(Frame {
            bitmap,
            width: width as i32,
            height: height as i32,
        })
    }
}
