//! The app's icons: picking an image out of an `.ico` file (pure) and turning
//! it into a Win32 icon or raw RGBA pixels (Windows decodes the PNG images, so
//! no image decoder is linked).

/// The application icon (Explorer, taskbar, Settings window).
pub const APP_ICO: &[u8] = include_bytes!("../../../assets/icons/app.ico");

/// The image inside an `.ico` file best suited to `want` pixels: the smallest
/// one at least that big, else the biggest. Returns the image bytes (PNG or
/// DIB, as `CreateIconFromResourceEx` takes them) and its size.
pub fn ico_image(ico: &[u8], want: u32) -> Option<(&[u8], u32)> {
    let u16_at = |i: usize| Some(u16::from_le_bytes(ico.get(i..i + 2)?.try_into().ok()?));
    let u32_at = |i: usize| Some(u32::from_le_bytes(ico.get(i..i + 4)?.try_into().ok()?));
    if u16_at(0)? != 0 || u16_at(2)? != 1 {
        return None;
    }
    let mut best: Option<(u32, usize, usize)> = None;
    for n in 0..u16_at(4)? as usize {
        let e = 6 + n * 16;
        let size = match *ico.get(e)? {
            0 => 256,
            w => w as u32,
        };
        let (len, off) = (u32_at(e + 8)? as usize, u32_at(e + 12)? as usize);
        if off.checked_add(len)? > ico.len() {
            continue;
        }
        let better = match best {
            None => true,
            Some((b, ..)) if b < want => size > b,
            Some((b, ..)) => size >= want && size < b,
        };
        if better {
            best = Some((size, off, len));
        }
    }
    best.map(|(size, off, len)| (&ico[off..off + len], size))
}

#[cfg(windows)]
mod win {
    use super::ico_image;

    #[repr(C)]
    struct IconInfo {
        is_icon: i32,
        x_hotspot: u32,
        y_hotspot: u32,
        mask: isize,
        color: isize,
    }

    #[repr(C)]
    struct BitmapInfoHeader {
        size: u32,
        width: i32,
        height: i32,
        planes: u16,
        bit_count: u16,
        compression: u32,
        size_image: u32,
        x_ppm: i32,
        y_ppm: i32,
        clr_used: u32,
        clr_important: u32,
    }

    #[link(name = "user32")]
    extern "system" {
        fn CreateIconFromResourceEx(
            bits: *const u8,
            len: u32,
            icon: i32,
            ver: u32,
            cx: i32,
            cy: i32,
            flags: u32,
        ) -> isize;
        fn DestroyIcon(i: isize) -> i32;
        fn GetIconInfo(i: isize, info: *mut IconInfo) -> i32;
        fn GetDC(h: isize) -> isize;
        fn ReleaseDC(h: isize, dc: isize) -> i32;
    }
    #[link(name = "gdi32")]
    extern "system" {
        fn GetDIBits(
            dc: isize,
            bmp: isize,
            start: u32,
            lines: u32,
            bits: *mut u8,
            info: *mut BitmapInfoHeader,
            usage: u32,
        ) -> i32;
        fn DeleteObject(o: isize) -> i32;
    }

    /// A Win32 icon (`HICON`, 0 on failure) of `px` pixels from an `.ico` file.
    /// The caller owns it (`DestroyIcon`).
    pub fn hicon_from_ico(ico: &[u8], px: u32) -> isize {
        let Some((bits, size)) = ico_image(ico, px) else {
            return 0;
        };
        let px = size.min(px) as i32;
        unsafe {
            CreateIconFromResourceEx(bits.as_ptr(), bits.len() as u32, 1, 0x0003_0000, px, px, 0)
        }
    }

    /// Straight RGBA pixels, `px` by `px`, of the image in `ico` closest to `px`.
    pub fn rgba_from_ico(ico: &[u8], px: u32) -> Option<Vec<u8>> {
        let icon = hicon_from_ico(ico, px);
        if icon == 0 {
            return None;
        }
        unsafe {
            let mut info: IconInfo = std::mem::zeroed();
            let ok = GetIconInfo(icon, &mut info) != 0;
            DestroyIcon(icon);
            if !ok {
                return None;
            }
            let mut hdr = BitmapInfoHeader {
                size: std::mem::size_of::<BitmapInfoHeader>() as u32,
                width: px as i32,
                height: -(px as i32), // top-down
                planes: 1,
                bit_count: 32,
                compression: 0,
                size_image: 0,
                x_ppm: 0,
                y_ppm: 0,
                clr_used: 0,
                clr_important: 0,
            };
            let mut out = vec![0u8; (px * px * 4) as usize];
            let dc = GetDC(0);
            let lines = GetDIBits(dc, info.color, 0, px, out.as_mut_ptr(), &mut hdr, 0);
            ReleaseDC(0, dc);
            DeleteObject(info.color);
            DeleteObject(info.mask);
            if lines != px as i32 {
                return None;
            }
            for p in out.as_chunks_mut::<4>().0 {
                p.swap(0, 2); // BGRA -> RGBA
            }
            Some(out)
        }
    }
}
#[cfg(windows)]
pub use win::{hicon_from_ico, rgba_from_ico};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_app_icon_has_every_size_from_16_to_256() {
        for want in [16, 20, 24, 32, 48, 64, 256] {
            assert_eq!(ico_image(APP_ICO, want).unwrap().1, want);
        }
        assert_eq!(ico_image(APP_ICO, 30).unwrap().1, 32, "rounds up");
        assert_eq!(ico_image(APP_ICO, 999).unwrap().1, 256, "else the biggest");
    }

    #[test]
    fn a_broken_file_yields_nothing() {
        assert_eq!(ico_image(&[], 16), None);
        assert_eq!(ico_image(&[0, 0, 1, 0, 1, 0], 16), None);
    }

    #[cfg(windows)]
    #[test]
    fn windows_decodes_the_app_icon_to_rgba_with_a_transparent_edge() {
        let px = rgba_from_ico(APP_ICO, 64).unwrap();
        assert_eq!(px.len(), 64 * 64 * 4);
        assert!(
            px.as_chunks::<4>().0.iter().any(|p| p[3] == 0xFF),
            "has opaque pixels"
        );
        assert!(
            px.as_chunks::<4>().0.iter().any(|p| p[3] == 0),
            "has transparent pixels"
        );
    }
}
