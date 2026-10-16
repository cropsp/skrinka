use windows::Win32::Foundation::POINT;
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits,
    ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, CAPTUREBLT, DIB_RGB_COLORS,
    SRCCOPY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
    SM_YVIRTUALSCREEN,
};

/// The whole virtual screen in physical pixels.
pub struct Capture {
    /// Virtual-screen origin; negative when a monitor sits left of / above the primary one.
    pub x0: i32,
    pub y0: i32,
    pub width: u32,
    pub height: u32,
    /// Row-major, `0x00RRGGBB` (the format softbuffer wants).
    pub pixels: Vec<u32>,
}

/// `0x00RRGGBB` → `[r, g, b, 255]`
pub fn to_rgba(pixel: u32) -> [u8; 4] {
    [(pixel >> 16) as u8, (pixel >> 8) as u8, pixel as u8, 255]
}

/// Mouse position in physical virtual-screen pixels.
pub fn cursor_position() -> Option<(i32, i32)> {
    let mut point = POINT::default();
    unsafe { GetCursorPos(&mut point) }.ok()?;
    Some((point.x, point.y))
}

/// Grabs every monitor with one `BitBlt` of the virtual screen.
///
/// The process is per-monitor DPI aware (winit sets that up), so all coordinates here are the
/// same physical pixels winit reports for monitors and windows.
pub fn capture_virtual_screen() -> anyhow::Result<Capture> {
    unsafe {
        let x0 = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let y0 = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let w = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        let h = GetSystemMetrics(SM_CYVIRTUALSCREEN);
        anyhow::ensure!(w > 0 && h > 0, "virtual screen size is {w}x{h}");

        let screen_dc = GetDC(None);
        anyhow::ensure!(!screen_dc.is_invalid(), "GetDC failed");
        let mem_dc = CreateCompatibleDC(Some(screen_dc));
        let bmp = CreateCompatibleBitmap(screen_dc, w, h);
        if mem_dc.is_invalid() || bmp.is_invalid() {
            if !bmp.is_invalid() {
                let _ = DeleteObject(bmp.into());
            }
            if !mem_dc.is_invalid() {
                let _ = DeleteDC(mem_dc);
            }
            ReleaseDC(None, screen_dc);
            anyhow::bail!("cannot allocate a {w}x{h} bitmap for the capture");
        }
        let old = SelectObject(mem_dc, bmp.into());
        // CAPTUREBLT includes layered (transparent) windows.
        let blt = BitBlt(mem_dc, 0, 0, w, h, Some(screen_dc), x0, y0, SRCCOPY | CAPTUREBLT);

        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h, // negative = top-down rows
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut pixels = vec![0u32; (w as usize) * (h as usize)];
        // GetDIBits wants the bitmap not selected into a DC.
        SelectObject(mem_dc, old);
        let lines = GetDIBits(
            mem_dc,
            bmp,
            0,
            h as u32,
            Some(pixels.as_mut_ptr() as *mut core::ffi::c_void),
            &mut info,
            DIB_RGB_COLORS,
        );

        let _ = DeleteObject(bmp.into());
        let _ = DeleteDC(mem_dc);
        ReleaseDC(None, screen_dc);

        blt?;
        anyhow::ensure!(lines == h, "GetDIBits returned {lines} of {h} lines");
        // A 32-bit DIB is B,G,R,X in memory = 0x00RRGGBB as a little-endian u32; clear the X byte.
        for p in &mut pixels {
            *p &= 0x00FF_FFFF;
        }
        Ok(Capture { x0, y0, width: w as u32, height: h as u32, pixels })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xrgb_to_rgba() {
        assert_eq!(to_rgba(0x00E3_4133), [0xE3, 0x41, 0x33, 255]);
        assert_eq!(to_rgba(0x0000_00FF), [0, 0, 255, 255]);
    }
}
