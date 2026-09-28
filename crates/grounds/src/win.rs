//! Driving the game window on Windows: finding it, moving and clicking the mouse, the wheel, and
//! screenshots. Positions are in reference pixels (2560x1600) and scaled to the real client area.

use crate::{Px, REFERENCE_SIZE};

use anyhow::{anyhow, bail, Result};
use windows::core::w;
use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    BitBlt, ClientToScreen, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject,
    GetDC, GetDIBits, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    DIB_RGB_COLORS, SRCCOPY,
};

use windows::Win32::UI::Input::KeyboardAndMouse::{
    mouse_event, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN,
    MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_WHEEL,
    MOUSE_EVENT_FLAGS,
};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowW, GetClientRect, IsIconic, SetCursorPos, SetForegroundWindow, SetProcessDPIAware,
};

use std::thread::sleep;
use std::time::Duration;

/// Where the cursor rests between actions, in reference pixels.
const PARK: Px = (2380.0, 962.0);

pub struct Game {
    hwnd: HWND,
    origin: (i32, i32),
    size: (i32, i32),
    /// Where the cursor is, in client pixels, for gliding.
    cursor: (i32, i32),
}

fn ms(n: u64) {
    sleep(Duration::from_millis(n));
}

impl Game {
    /// Find the game window (it must be open and not minimized) and bring it forward.
    pub fn find() -> Result<Game> {
        unsafe {
            let _ = SetProcessDPIAware();
            let hwnd = FindWindowW(None, w!("Mechabellum"))
                .map_err(|_| anyhow!("Mechabellum isn't running (no window named Mechabellum)"))?;
            if IsIconic(hwnd).as_bool() {
                bail!("the Mechabellum window is minimized");
            }
            let mut rect = RECT::default();
            GetClientRect(hwnd, &mut rect)?;
            let mut origin = POINT { x: 0, y: 0 };
            let _ = ClientToScreen(hwnd, &mut origin);
            let _ = SetForegroundWindow(hwnd);
            ms(300);
            let size = (rect.right - rect.left, rect.bottom - rect.top);
            Ok(Game {
                hwnd,
                origin: (origin.x, origin.y),
                size,
                cursor: (size.0 / 2, size.1 / 2),
            })
        }
    }

    /// Find the game window, starting the game through Steam if it isn't running (it can
    /// crash). Waits up to 3 minutes for it to reach a screen with a known layout.
    pub fn find_or_launch() -> Result<Game> {
        if let Ok(game) = Game::find() {
            return Ok(game);
        }
        eprintln!("Mechabellum isn't running; starting it through Steam");
        std::process::Command::new("cmd")
            .args(["/C", "start", "", "steam://rungameid/669330"])
            .status()?;
        let start = std::time::Instant::now();
        while start.elapsed() < std::time::Duration::from_secs(180) {
            ms(5000);
            if let Ok(mut game) = Game::find() {
                // Let the main menu finish loading.
                for _ in 0..24 {
                    if crate::screen::classify(&game.screenshot()?) != crate::screen::Screen::Other
                    {
                        return Ok(game);
                    }
                    ms(5000);
                }
                return Ok(game);
            }
        }
        bail!("Mechabellum didn't start")
    }

    pub fn client_size(&self) -> (i32, i32) {
        self.size
    }

    fn to_client(&self, p: Px) -> (i32, i32) {
        (
            (p.0 / REFERENCE_SIZE.0 * f64::from(self.size.0)).round() as i32,
            (p.1 / REFERENCE_SIZE.1 * f64::from(self.size.1)).round() as i32,
        )
    }

    fn set_cursor(&mut self, c: (i32, i32)) {
        unsafe {
            let _ = SetCursorPos(self.origin.0 + c.0, self.origin.1 + c.1);
        }
        self.cursor = c;
    }

    /// Move the cursor there in small steps: the game tracks carried units by mouse moves.
    pub fn glide(&mut self, p: Px) {
        let (tx, ty) = self.to_client(p);
        let (fx, fy) = self.cursor;
        for i in 1..=12 {
            self.set_cursor((fx + (tx - fx) * i / 12, fy + (ty - fy) * i / 12));
            ms(30);
        }
        ms(250);
    }

