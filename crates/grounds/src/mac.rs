//! Driving the game window on macOS: finding it, moving and clicking the mouse, the wheel, keys,
//! and screenshots. Positions are in reference pixels (2560x1600) and scaled to the window's
//! content area, so the game must run at a 16:10 size (fullscreen below a 16:10 notch, or
//! windowed at 16:10).
//!
//! The process that runs this (the terminal, or the desktop app) needs two permissions in System
//! Settings, Privacy & Security: Screen Recording, to capture the window, and Accessibility, to
//! send mouse and key events. Without Accessibility, macOS drops posted events without an error,
//! so `find` checks for it before doing anything. [`access`] reports both and [`request_access`]
//! asks for them.
//!
//! Every input first checks that the cursor is where the last one left it; if not, the user has
//! moved the mouse, and the run stops (see [`crate::stop`]).

use crate::{Px, REFERENCE_SIZE};

use anyhow::{bail, Result};

use std::ffi::{c_char, c_void};
use std::thread::sleep;
use std::time::Duration;

/// Where the cursor rests between actions, in reference pixels.
const PARK: Px = (2380.0, 962.0);

/// The game's process name, as the window server lists it.
const OWNER: &str = "Mechabellum";
/// The game's bundle identifier, used to bring it to the front.
const BUNDLE_ID: &str = "unity.GameRiver.MJfighter";

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct CGPoint {
    x: f64,
    y: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct CGSize {
    width: f64,
    height: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct CGRect {
    origin: CGPoint,
    size: CGSize,
}

type CFTypeRef = *const c_void;
type CGEventRef = *mut c_void;

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRelease(cf: CFTypeRef);
    fn CFArrayGetCount(array: CFTypeRef) -> isize;
    fn CFArrayGetValueAtIndex(array: CFTypeRef, index: isize) -> CFTypeRef;
    fn CFDictionaryGetValue(dict: CFTypeRef, key: CFTypeRef) -> CFTypeRef;
    fn CFNumberGetValue(number: CFTypeRef, kind: isize, out: *mut c_void) -> bool;
    fn CFStringGetCString(s: CFTypeRef, buf: *mut c_char, size: isize, encoding: u32) -> bool;
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    static kCGWindowNumber: CFTypeRef;
    static kCGWindowOwnerName: CFTypeRef;
    static kCGWindowBounds: CFTypeRef;
    static kCGWindowLayer: CFTypeRef;
    fn CGWindowListCopyWindowInfo(option: u32, relative_to: u32) -> CFTypeRef;
    fn CGRectMakeWithDictionaryRepresentation(dict: CFTypeRef, rect: *mut CGRect) -> bool;
    fn CGWindowListCreateImage(bounds: CGRect, option: u32, window: u32, image: u32) -> CFTypeRef;
    fn CGImageGetWidth(image: CFTypeRef) -> usize;
    fn CGImageGetHeight(image: CFTypeRef) -> usize;
    fn CGImageRelease(image: CFTypeRef);
    static kCGColorSpaceSRGB: CFTypeRef;
    fn CGColorSpaceCreateWithName(name: CFTypeRef) -> *mut c_void;
    fn CGColorSpaceRelease(space: *mut c_void);
    fn CGBitmapContextCreate(
        data: *mut c_void,
        width: usize,
        height: usize,
        bits_per_component: usize,
        bytes_per_row: usize,
        space: *mut c_void,
        bitmap_info: u32,
    ) -> *mut c_void;
    fn CGContextDrawImage(context: *mut c_void, rect: CGRect, image: CFTypeRef);
    fn CGContextSetInterpolationQuality(context: *mut c_void, quality: i32);
    fn CGContextRelease(context: *mut c_void);
    fn CGEventCreateMouseEvent(
        source: *const c_void,
        kind: u32,
        at: CGPoint,
        button: u32,
    ) -> CGEventRef;
    fn CGEventCreateScrollWheelEvent2(
        source: *const c_void,
        units: u32,
        wheel_count: u32,
        wheel1: i32,
        wheel2: i32,
        wheel3: i32,
    ) -> CGEventRef;
    fn CGEventCreateKeyboardEvent(source: *const c_void, key: u16, down: bool) -> CGEventRef;
    fn CGEventSetIntegerValueField(event: CGEventRef, field: u32, value: i64);
    fn CGEventPost(tap: u32, event: CGEventRef);
    fn CGEventCreate(source: *const c_void) -> CGEventRef;
    fn CGEventGetLocation(event: CGEventRef) -> CGPoint;
    fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGRequestScreenCaptureAccess() -> bool;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    static kCFBooleanTrue: CFTypeRef;
    static kCFTypeDictionaryKeyCallBacks: c_void;
    static kCFTypeDictionaryValueCallBacks: c_void;
    fn CFDictionaryCreate(
        allocator: *const c_void,
        keys: *const CFTypeRef,
        values: *const CFTypeRef,
        count: isize,
        key_callbacks: *const c_void,
        value_callbacks: *const c_void,
    ) -> CFTypeRef;
}

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    static kAXTrustedCheckOptionPrompt: CFTypeRef;
    fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(options: CFTypeRef) -> bool;
}

/// Whether this process may drive the game.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Access {
    /// Accessibility: sending mouse and key events.
    pub accessibility: bool,
    /// Screen Recording: capturing the game's window.
    pub screen_recording: bool,
}

/// What macOS currently allows this process.
pub fn access() -> Access {
    unsafe {
        Access {
            accessibility: AXIsProcessTrusted(),
            screen_recording: CGPreflightScreenCaptureAccess(),
        }
    }
}

/// Ask macOS for whichever permission is missing. Each request shows the system's prompt the
/// first time and adds this program to the list in System Settings, switched off; the user
/// switches it on there. Screen Recording takes effect after the program restarts.
pub fn request_access() -> Access {
    unsafe {
        if !AXIsProcessTrusted() {
            let keys = [kAXTrustedCheckOptionPrompt];
            let values = [kCFBooleanTrue];
            let options = CFDictionaryCreate(
                std::ptr::null(),
                keys.as_ptr(),
                values.as_ptr(),
                1,
                &kCFTypeDictionaryKeyCallBacks,
                &kCFTypeDictionaryValueCallBacks,
            );
            AXIsProcessTrustedWithOptions(options);
            if !options.is_null() {
                CFRelease(options);
            }
        }
        if !CGPreflightScreenCaptureAccess() {
            CGRequestScreenCaptureAccess();
        }
    }
    access()
}

/// Where the cursor is now, in global points.
fn cursor_location() -> Option<CGPoint> {
    unsafe {
        let event = CGEventCreate(std::ptr::null());
        if event.is_null() {
            return None;
        }
        let at = CGEventGetLocation(event);
        CFRelease(event as CFTypeRef);
        Some(at)
    }
}

/// How far the cursor may sit from where it was put before that counts as the user moving it,
/// in points. Posted moves land a few milliseconds late and are rounded.
const MOVED_BY_USER: f64 = 6.0;

const UTF8: u32 = 0x0800_0100;
const NUMBER_SINT64: isize = 4;

const LIST_ON_SCREEN_ONLY: u32 = 1 << 0;
const LIST_INCLUDING_WINDOW: u32 = 1 << 3;
const LIST_EXCLUDE_DESKTOP: u32 = 1 << 4;
const IMAGE_IGNORE_FRAMING: u32 = 1 << 0;
const IMAGE_SHOULD_BE_OPAQUE: u32 = 1 << 1;
const IMAGE_BEST_RESOLUTION: u32 = 1 << 3;
/// RGBX, one byte each, in memory order.
const ALPHA_NONE_SKIP_LAST: u32 = 5;
const INTERPOLATION_HIGH: i32 = 3;

const HID_TAP: u32 = 0;
const LEFT_DOWN: u32 = 1;
const LEFT_UP: u32 = 2;
const RIGHT_DOWN: u32 = 3;
const RIGHT_UP: u32 = 4;
const MOVED: u32 = 5;
const LEFT_DRAGGED: u32 = 6;
const RIGHT_DRAGGED: u32 = 7;
const OTHER_DOWN: u32 = 25;
const OTHER_UP: u32 = 26;
const OTHER_DRAGGED: u32 = 27;
const FIELD_CLICK_STATE: u32 = 1;
const FIELD_DELTA_X: u32 = 4;
const FIELD_DELTA_Y: u32 = 5;
const SCROLL_UNIT_LINE: u32 = 1;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Button {
    Left,
    Right,
    Middle,
}