    fn button(&self, down: MOUSE_EVENT_FLAGS, up: MOUSE_EVENT_FLAGS) {
        unsafe {
            mouse_event(down, 0, 0, 0, 0);
            ms(50);
            mouse_event(up, 0, 0, 0, 0);
            ms(90);
        }
    }

    pub fn click(&mut self, p: Px, wait_ms: u64) {
        let _ = unsafe { SetForegroundWindow(self.hwnd) };
        self.glide(p);
        self.button(MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP);
        ms(wait_ms);
    }

    pub fn double_click(&mut self, p: Px, wait_ms: u64) {
        self.glide(p);
        self.button(MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP);
        self.button(MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP);
        ms(wait_ms);
    }

    /// While carrying a unit, a middle click turns it 90 degrees.
    pub fn middle_click(&mut self, p: Px, wait_ms: u64) {
        self.glide(p);
        self.button(MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP);
        ms(wait_ms);
    }

    pub fn right_click(&mut self, p: Px, wait_ms: u64) {
        self.glide(p);
        self.button(MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP);
        ms(wait_ms);
    }

    /// Scroll the wheel at the middle of the window; negative zooms out.
    pub fn wheel(&mut self, steps: i32) {
        self.glide((REFERENCE_SIZE.0 / 2.0, REFERENCE_SIZE.1 / 2.0));
        for _ in 0..steps.unsigned_abs() {
            unsafe { mouse_event(MOUSEEVENTF_WHEEL, 0, 0, 120 * steps.signum(), 0) };
            ms(60);
        }
        ms(1000);
    }

    /// Scroll the wheel at a point; negative scrolls down (or zooms out).
    pub fn wheel_at(&mut self, p: Px, steps: i32) {
        self.glide(p);
        for _ in 0..steps.unsigned_abs() {
            unsafe { mouse_event(MOUSEEVENTF_WHEEL, 0, 0, 120 * steps.signum(), 0) };
            ms(40);
        }
        ms(600);
    }

    /// Park the cursor over the supplies counter: off the board, and away from the screen edges,
    /// where a resting cursor scrolls the camera.
    pub fn park(&mut self) {
        let c = self.to_client(PARK);
        self.set_cursor(c);
        ms(200);
    }

    /// The client area as an RGB image, scaled to reference size if needed.
    pub fn screenshot(&mut self) -> Result<image::RgbImage> {
        self.park();
        let (w, h) = self.size;
        unsafe {
            let screen = GetDC(HWND::default());
            let mem = CreateCompatibleDC(screen);
            let bmp = CreateCompatibleBitmap(screen, w, h);
            let old = SelectObject(mem, bmp);
            BitBlt(
                mem,
                0,
                0,
                w,
                h,
                screen,
                self.origin.0,
                self.origin.1,
                SRCCOPY,
            )?;
            let mut info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w,
                    biHeight: -h,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut buf = vec![0u8; (w * h * 4) as usize];
            GetDIBits(
                mem,
                bmp,
                0,
                h as u32,
                Some(buf.as_mut_ptr().cast()),
                &mut info,
                DIB_RGB_COLORS,
            );
            SelectObject(mem, old);
            let _ = DeleteObject(bmp);
            let _ = DeleteDC(mem);
            ReleaseDC(HWND::default(), screen);
            let mut img = image::RgbImage::new(w as u32, h as u32);
            for (i, px) in buf.chunks_exact(4).enumerate() {
                img.put_pixel(
                    i as u32 % w as u32,
                    i as u32 / w as u32,
                    image::Rgb([px[2], px[1], px[0]]),
                );
            }
            if (w, h) != (REFERENCE_SIZE.0 as i32, REFERENCE_SIZE.1 as i32) {
                img = image::imageops::resize(
                    &img,
                    REFERENCE_SIZE.0 as u32,
                    REFERENCE_SIZE.1 as u32,
                    image::imageops::FilterType::Triangle,
                );
            }
            Ok(img)
        }
    }
}