impl Button {
    fn number(self) -> u32 {
        match self {
            Button::Left => 0,
            Button::Right => 1,
            Button::Middle => 2,
        }
    }

    fn events(self) -> (u32, u32, u32) {
        match self {
            Button::Left => (LEFT_DOWN, LEFT_UP, LEFT_DRAGGED),
            Button::Right => (RIGHT_DOWN, RIGHT_UP, RIGHT_DRAGGED),
            Button::Middle => (OTHER_DOWN, OTHER_UP, OTHER_DRAGGED),
        }
    }
}

/// A window the window server lists for the game's process.
#[derive(Clone, Copy, Debug)]
struct WindowInfo {
    id: u32,
    /// In points, top-left origin, global display coordinates.
    bounds: CGRect,
    /// Whether no ordinary window of another app is in front of it.
    front: bool,
}

pub struct Game {
    window: u32,
    /// The content area's top-left corner, in global points.
    origin: (f64, f64),
    /// The content area's size, in points.
    size: (f64, f64),
    /// How far the content area starts below the window's top edge (a title bar or a
    /// letterbox bar), in points.
    inset_top: f64,
    /// The whole window's height, in points.
    window_height: f64,
    /// Where the cursor is, in content points, for gliding.
    cursor: (f64, f64),
    /// The button held down, if any, so moves are sent as drags.
    held: Option<Button>,
}

/// Wait, unless the run has stopped: then whatever was being waited for won't happen.
fn ms(n: u64) {
    if crate::stop::reason().is_none() {
        sleep(Duration::from_millis(n));
    }
}

/// Read a CFString value from a window-info dictionary.
unsafe fn dict_string(dict: CFTypeRef, key: CFTypeRef) -> Option<String> {
    let value = CFDictionaryGetValue(dict, key);
    if value.is_null() {
        return None;
    }
    let mut buf = [0 as c_char; 256];
    CFStringGetCString(value, buf.as_mut_ptr(), buf.len() as isize, UTF8).then(|| {
        std::ffi::CStr::from_ptr(buf.as_ptr())
            .to_string_lossy()
            .into_owned()
    })
}

unsafe fn dict_number(dict: CFTypeRef, key: CFTypeRef) -> Option<i64> {
    let value = CFDictionaryGetValue(dict, key);
    if value.is_null() {
        return None;
    }
    let mut out = 0i64;
    CFNumberGetValue(value, NUMBER_SINT64, (&mut out as *mut i64).cast()).then_some(out)
}

/// The game's largest on-screen window, and whether it's frontmost.
fn game_window() -> Option<WindowInfo> {
    windows(true)
}

/// Whether the game has a window at all, on screen or not (in another Space, say).
fn game_running() -> bool {
    windows(false).is_some()
}

/// The game's largest ordinary window, from the on-screen windows or from all of them.
fn windows(on_screen_only: bool) -> Option<WindowInfo> {
    unsafe {
        let option = if on_screen_only {
            LIST_ON_SCREEN_ONLY | LIST_EXCLUDE_DESKTOP
        } else {
            LIST_EXCLUDE_DESKTOP
        };
        let list = CGWindowListCopyWindowInfo(option, 0);
        if list.is_null() {
            return None;
        }
        let mut best: Option<WindowInfo> = None;
        // The list runs front to back; ordinary app windows are on layer 0.
        let mut other_in_front = false;
        for i in 0..CFArrayGetCount(list) {
            let dict = CFArrayGetValueAtIndex(list, i);
            let layer = dict_number(dict, kCGWindowLayer).unwrap_or(-1);
            let owner = dict_string(dict, kCGWindowOwnerName).unwrap_or_default();
            let mut bounds = CGRect::default();
            let bounds_dict = CFDictionaryGetValue(dict, kCGWindowBounds);
            if bounds_dict.is_null()
                || !CGRectMakeWithDictionaryRepresentation(bounds_dict, &mut bounds)
            {
                continue;
            }
            if owner != OWNER {
                // Tiny windows (status items, overlays) don't cover the game.
                if layer == 0 && bounds.size.width > 200.0 && bounds.size.height > 200.0 {
                    other_in_front = true;
                }
                continue;
            }
            if layer != 0 {
                continue;
            }
            let area = bounds.size.width * bounds.size.height;
            if best.is_some_and(|b| b.bounds.size.width * b.bounds.size.height >= area) {
                continue;
            }
            best = Some(WindowInfo {
                id: dict_number(dict, kCGWindowNumber).unwrap_or(0) as u32,
                bounds,
                front: !other_in_front,
            });
        }
        CFRelease(list);
        best
    }
}

/// How much of the window's top and bottom the game doesn't draw in, in points. The layout is
/// 16:10, so height beyond that is either a title bar (windowed: 28 points or so, all at the top)
/// or letterboxing (fullscreen below the notch on a 16-inch MacBook Pro: the window is 1728x1084
/// and the game draws 1728x1080 in the middle, with black bars of 2 points above and below).
fn content_insets(bounds: CGRect) -> (f64, f64) {
    let extra = bounds.size.height - bounds.size.width * REFERENCE_SIZE.1 / REFERENCE_SIZE.0;
    if !(0.5..=80.0).contains(&extra) {
        (0.0, 0.0)
    } else if extra < 20.0 {
        (extra / 2.0, extra / 2.0)
    } else {
        (extra, 0.0)
    }
}

/// Bring the game to the front (switching to its Space when it's fullscreen).
fn activate() {
    let _ = std::process::Command::new("/usr/bin/open")
        .args(["-b", BUNDLE_ID])
        .status();
}

fn post(event: CGEventRef) {
    if event.is_null() {
        return;
    }
    unsafe {
        CGEventPost(HID_TAP, event);
        CFRelease(event as CFTypeRef);
    }
}

impl Game {
    /// Find the game window (it must be open and on screen) and bring it forward.
    pub fn find() -> Result<Game> {
        crate::stop::check()?;
        let allowed = access();
        if !allowed.accessibility {
            bail!(
                "macOS doesn't let this program send mouse or key events: switch on the app that \
                 runs it (your terminal, or Mechabellum Stats) under System Settings, Privacy & \
                 Security, Accessibility (macOS drops the events silently otherwise)"
            );
        }
        if !allowed.screen_recording {
            bail!(
                "macOS doesn't let this program capture the screen: switch on the app that runs \
                 it (your terminal, or Mechabellum Stats) under System Settings, Privacy & \
                 Security, Screen & System Audio Recording, then restart that app"
            );
        }
        let mut info = game_window();
        // A fullscreen game in another Space isn't on screen until it's brought forward. Only
        // bring forward a game that's running: `open` would start one outside Steam.
        if !info.is_some_and(|w| w.front) && game_running() {
            activate();
            ms(1500);
            info = game_window();
        }
        let Some(info) = info else {
            bail!("Mechabellum isn't running (no window owned by Mechabellum on screen)");
        };
        let (inset_top, inset_bottom) = content_insets(info.bounds);
        let size = (
            info.bounds.size.width,
            info.bounds.size.height - inset_top - inset_bottom,
        );
        let aspect = size.0 / size.1;
        let want = REFERENCE_SIZE.0 / REFERENCE_SIZE.1;
        if (aspect - want).abs() > 0.02 {
            eprintln!(
                "warning: the game's content is {:.0}x{:.0} points ({aspect:.3}:1), not 16:10; \
                 screen positions are measured at 16:10 and may miss",
                size.0, size.1
            );
        }
        ms(300);
        let origin = (info.bounds.origin.x, info.bounds.origin.y + inset_top);
        // Start from wherever the cursor is, so the first check doesn't take the user's resting
        // cursor for a move.
        let cursor = cursor_location().map_or((size.0 / 2.0, size.1 / 2.0), |at| {
            (at.x - origin.0, at.y - origin.1)
        });
        Ok(Game {
            window: info.id,
            origin,
            size,
            inset_top,
            window_height: info.bounds.size.height,
            cursor,
            held: None,
        })
    }

    /// Whether input may still be sent. False once a stop was asked for, or once the cursor
    /// isn't where the last input put it (the user moved the mouse: that stops the run). On the
    /// way out it lets go of a held button.
    fn live(&mut self) -> bool {
        if crate::stop::reason().is_none() && self.moved_by_user() {
            crate::stop::mouse_moved();
        }
        if crate::stop::reason().is_none() {
            return true;
        }
        if let Some(button) = self.held {
            self.press(button, false, 1);
        }
        false
    }

    fn moved_by_user(&self) -> bool {
        let expected = self.global(self.cursor);
        let off = |at: CGPoint| (at.x - expected.x).hypot(at.y - expected.y) > MOVED_BY_USER;
        // Read twice, a moment apart: the last posted move may not have landed yet.
        if !cursor_location().is_some_and(off) {
            return false;
        }
        ms(60);
        cursor_location().is_some_and(off)
    }

    /// Find the game window, starting the game through Steam if it isn't running (it can
    /// crash). Waits up to 3 minutes for it to reach a screen with a known layout.
    pub fn find_or_launch() -> Result<Game> {
        if let Ok(game) = Game::find() {
            return Ok(game);
        }
        if game_running() {
            // It's running; the failure was something else (a permission).
            return Game::find();
        }
        eprintln!("Mechabellum isn't running; starting it through Steam");
        std::process::Command::new("/usr/bin/open")
            .arg("steam://rungameid/669330")
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

    /// The content area's size, in points.
    pub fn client_size(&self) -> (i32, i32) {
        (self.size.0 as i32, self.size.1 as i32)
    }

    fn to_client(&self, p: Px) -> (f64, f64) {
        (
            p.0 / REFERENCE_SIZE.0 * self.size.0,
            p.1 / REFERENCE_SIZE.1 * self.size.1,
        )
    }

    fn global(&self, c: (f64, f64)) -> CGPoint {
        CGPoint {
            x: self.origin.0 + c.0,
            y: self.origin.1 + c.1,
        }
    }

    /// Bring the game forward if another window is in front of it, as a click would otherwise
    /// land in that window.
    fn front(&self) {
        // Once stopped, leave whatever the user switched to in front.
        if crate::stop::reason().is_none() && game_window().is_some_and(|w| !w.front) {
            activate();
            ms(1000);
        }
    }

    /// Move the cursor to a content point, as a drag when a button is held.
    fn set_cursor(&mut self, c: (f64, f64)) {
        if !self.live() {
            return;
        }
        let kind = match self.held {
            None => MOVED,
            Some(b) => b.events().2,
        };
        let button = self.held.map_or(0, Button::number);
        let (dx, dy) = (c.0 - self.cursor.0, c.1 - self.cursor.1);
        unsafe {
            let event = CGEventCreateMouseEvent(std::ptr::null(), kind, self.global(c), button);
            if !event.is_null() {
                CGEventSetIntegerValueField(event, FIELD_DELTA_X, dx.round() as i64);
                CGEventSetIntegerValueField(event, FIELD_DELTA_Y, dy.round() as i64);
            }
            post(event);
        }
        self.cursor = c;
    }

    /// Move the cursor there in small steps: the game tracks carried units by mouse moves.
    pub fn glide(&mut self, p: Px) {
        let (tx, ty) = self.to_client(p);
        let (fx, fy) = self.cursor;
        for i in 1..=12 {
            let f = f64::from(i) / 12.0;
            self.set_cursor((fx + (tx - fx) * f, fy + (ty - fy) * f));
            ms(30);
        }
        ms(250);
    }

    fn press(&mut self, button: Button, down: bool, clicks: i64) {
        if down && !self.live() {
            return;
        }
        // Let go only of a button this pressed: after a stop the press was never sent, and a
        // release (which carries a position) would move the user's cursor back.
        if !down && self.held != Some(button) {
            return;
        }
        // A stop mustn't leave a button held: let go where the user has the cursor now.
        let at = if crate::stop::reason().is_some() {
            cursor_location().unwrap_or_else(|| self.global(self.cursor))
        } else {
            self.global(self.cursor)
        };
        let (down_kind, up_kind, _) = button.events();
        let kind = if down { down_kind } else { up_kind };
        unsafe {
            let event = CGEventCreateMouseEvent(std::ptr::null(), kind, at, button.number());
            if !event.is_null() {
                CGEventSetIntegerValueField(event, FIELD_CLICK_STATE, clicks);
            }
            post(event);
        }
        self.held = down.then_some(button);
    }

    fn button(&mut self, button: Button, clicks: i64) {
        self.press(button, true, clicks);
        ms(50);
        self.press(button, false, clicks);
        ms(90);
    }

    pub fn click(&mut self, p: Px, wait_ms: u64) {
        self.front();
        self.glide(p);
        self.button(Button::Left, 1);
        ms(wait_ms);
    }

    pub fn double_click(&mut self, p: Px, wait_ms: u64) {
        self.glide(p);
        self.button(Button::Left, 1);
        self.button(Button::Left, 2);
        ms(wait_ms);
    }

    /// While carrying a unit, a middle click turns it 90 degrees.
    pub fn middle_click(&mut self, p: Px, wait_ms: u64) {
        self.glide(p);
        self.button(Button::Middle, 1);
        ms(wait_ms);
    }

    pub fn right_click(&mut self, p: Px, wait_ms: u64) {
        self.glide(p);
        self.button(Button::Right, 1);
        ms(wait_ms);
    }

    /// Press or release the left button where the cursor is (for drags made of glides).
    pub fn left_button(&mut self, down: bool) {
        self.press(Button::Left, down, 1);
        ms(150);
    }

    /// Hold the right button and move from `from` to `to`: this pans the camera.
    pub fn right_drag(&mut self, from: Px, to: Px, wait_ms: u64) {
        self.glide(from);
        self.press(Button::Right, true, 1);
        ms(120);
        self.glide(to);
        self.press(Button::Right, false, 1);
        ms(wait_ms);
    }

    /// Hold the middle button and move from `from` to `to`: this turns the camera. The game
    /// reads turning from mouse deltas, which the drag events carry. A cursor at the screen's
    /// edge scrolls the camera, so keep drags short and start them mid-screen.
    pub fn middle_drag(&mut self, from: Px, to: Px, wait_ms: u64) {
        self.glide(from);
        let a = self.to_client(from);
        let b = self.to_client(to);
        self.press(Button::Middle, true, 1);
        ms(120);
        const STEPS: i32 = 40;
        for i in 1..=STEPS {
            let f = f64::from(i) / f64::from(STEPS);
            self.set_cursor((a.0 + (b.0 - a.0) * f, a.1 + (b.1 - a.1) * f));
            ms(20);
        }
        ms(120);
        self.press(Button::Middle, false, 1);
        // Put the cursor back where the drag began, as the Windows driver does.
        self.set_cursor(a);
        ms(wait_ms);
    }

    /// Hold a letter key (a camera control: Q, E, R, F) for `ms_held`. Only those letters:
    /// never Enter, which opens the chat.
    pub fn hold_key(&mut self, letter: char, ms_held: u64) {
        if !self.live() {
            return;
        }
        // macOS virtual key codes (ANSI layout).
        let key: u16 = match letter.to_ascii_uppercase() {
            'Q' => 0x0C,
            'E' => 0x0E,
            'R' => 0x0F,
            'F' => 0x03,
            _ => return,
        };
        self.front();
        unsafe {
            post(CGEventCreateKeyboardEvent(std::ptr::null(), key, true));
            ms(ms_held);
            post(CGEventCreateKeyboardEvent(std::ptr::null(), key, false));
        }
        ms(600);
    }

    fn scroll(&mut self, steps: i32, gap_ms: u64) {
        for _ in 0..steps.unsigned_abs() {
            if !self.live() {
                return;
            }
            unsafe {
                post(CGEventCreateScrollWheelEvent2(
                    std::ptr::null(),
                    SCROLL_UNIT_LINE,
                    1,
                    steps.signum(),
                    0,
                    0,
                ));
            }
            ms(gap_ms);
        }
    }

    /// Scroll the wheel at the middle of the window; negative zooms out.
    pub fn wheel(&mut self, steps: i32) {
        self.glide((REFERENCE_SIZE.0 / 2.0, REFERENCE_SIZE.1 / 2.0));
        self.scroll(steps, 60);
        ms(1000);
    }

    /// Scroll the wheel at a point; negative scrolls down (or zooms out).
    pub fn wheel_at(&mut self, p: Px, steps: i32) {
        self.glide(p);
        self.scroll(steps, 40);
        ms(600);
    }

    /// Park the cursor over the supplies counter: off the board, and away from the screen edges,
    /// where a resting cursor scrolls the camera.
    pub fn park(&mut self) {
        let c = self.to_client(PARK);
        self.set_cursor(c);
        ms(200);
    }

    /// The content area as an RGB image, scaled to reference size if needed.
    pub fn screenshot(&mut self) -> Result<image::RgbImage> {
        self.park();
        self.screenshot_here()
    }

    /// The content area without moving the cursor first, e.g. while it carries a unit. Fails
    /// once the run has stopped, so whatever is running unwinds.
    pub fn screenshot_here(&mut self) -> Result<image::RgbImage> {
        self.live();
        crate::stop::check()?;
        capture_window(
            self.window,
            self.inset_top / self.window_height,
            self.size.1 / self.window_height,
        )
    }
}

/// One window's content area at reference size, as sRGB. The content is the band of the window
/// from `top` down `height` (both fractions of the window's height). CoreGraphics crops and
/// scales while drawing, which is far quicker than resizing the 3456x2160 capture in Rust.
fn capture_window(window: u32, top: f64, height: f64) -> Result<image::RgbImage> {
    unsafe {
        let null_rect = CGRect {
            origin: CGPoint {
                x: f64::INFINITY,
                y: f64::INFINITY,
            },
            size: CGSize::default(),
        };
        let image = CGWindowListCreateImage(
            null_rect,
            LIST_INCLUDING_WINDOW,
            window,
            IMAGE_IGNORE_FRAMING | IMAGE_SHOULD_BE_OPAQUE | IMAGE_BEST_RESOLUTION,
        );
        if image.is_null() {
            bail!("couldn't capture the game window (is Screen Recording allowed?)");
        }
        let (w, h) = (CGImageGetWidth(image), CGImageGetHeight(image));
        if w == 0 || h == 0 || w > 16384 || h > 16384 {
            CGImageRelease(image);
            bail!("the game window capture is {w}x{h}");
        }
        let (tw, th) = (REFERENCE_SIZE.0 as usize, REFERENCE_SIZE.1 as usize);
        let mut buf = vec![0u8; tw * th * 4];
        let space = CGColorSpaceCreateWithName(kCGColorSpaceSRGB);
        let context = CGBitmapContextCreate(
            buf.as_mut_ptr().cast(),
            tw,
            th,
            8,
            tw * 4,
            space,
            ALPHA_NONE_SKIP_LAST,
        );
        CGColorSpaceRelease(space);
        if context.is_null() {
            CGImageRelease(image);
            bail!("couldn't make a bitmap for the capture");
        }
        // Scale the whole capture so the content band fills the bitmap; the rest falls outside
        // it. The bitmap's origin is its bottom-left corner.
        let (w, h) = (w as f64, h as f64);
        let sx = tw as f64 / w;
        let sy = th as f64 / (height * h);
        let below = (1.0 - top - height) * h;
        let rect = CGRect {
            origin: CGPoint {
                x: 0.0,
                y: -below * sy,
            },
            size: CGSize {
                width: w * sx,
                height: h * sy,
            },
        };
        CGContextSetInterpolationQuality(context, INTERPOLATION_HIGH);
        CGContextDrawImage(context, rect, image);
        CGContextRelease(context);
        CGImageRelease(image);
        let rgb: Vec<u8> = buf
            .chunks_exact(4)
            .flat_map(|px| [px[0], px[1], px[2]])
            .collect();
        image::RgbImage::from_raw(tw as u32, th as u32, rgb)
            .ok_or_else(|| anyhow::anyhow!("capture buffer size"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(w: f64, h: f64) -> CGRect {
        CGRect {
            origin: CGPoint::default(),
            size: CGSize {
                width: w,
                height: h,
            },
        }
    }

    #[test]
    fn test_content_insets() {
        // Fullscreen at exactly 16:10.
        assert_eq!(content_insets(rect(1728.0, 1080.0)), (0.0, 0.0));
        // Fullscreen below the notch: 4 points of letterbox, half above and half below
        // (measured: 4 black pixel rows at each edge of a 3456x2168 capture).
        assert_eq!(content_insets(rect(1728.0, 1084.0)), (2.0, 2.0));
        // Windowed 1280x800 content under a 28-point title bar.
        assert_eq!(content_insets(rect(1280.0, 828.0)), (28.0, 0.0));
        // Not 16:10 at all (a 4:3 window): don't guess.
        assert_eq!(content_insets(rect(1024.0, 768.0)), (0.0, 0.0));
        // Wider than 16:10: nothing to take off.
        assert_eq!(content_insets(rect(1920.0, 1080.0)), (0.0, 0.0));
    }
}
