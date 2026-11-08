//! Custom watch faces for the Garmin Instinct (006-B3126), firmware 19.10.
//!
//! One hook. The first instruction of the stock face renderer FUN_00062858
//! (push.w {r4-r11, lr}) becomes "b.w face_entry". face_entry runs the stock
//! renderer unchanged, then face_draw paints the custom face over it. The stock
//! code keeps doing everything it did before (inset state, partial updates,
//! value bookkeeping), so with no face file, a bad file or the kill switch the
//! watch looks exactly like stock.
//!
//! Files (USB drive, GARMIN/FACES/):
//!   ACTIVE.FACE  the face (IFC1 format, faces/facekit.py); up to 32 KB
//!   OFF          if present, custom faces are off
//!   STATE.BIN    crash marker written by the watch: status byte + file
//!                signature (u32 length, u32 FNV-1a). A new file is marked
//!                'R' before its first render and 'G' after it. A file still
//!                marked 'R' at the next draw crashed or hung the watch: it
//!                becomes 'X' and is never drawn again. Copy a different file
//!                (or delete STATE.BIN) to retry.
//!
//! Not drawn on the watch yet: the face file's INSET op (the stock inset
//! stays, as set in the watch's own face settings, unless the face owns it),
//! phone data, wind, altitude, sensor temperature, alarm, do-not-disturb
//! (all show as "--").
//! Weather temperature, high/low, condition and precip come from the phone's
//! last report (Gadgetbridge or Garmin Connect), in the watch's C/F setting.
//!
//! No writable statics: the linker script rejects .data/.bss.

#![no_std]
#![allow(clippy::missing_safety_doc)]

#[path = "../../src/face.rs"]
#[allow(dead_code)]
mod face;
#[path = "../../src/planecore.rs"]
#[allow(dead_code)]
mod planecore;
mod plane;

use core::arch::global_asm;
use core::mem::{size_of, transmute};
use core::ptr::addr_of_mut;

// ---- firmware (19.10), Thumb, called with |1 --------------------------------
const GET_DC: usize = 0x0004_79c8; // () -> dc
const FILL_RECT: usize = 0x0006_b358; // (dc, x, y, w, h), colour dc[0x31], dc[0x32]
const THEME_PP: usize = 0x0003_b440; // *THEME_PP -> theme, [3] fg, [4] bg
const MALLOC: usize = 0x0000_3af8; // (size) -> ptr or 0
const FREE: usize = 0x0000_39d8; // (ptr)
const F_EXISTS: usize = 0x0009_b168; // (path) -> bool
const F_OPEN: usize = 0x0009_b48c; // (path, flags) -> fd >= 0
const F_READ: usize = 0x0009_b744; // (fd, buf, len) -> bytes or < 0
const F_WRITE: usize = 0x0009_ba88; // (fd, buf, len) -> bytes or < 0
const F_CLOSE: usize = 0x0009_b044; // (fd)
const O_READ: u32 = 0x20; // FatFs FA_READ
const O_WRITE: u32 = 0x42; // FA_WRITE | FA_OPEN_ALWAYS (no truncate)

const TIME_NOW: usize = 0x1017_e248; // (&t) -> 1 when the clock is set
const TIME_CONV: usize = 0x1017_dbac; // (dst, &src, code, 1)
const CV_HMS: u32 = 0x29; // -> {i16 hour, u8 min, u8 sec}
const CV_DATE: u32 = 0x2b; // -> {u8 month, u8 day, u16 year}
const SETTINGS: usize = 0x2000_2458; // +7 time mode (conv code), +0x1c 12/24h (1 = 12h)

const BATTERY: usize = 0x0001_92a0; // (0) -> percent
const HEART: usize = 0x000d_7884; // (&s, 100); byte +2 = bpm, 0xff = none
const STEPS: usize = 0x0003_43c0;
const STEP_GOAL: usize = 0x0003_441c;
const CAL_ACTIVE: usize = 0x0003_4280;
const CAL_REST: usize = 0x0003_4234;
const FLOORS: usize = 0x1007_f31c; // u16
const INTENSITY: usize = 0x1007_d238;
const PHONE: usize = 0x000a_ee78; // () -> phone connected
const NOTIF_LOCK: usize = 0x000b_2354;
const NOTIF_UNLOCK: usize = 0x000b_2360;
const NOTIF_FIRST: usize = 0x000b_1f3c;
const NOTIF_HAS: usize = 0x000b_1f44;
const NOTIF_NEXT: usize = 0x000b_1f6c;
const POSITION: usize = 0x000d_39f0; // (&pos, 1)
const SUN_TZ: usize = 0x1017_ab6c; // (t, pos0, pos1) -> x
const SUN_TIMES: usize = 0x1017_e884; // (&pos, t, x, &rise, &set), local seconds of day

// Weather, as the stock face's inset type 7 reads it (renderer 0x63904): thunk to
// FUN_1011f400 -> FUN_000c0af4(buf, 1), copies the 0x60-byte record at
// 0x20006bcc under its lock and patches today's high/low/precip from the daily
// forecast. Returns 0 when a report has arrived since boot. If the data is over
// 20 min old and the phone is linked it also flags a weather request, same as
// stock. Gadgetbridge and Garmin Connect fill the same record (FIT msg 128).
const WX_GET: usize = 0x000e_ebf0; // (buf[0x60]) -> u8 status
const WX_REC: usize = 0x60;
const WX_TEMP_OFF: usize = 0x55; // s8 deg C, 0x7f = none
const WX_COND_OFF: usize = 0x56; // FIT weather_status, 0xff = none
const WX_PRECIP_OFF: usize = 0x57; // percent
const WX_HI_OFF: usize = 0x5b; // s8 deg C
const WX_LO_OFF: usize = 0x5c; // s8 deg C
// FUN_0008b184(3, 0, 0x15, &u8): temperature unit setting, 1 = deg F (as
// FUN_1011f43c reads it before formatting the stock inset)
const SETTING_GET: usize = 0x0008_b184;
// Stock inset state read by the painter FUN_00057d94: +0 mode, +1 invert, +0xc
// icon / text / callback depending on the mode.
const INSET_STATE: usize = 0x2000_9874;
// Music now playing: getter fills a 0x34-byte out struct and
// keeps the music mutex held on success; the release clears it and unlocks.
const MUS_GET: usize = 0x1016_9d7c; // (out) -> 0 none (unlocked) / 1 (locked)
const MUS_RELEASE: usize = 0x1016_9e08; // (out)
const MUS_OUT: usize = 0x34;
const MUS_STR: usize = 100; // record slots are 100 B, value <= 99 B + NUL
const TICK_MS: usize = 0x0000_5c48; // () -> ms tick, the clock the record's tick uses
// Music refresh without new wake-ups. The face page already gets a timer event
// (0xf0) every 1000 ms: FUN_00055b60 calls FUN_0004c664(face node, 1000), and
// the page manager (0x4bb5c) posts 0xf0 to visible pages only. page_entry runs
// the stock page handler FUN_000562e0, then face_tick on 0xf0 asks for one
// repaint (0x5f, same as the handler's own create path) when the phone sent new
// playback state in the last tick (song swap, play/pause, seek: Gadgetbridge
// sends track info then playback state, and only on change) or the playing
// position crossed a PLAY_STEP_MS boundary.
const MUS_ACTIVE_SRC: usize = 0x2001_f786; // u8, < 2 when a phone feeds the record
const CUR_NODE: usize = 0x2000_9214; // page node being dispatched (DAT_00056930)
const PAGE_POST: usize = 0x0004_bf24; // (node, event), UI task queue
const EV_PAINT: u32 = 0x5f;
const EV_TICK: u32 = 0xf0;
const TICK_PERIOD: u32 = 1000;
const PLAY_STEP_MS: u32 = 15_000;
const ST_MUSIC: u8 = 1; // STATE.BIN flags: the active face shows music
// value-source slots (faces/facekit.py WX_*)
const S_WX_COND: usize = 50;
const S_WX_TEMP: usize = 51;
const S_WX_HI: usize = 52;
const S_WX_LO: usize = 53;
const S_WX_PRECIP: usize = 54;

// the stock face editor (blinks the field being edited); leave it stock
const EDITOR: (usize, usize) = (0x0005_6e64, 0x0005_74d8);

// Repaint gate (battery). The face page's 1 Hz timer (0xf0, states 0 and 2)
// calls FUN_00055cd0, which fills the time into vals[0] ({i16 hour, u8 min,
// u8 sec}), calls the renderer FUN_00062858 (bl at 0x55cee, patched to
// tick_entry), then copies vals[0] to vals[1]. So the stock face repaints every
// second, but in display mode 2 the renderer only refills its values when
// hour:minute (vals[0] & 0xffffff) changed. A custom face used to re-read,
// re-render and re-blit its file on every one of those calls. tick_entry skips
// the whole repaint (stock and custom) when nothing on screen can have changed:
// the last repaint on this page drew the custom face in mode 2, the mode is
// still 2, hour:minute is unchanged and the stock inset shows a value that
// only changes with the values (STATIC_INSETS). Music, phone pushes and every
// other repaint (0x5f) still go through the ungated path in FUN_000562e0.
// Page state block (node + 0xc, 0x194 bytes, zeroed at page create): the
// stock code uses bytes up to 0x190; 0x191..0x193 are padding (no access in
// 0x50000-0x66000, and the text context at +0x120 ends before the halfwords
// at +0x18c). We keep two bytes there.
const MODE: usize = 0x2001_f5c9; // u8 display mode (FUN_0005fdfc)
const ST_DRAWN: usize = 0x191; // 1: the last repaint drew the custom face
const ST_MODE: usize = 0x192; // display mode at that repaint
const CFG_IN_STATE: usize = 10; // cfg = state + 10 at both face-page call sites
// callers whose cfg sits in the face page's state block: FUN_000562e0 (0x5f
// paint, bl at 0x56348) and FUN_00055cd0 (1 Hz, via tick_entry)
const PAGE_FN: (usize, usize) = (0x0005_62e0, 0x0005_6984);
const TICK_FN: (usize, usize) = (0x0005_5cd0, 0x0005_5d3e);

/// Inset types (cfg[7]) whose content comes only from the values the renderer
/// refills on a minute change, or never changes: 1 steps, 2 calories, 4
/// floors, 5 intensity, 8 calendar, 0xe altitude, 0x12 heart rate, 0x18 icon
/// + value, 0x20 logo, 0x33 fixed icon, 0xff none. Every other type (seconds,
/// gauges, live reads such as 0x11's phone link, unknown) keeps the stock 1 Hz
/// repaint.
fn static_inset(t: u8) -> bool {
    matches!(t, 1 | 2 | 4 | 5 | 8 | 0xe | 0x12 | 0x18 | 0x20 | 0x33 | 0xff)
}

// inset painter's circle (FUN_00057d94: centre (105, 22), radius 27); we never
// paint inside it, whichever order the two run in
const INSET_CX2: i32 = 210; // doubled
const INSET_CY2: i32 = 44;
const INSET_R2: i32 = 54;

const MAX_FILE: usize = 32 * 1024;

const P_FACE: &[u8] = b"0:/GARMIN/FACES/ACTIVE.FACE\0";
const P_OFF: &[u8] = b"0:/GARMIN/FACES/OFF\0";
const P_STATE: &[u8] = b"0:/GARMIN/FACES/STATE.BIN\0";

#[inline(always)]
unsafe fn f0<R>(a: usize) -> extern "C" fn() -> R {
    transmute::<usize, extern "C" fn() -> R>(a | 1)
}
#[inline(always)]
unsafe fn f1<A, R>(a: usize) -> extern "C" fn(A) -> R {
    transmute::<usize, extern "C" fn(A) -> R>(a | 1)
}
#[inline(always)]
unsafe fn f2<A, B, R>(a: usize) -> extern "C" fn(A, B) -> R {
    transmute::<usize, extern "C" fn(A, B) -> R>(a | 1)
}
#[inline(always)]
unsafe fn f3<A, B, C, R>(a: usize) -> extern "C" fn(A, B, C) -> R {
    transmute::<usize, extern "C" fn(A, B, C) -> R>(a | 1)
}

// ---- files ------------------------------------------------------------------

unsafe fn exists(p: &[u8]) -> bool {
    f1::<*const u8, i32>(F_EXISTS)(p.as_ptr()) != 0
}
unsafe fn open(p: &[u8], fl: u32) -> i32 {
    f2::<*const u8, u32, i32>(F_OPEN)(p.as_ptr(), fl)
}
unsafe fn close(fd: i32) {
    f1::<i32, i32>(F_CLOSE)(fd);
}
/// Read exactly buf.len() bytes; false on error or short file.
unsafe fn read_all(fd: i32, buf: &mut [u8]) -> bool {
    let rd = f3::<i32, *mut u8, u32, i32>(F_READ);
    let mut got = 0usize;
    while got < buf.len() {
        let n = rd(fd, buf.as_mut_ptr().add(got), (buf.len() - got) as u32);
        if n <= 0 {
            return false;
        }
        got += n as usize;
    }
    true
}

struct Heap(*mut u8);
impl Heap {
    unsafe fn new(n: usize) -> Option<Heap> {
        let p = f1::<u32, *mut u8>(MALLOC)(n as u32);
        if p.is_null() { None } else { Some(Heap(p)) }
    }
}
impl Drop for Heap {
    fn drop(&mut self) {
        unsafe { f1::<*mut u8, ()>(FREE)(self.0) }
    }
}

/// Everything big lives here, on the heap, not on the UI task's stack.
struct Work {
    px: [u8; face::CANVAS],
    sc: face::Scratch,
    v: face::Vals<'static>,
    // copies of the now-playing strings (title, artist, album); v.strs points
    // here, so they must outlive the render, which they do (same block)
    mus: [[u8; MUS_STR]; 3],
}

fn fnv(b: &[u8]) -> u32 {
    let mut h = 0x811c_9dc5u32;
    for &c in b {
        h = (h ^ c as u32).wrapping_mul(0x0100_0193);
    }
    h
}

/// [status][u32 len][u32 FNV][flags]. A 9-byte file from an older loader reads
/// as all zero (unknown), so the face goes through R/G once and gains flags.
const STATE_LEN: usize = 10;
unsafe fn state_read() -> [u8; STATE_LEN] {
    let mut s = [0u8; STATE_LEN];
    if exists(P_STATE) {
        let fd = open(P_STATE, O_READ);
        if fd >= 0 {
            if !read_all(fd, &mut s) {
                s = [0; STATE_LEN];
            }
            close(fd);
        }
    }
    s
}
/// false if the marker could not be written (then we do not draw: without a
/// working marker a crashing face could not be stopped).
unsafe fn state_write(st: u8, sig: &[u8; 8], flags: u8) -> bool {
    let fd = open(P_STATE, O_WRITE);
    if fd < 0 {
        return false;
    }
    let mut s = [0u8; STATE_LEN];
    s[0] = st;
    s[1..9].copy_from_slice(sig);
    s[9] = flags;
    let n = f3::<i32, *const u8, u32, i32>(F_WRITE)(fd, s.as_ptr(), STATE_LEN as u32);
    close(fd);
    n == STATE_LEN as i32
}

// ---- values -----------------------------------------------------------------

unsafe fn gather(v: &mut face::Vals, need: &[u32; 4]) {
    let u = |s| face::uses_src(need, s);
    let conv = transmute::<usize, extern "C" fn(*mut u8, *const u8, u32, u32)>(TIME_CONV | 1);
    let set = SETTINGS as *const u8;

    let mut t = [0u32; 2];
    if f1::<*mut u32, i32>(TIME_NOW)(t.as_mut_ptr()) == 1 {
        let raw = t[0];
        conv(t.as_mut_ptr() as *mut u8, t.as_ptr() as *const u8, *set.add(7) as u32, 1);
        let mut hms = [0u8; 8];
        conv(hms.as_mut_ptr(), t.as_ptr() as *const u8, CV_HMS, 1);
        let h = i16::from_le_bytes([hms[0], hms[1]]) as i32;
        if (0..24).contains(&h) && hms[2] < 60 && hms[3] < 60 {
            v.set(face::HOUR24, h);
            v.set(face::MINUTE, hms[2] as i32);
            v.set(face::SECOND, hms[3] as i32);
        }
        let mut dt = [0u8; 8];
        conv(dt.as_mut_ptr(), t.as_ptr() as *const u8, CV_DATE, 1);
        let (mo, d, y) = (dt[0] as i32, dt[1] as i32, u16::from_le_bytes([dt[2], dt[3]]) as i32);
        if (1..=12).contains(&mo) && (1..=31).contains(&d) {
            v.set(face::MONTH, mo);
            v.set(face::DAY, d);
            v.set(face::YEAR, y);
            // 1970-01-01 was a Thursday; 0 = Sunday
            v.set(face::WEEKDAY, (face::days_from_civil(y, mo, d) + 4).rem_euclid(7));
        }
        if u(face::SUNRISE) || u(face::SUNSET) {
            let mut pos = [0u32; 4];
            f2::<*mut u32, u32, i32>(POSITION)(pos.as_mut_ptr(), 1);
            let x = f3::<u32, u32, u32, u32>(SUN_TZ)(raw, pos[0], pos[1]);
            let (mut rise, mut sset) = (u32::MAX, u32::MAX);
            let st = transmute::<usize, extern "C" fn(*mut u32, u32, u32, *mut u32, *mut u32)>(SUN_TIMES | 1);
            st(pos.as_mut_ptr(), raw, x, &mut rise, &mut sset);
            if rise < 86_400 && sset < 86_400 {
                v.set(face::SUNRISE, (rise / 60) as i32);
                v.set(face::SUNSET, (sset / 60) as i32);
            }
        }
    }
    v.set(face::IS24H, (*set.add(0x1c) != 1) as i32);

    if u(face::BATTERY) {
        v.set(face::BATTERY, f1::<u32, u8>(BATTERY)(0) as i32);
    }
    if u(face::HR) {
        let mut s = [0u32; 2];
        f2::<*mut u32, u32, i32>(HEART)(s.as_mut_ptr(), 100);
        let bpm = (s[0] >> 16) as u8;
        if bpm != 0xff && bpm != 0 {
            v.set(face::HR, bpm as i32);
        }
    }
    if u(face::STEPS) {
        v.set(face::STEPS, f0::<i32>(STEPS)());
    }
    if u(face::STEP_GOAL) {
        v.set(face::STEP_GOAL, f0::<i32>(STEP_GOAL)());
    }
    if u(face::CALORIES) {
        v.set(face::CALORIES, f0::<i32>(CAL_ACTIVE)().wrapping_add(f0::<i32>(CAL_REST)()));
    }
    if u(face::FLOORS) {
        v.set(face::FLOORS, (f0::<u32>(FLOORS)() & 0xffff) as i32);
    }
    if u(face::INTENSITY) {
        v.set(face::INTENSITY, f0::<i32>(INTENSITY)());
    }
    if u(face::BT) || u(face::NOTIFS) {
        let phone = f0::<i32>(PHONE)() != 0;
        v.set(face::BT, phone as i32);
        if phone && u(face::NOTIFS) {
            // same walk as the stock notification field (FUN_000625cc case 0x11)
            f0::<()>(NOTIF_LOCK)();
            f0::<()>(NOTIF_FIRST)();
            let mut n = 0;
            while f0::<i32>(NOTIF_HAS)() != 0 && n < 1000 {
                n += 1;
                f0::<i32>(NOTIF_NEXT)();
            }
            f0::<()>(NOTIF_UNLOCK)();
            v.set(face::NOTIFS, n);
        }
    }
    if u(S_WX_TEMP) || u(S_WX_HI) || u(S_WX_LO) || u(S_WX_COND) || u(S_WX_PRECIP) {
        weather(v);
    }
}

/// Now playing, as the stock music widget reads it (FUN_100c02f4). The getter
/// FUN_10169d7c takes the music mutex and, when it returns 1, leaves it held
/// with `out` pointing into the live record; FUN_10169e08 clears `out` and
/// unlocks. Returns 0 = no phone source, lock already released. The two must
/// pair exactly: unlocking an unheld mutex asserts. Strings are copied before
/// the unlock because the phone may rewrite them right after.
unsafe fn music(v: &mut face::Vals<'static>, mus: &mut [[u8; MUS_STR]; 3]) {
    let mut out = [0u32; MUS_OUT / 4];
    let o = out.as_mut_ptr() as *mut u8;
    if f1::<*mut u8, u32>(MUS_GET)(o) == 0 {
        return;
    }
    // lock held from here to MUS_RELEASE: no early returns
    let state = *o.add(0x1c);
    if state != 4 {
        let mut lens = [0usize; 3];
        for (i, l) in lens.iter_mut().enumerate() {
            let p = out[i] as *const u8; // +0 title, +4 artist, +8 album
            if !p.is_null() {
                *l = fold_utf8(p, &mut mus[i]);
            }
        }
        let dur_ms = out[3];
        let mut pos_ms = out[5];
        let tick = out[6];
        if state == 1 && tick != 0 {
            let dt = f0::<u32>(TICK_MS)().wrapping_sub(tick);
            // shown in PLAY_STEP_MS steps while playing: face_tick repaints
            // once per step, so every draw shows the same quantised time
            pos_ms = play_pos(pos_ms, out[8], dt);
            pos_ms -= pos_ms % PLAY_STEP_MS;
        }
        if dur_ms != 0 && pos_ms > dur_ms {
            pos_ms = dur_ms;
        }
        // title, artist, album are sources 90, 91, 92
        for (i, &l) in lens.iter().enumerate() {
            if l > 0 {
                v.set_str(face::MUSIC_TITLE + i, core::slice::from_raw_parts(mus[i].as_ptr(), l));
            }
        }
        v.set(face::MUSIC_PLAYING, (state == 1) as i32);
        if dur_ms != 0 {
            v.set(face::MUSIC_DUR, (dur_ms / 1000) as i32);
            v.set(face::MUSIC_POS, (pos_ms / 1000) as i32);
        }
    }
    f1::<*mut u8, ()>(MUS_RELEASE)(o);
}

/// Copy a NUL-terminated UTF-8 string (at most MUS_STR - 1 bytes are read)
/// into ASCII for the face fonts. Returns the length written.
unsafe fn fold_utf8(p: *const u8, dst: &mut [u8; MUS_STR]) -> usize {
    let mut len = 0usize;
    while len < MUS_STR - 1 && *p.add(len) != 0 {
        len += 1;
    }
    fold(core::slice::from_raw_parts(p, len), dst)
}

/// UTF-8 to the ASCII the face fonts have: U+00C0..U+00FF lose their accents
/// (e acute becomes e), other non-ASCII characters and control bytes are
/// dropped. A non-empty input with nothing left becomes "?", so a playing
/// track never looks like "nothing playing". Output is never longer than the
/// input. No panics: every index goes through get().
pub fn fold(src: &[u8], dst: &mut [u8]) -> usize {
    // U+00C0..U+00FF; '*' = no ASCII form (dropped)
    const L1: &[u8; 64] = b"AAAAAAACEEEEIIIIDNOOOOO*OUUUUYTsaaaaaaaceeeeiiiidnooooo*ouuuuyty";
    let mut n = 0usize;
    let mut i = 0usize;
    while let Some(&c) = src.get(i) {
        let mut out = 0u8;
        if c < 0x80 {
            if c >= 0x20 {
                out = c;
            }
            i += 1;
        } else if c == 0xc3 && matches!(src.get(i + 1), Some(&t) if t & 0xc0 == 0x80) {
            let t = src.get(i + 1).copied().unwrap_or(0x80);
            if let Some(&a) = L1.get((t & 0x3f) as usize) {
                if a != b'*' {
                    out = a;
                }
            }
            i += 2;
        } else {
            i += 1; // part of a character the fonts can't draw
        }
        if out != 0 {
            if let Some(d) = dst.get_mut(n) {
                *d = out;
                n += 1;
            }
        }
    }
    if n == 0 && !src.is_empty() {
        if let Some(d) = dst.get_mut(0) {
            *d = b'?';
            n = 1;
        }
    }
    n
}

/// deg C to the watch's unit. Stock rounds negatives toward zero (-3 C shows
/// as -2 in deg F mode's float path); this rounds half away from zero.
fn temp_unit(c: i32, fahrenheit: bool) -> i32 {
    if !fahrenheit {
        return c;
    }
    let m = c * 9 + 160; // 5 * F
    if m >= 0 { (m + 2) / 5 } else { -((2 - m) / 5) }
}

/// FIT weather_status -> facekit WX_COND (CLEAR PARTLY CLOUDY RAIN SNOW STORM FOG WINDY)
fn wx_cond(s: u8) -> Option<i32> {
    Some(match s {
        0 => 0,
        1 => 1,
        2 | 22 => 2,
        3 | 13 | 15 | 16 | 17 => 3,
        4 | 7 | 12 | 18 | 19 | 20 | 21 => 4,
        6 | 14 => 5,
        8 | 11 => 6,
        5 => 7,
        _ => return None,
    })
}

unsafe fn weather(v: &mut face::Vals) {
    let mut b = [0u8; WX_REC];
    if f1::<*mut u8, u8>(WX_GET)(b.as_mut_ptr()) != 0 {
        return;
    }
    let mut unit = 0u8;
    transmute::<usize, extern "C" fn(u32, u32, u32, *mut u8) -> u32>(SETTING_GET | 1)(3, 0, 0x15, &mut unit);
    let f = unit == 1;
    let t = |o: usize| b[o] as i8 as i32;
    if b[WX_TEMP_OFF] != 0x7f {
        v.set(S_WX_TEMP, temp_unit(t(WX_TEMP_OFF), f));
    }
    // high/low come from the daily forecast; both 0 means no forecast arrived
    if b[WX_HI_OFF] != 0x7f && b[WX_LO_OFF] != 0x7f && (b[WX_HI_OFF] | b[WX_LO_OFF]) != 0 {
        v.set(S_WX_HI, temp_unit(t(WX_HI_OFF), f));
        v.set(S_WX_LO, temp_unit(t(WX_LO_OFF), f));
    }
    if let Some(c) = wx_cond(b[WX_COND_OFF]) {
        v.set(S_WX_COND, c);
    }
    if b[WX_PRECIP_OFF] <= 100 && b[WX_COND_OFF] != 0xff {
        v.set(S_WX_PRECIP, b[WX_PRECIP_OFF] as i32);
    }
}

// ---- drawing ----------------------------------------------------------------

struct Gfx {
    dc: usize,
    rect: extern "C" fn(usize, i32, i32, i32, i32),
}
impl Gfx {
    unsafe fn colour(&self, c: u8) {
        let p = self.dc as *mut u8;
        *p.add(0x31) = c;
        *p.add(0x32) = c;
    }
    fn fill(&self, x: i32, y: i32, w: i32, h: i32) {
        if w > 0 && h > 0 {
            (self.rect)(self.dc, x, y, w, h);
        }
    }
}

/// Columns [x0, x1) of row y covered by the inset circle (x0 >= x1: none).
fn inset_span(y: i32) -> (i32, i32) {
    let dy = 2 * y + 1 - INSET_CY2;
    let mut x0 = 128;
    let mut x1 = 128;
    let mut x = 70;
    while x < 128 {
        let dx = 2 * x + 1 - INSET_CX2;
        if dx * dx + dy * dy <= INSET_R2 * INSET_R2 {
            if x0 == 128 {
                x0 = x;
            }
            x1 = x + 1;
        }
        x += 1;
    }
    (x0, x1)
}

/// Copy the canvas to the screen: background everywhere, then one rectangle
/// per run of set pixels, skipping the inset circle.
unsafe fn blit(px: &[u8; face::CANVAS], own: bool) {
    // own: the face draws the inset circle too, so nothing is skipped
    let span = |y: i32| if own || y >= 49 { (128, 128) } else { inset_span(y) };
    let g = Gfx { dc: f0::<usize>(GET_DC)(), rect: transmute(FILL_RECT | 1) };
    // Faces are drawn the way facekit previews them: ink dark, paper light.
    // The face page sets the theme with FUN_0003b408(face cfg[0xb] == 0): in
    // the default face setting fg = 1 (light) and bg = 0 (dark), so ink takes
    // the theme's bg colour (seen on the watch: with ink = fg the dark
    // SOUND came out light). Flipping the watch's own face colour setting
    // still inverts a face.
    let theme = *(THEME_PP as *const *const u8);
    let (fg, bg) = (*theme.add(4), *theme.add(3));
    g.colour(bg);
    g.fill(0, 49, 128, 79);
    for y in 0..49 {
        let (m0, m1) = span(y);
        g.fill(0, y, m0.min(128), 1);
        g.fill(m1, y, 128 - m1, 1);
    }
    g.colour(fg);
    for y in 0..128i32 {
        let (m0, m1) = span(y);
        let row = &px[y as usize * face::STRIDE..][..face::STRIDE];
        let on = |x: i32| row[(x >> 3) as usize] & (0x80 >> (x & 7)) != 0;
        let mut x = 0;
        while x < 128 {
            if !on(x) {
                x += 1;
                continue;
            }
            let s = x;
            while x < 128 && on(x) {
                x += 1;
            }
            // clip [s, x) against the inset span
            if x <= m0 || s >= m1 {
                g.fill(s, y, x - s, 1);
            } else {
                g.fill(s, y, m0 - s, 1);
                g.fill(m1, y, x - m1, 1);
            }
        }
    }
}

/// Called after the stock renderer with its arguments and its caller.
#[no_mangle]
pub unsafe extern "C" fn face_draw(cfg: usize, _vals: usize, caller: usize) {
    if caller >= EDITOR.0 && caller < EDITOR.1 {
        return;
    }
    let drawn = draw_face(*(cfg as *const u8).add(7));
    // remember, in the face page's own state, whether this repaint drew the
    // custom face (and in which display mode), for tick_skip
    let page = (caller >= PAGE_FN.0 && caller < PAGE_FN.1) || (caller >= TICK_FN.0 && caller < TICK_FN.1);
    if page && cfg > CFG_IN_STATE {
        let st = (cfg - CFG_IN_STATE) as *mut u8;
        *st.add(ST_DRAWN) = drawn as u8;
        *st.add(ST_MODE) = *(MODE as *const u8);
    }
}

/// 1 Hz repaint from FUN_00055cd0 (tick_entry): true to skip it. See the
/// repaint gate notes at ST_DRAWN.
#[no_mangle]
pub unsafe extern "C" fn tick_skip(cfg: usize, vals: usize) -> bool {
    if cfg <= CFG_IN_STATE || vals == 0 {
        return false;
    }
    let st = (cfg - CFG_IN_STATE) as *const u8;
    let v = vals as *const u32;
    let mode = *(MODE as *const u8);
    *st.add(ST_DRAWN) == 1
        && mode == 2
        && *st.add(ST_MODE) == 2
        && (v.read_unaligned() & 0x00ff_ffff) == (v.add(1).read_unaligned() & 0x00ff_ffff)
        && static_inset(*(cfg as *const u8).add(7))
}

/// Draws the custom face over the stock one; true when it drew.
unsafe fn draw_face(inset_type: u8) -> bool {
    if !exists(P_FACE) || exists(P_OFF) {
        return false;
    }
    let fd = open(P_FACE, O_READ);
    if fd < 0 {
        return false;
    }
    let mut hdr = [0u8; 16];
    let len = if read_all(fd, &mut hdr) && &hdr[..4] == b"IFC1" {
        u32::from_le_bytes([hdr[12], hdr[13], hdr[14], hdr[15]]) as usize
    } else {
        0
    };
    if !(16..=MAX_FILE).contains(&len) {
        close(fd);
        return false;
    }
    let Some(fbuf) = Heap::new(len) else {
        close(fd);
        return false;
    };
    let file = core::slice::from_raw_parts_mut(fbuf.0, len);
    file[..16].copy_from_slice(&hdr);
    let ok = read_all(fd, &mut file[16..]);
    close(fd);
    if !ok || face::check(file).is_none() {
        return false;
    }
    let Some(wbuf) = Heap::new(size_of::<Work>()) else {
        return false;
    };
    // zero in place (no big temporaries on the stack); zero is a valid state
    // for every field except the string slices, which get empty slices
    let w = wbuf.0 as *mut Work;
    core::ptr::write_bytes(wbuf.0, 0, size_of::<Work>());
    addr_of_mut!((*w).v.strs).write([&[]; 16]);
    let w = &mut *w;

    let need = face::uses(file);
    let uses_music = (face::MUSIC_TITLE..=face::MUSIC_PCT).any(|s| face::uses_src(&need, s));
    let flags = if uses_music { ST_MUSIC } else { 0 };

    // crash marker
    let mut sig = [0u8; 8];
    sig[..4].copy_from_slice(&(len as u32).to_le_bytes());
    sig[4..].copy_from_slice(&fnv(file).to_le_bytes());
    let st = state_read();
    let known = st[1..9] == sig;
    let first = if known {
        match st[0] {
            b'G' => false,
            b'R' => {
                state_write(b'X', &sig, flags);
                return false;
            }
            _ => return false,
        }
    } else {
        if !state_write(b'R', &sig, flags) {
            return false;
        }
        true
    };

    gather(&mut w.v, &need);
    w.v.set(face::INSET_TYPE, inset_type as i32);
    if uses_music {
        music(&mut w.v, &mut w.mus);
    }
    face::derive(&mut w.v);
    let drawn = {
        let mut cv = face::Canvas::new(&mut w.px);
        face::render(file, &w.v, &mut cv, &mut w.sc).is_some()
    };
    if drawn {
        let own = w.sc.own;
        blit(&w.px, own);
        if own {
            // The face drew the inset circle. Inset state (0x20009874, set by
            // the stock renderer that just ran): mode 0x1f is the painter's
            // custom-callback mode, and with a null callback (+0xc) FUN_00057d94
            // draws nothing there (0x5803a..0x58048), so the face shows through.
            // The stock renderer sets the mode again on every draw.
            *(INSET_STATE as *mut u8) = 0x1f;
            *((INSET_STATE + 0xc) as *mut u32) = 0;
        }
    }
    if first {
        state_write(b'G', &sig, flags);
    }
    drawn
}

/// Position in ms after `dt` ms of playing at `rate_bits` (f32 rate, 1.0 =
/// normal). Saturates instead of wrapping for a negative rate.
fn play_pos(elapsed: u32, rate_bits: u32, dt: u32) -> u32 {
    elapsed.wrapping_add((f32::from_bits(rate_bits) * dt as f32) as u32)
}

/// Once per face-page timer event (1 Hz, already scheduled by stock code).
/// Cost when nothing is due: one byte read with no phone music source,
/// otherwise one lock/copy/unlock of the music record. Only when a repaint is
/// due does it touch the file system (OFF and STATE.BIN, to skip faces that
/// show no music or are marked crashed).
unsafe fn face_tick() {
    if *(MUS_ACTIVE_SRC as *const u8) >= 2 {
        return;
    }
    let node = *(CUR_NODE as *const usize);
    if node == 0 {
        return;
    }
    let mut out = [0u32; MUS_OUT / 4];
    let o = out.as_mut_ptr() as *mut u8;
    if f1::<*mut u8, u32>(MUS_GET)(o) == 0 {
        return;
    }
    // lock held: copy what we need, release, then decide
    let (dur, el, tick, rate, state) = (out[3], out[5], out[6], out[8], *o.add(0x1c));
    f1::<*mut u8, ()>(MUS_RELEASE)(o);
    if tick == 0 || state == 4 {
        return;
    }
    let age = f0::<u32>(TICK_MS)().wrapping_sub(tick);
    let due = if age < TICK_PERIOD {
        true // new playback state since the last tick
    } else if state == 1 {
        let before = play_pos(el, rate, age - TICK_PERIOD);
        let now = play_pos(el, rate, age);
        now / PLAY_STEP_MS != before / PLAY_STEP_MS && (dur == 0 || before < dur)
    } else {
        false
    };
    if !due || exists(P_OFF) {
        return;
    }
    let st = state_read();
    if st[0] != b'G' || st[9] & ST_MUSIC == 0 {
        return;
    }
    f2::<usize, u32, ()>(PAGE_POST)(node, EV_PAINT);
}

/// After the stock face page handler; `ev` is the event it was called for.
#[no_mangle]
pub unsafe extern "C" fn page_after(ev: u32) {
    if ev == EV_TICK {
        face_tick();
    }
}

/// Before the stock face page handler. For the events whose stock handler
/// itself uses the page state block (0xd0 page shown again, 0x8a, 0x53, 0x2a),
/// clear the drawn flag so the next 1 Hz tick repaints in full (tick_skip):
/// returning from menus, popups or settings never waits for the minute.
/// Done before the handler so the block is the live one; 0x82 (create) zeroes
/// a new block itself.
///
/// Holding SET (key 0x21, see plane.rs) opens the plane page instead, unless
/// GARMIN/FACES/PLANEOFF exists; returns true when the stock handler should
/// not see the event.
#[no_mangle]
pub unsafe extern "C" fn page_before(ev: u32) -> bool {
    if ev == 0x2a && *((0x2000_8cec + 6) as *const u8) == plane::KEY_HOLD_SET && plane::enabled() {
        plane::open();
        return true;
    }
    if !matches!(ev, 0xd0 | 0x8a | 0x53 | 0x2a) {
        return false;
    }
    let node = *(CUR_NODE as *const usize);
    if node == 0 {
        return false;
    }
    let st = *((node + 0xc) as *const usize);
    if st != 0 {
        *((st + ST_DRAWN) as *mut u8) = 0;
    }
    false
}

// FUN_00062858(cfg, vals) entry, patched to "b.w face_entry" over its first
// instruction (push.w {r4-r11, lr}). Runs the stock renderer, then
// face_draw(cfg, vals, caller) and returns to the caller.
global_asm!(
    ".section .text.face_entry,\"ax\",%progbits",
    ".global face_entry",
    ".type face_entry,%function",
    ".thumb_func",
    "face_entry:",
    "    push  {{r0, r1, r2, lr}}",
    "    bl    orig_62858",
    "    pop   {{r0, r1, r2, lr}}",
    "    mov   r2, lr",
    "    b.w   face_draw",
    ".global orig_62858",
    ".type orig_62858,%function",
    ".thumb_func",
    "orig_62858:",
    "    push.w {{r4, r5, r6, r7, r8, r9, r10, r11, lr}}",
    "    ldr   r12, =0x0006285d",
    "    bx    r12",
    "    .ltorg",
);

// bl FUN_00062858 at 0x55cee (in FUN_00055cd0, the 1 Hz repaint) patched to
// "bl tick_entry". Skips the repaint when tick_skip says nothing changed,
// else continues into face_entry with lr still pointing into FUN_00055cd0.
// pop does not touch the flags set by cmp.
global_asm!(
    ".section .text.face_entry,\"ax\",%progbits",
    ".global tick_entry",
    ".type tick_entry,%function",
    ".thumb_func",
    "tick_entry:",
    "    push  {{r0, r1, r2, lr}}",
    "    bl    tick_skip",
    "    cmp   r0, #0",
    "    pop   {{r0, r1, r2, lr}}",
    "    it    ne",
    "    bxne  lr",
    "    b.w   face_entry",
);

// FUN_000562e0 (face page handler, void, reads the event at 0x20008cec+4)
// entry, patched to "b.w page_entry" over its first instruction
// (push.w {r4-r11, lr}). Saves the event id before the stock handler runs
// (it may reuse the message buffer), calls page_before(ev), runs the handler
// (void, no arguments, reads its globals) unless page_before took the event,
// then page_after(ev).
global_asm!(
    ".section .text.face_entry,\"ax\",%progbits",
    ".global page_entry",
    ".type page_entry,%function",
    ".thumb_func",
    "page_entry:",
    "    push  {{r4, lr}}",
    "    ldr   r4, =0x20008cec",
    "    ldrh  r4, [r4, #4]",
    "    mov   r0, r4",
    "    bl    page_before",
    "    cmp   r0, #0",
    "    bne   1f",
    "    bl    orig_562e0",
    "    mov   r0, r4",
    "    pop   {{r4, lr}}",
    "    b.w   page_after",
    "1:",
    "    pop   {{r4, pc}}",
    ".global orig_562e0",
    ".type orig_562e0,%function",
    ".thumb_func",
    "orig_562e0:",
    "    push.w {{r4, r5, r6, r7, r8, r9, r10, r11, lr}}",
    "    ldr   r12, =0x000562e5",
    "    bx    r12",
    "    .ltorg",
);

// A panic used to spin forever, which on the watch is a frozen screen. Now it
// faults on purpose: the firmware's fault handler logs the crash (PC in this
// payload, r0 = source line, r1 = 0x0badc0de) to GARMIN/Debug and reboots.
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    let line = info.location().map(|l| l.line()).unwrap_or(0);
    unsafe {
        core::arch::asm!("udf #0xfe", in("r0") line, in("r1") 0x0bad_c0de_u32, options(noreturn));
    }
}

// FUN_100ae200(handler) -> page flags, found in a table of every stock page
// (0x101bf408, 0x25f entries; all have bit 0 set). The plane page is not in it
// and got 0, so the dispatcher treated it as a see-through overlay: every
// paint of it repainted the watch face underneath first. Hooked at 0x100ae202
// (after its push {r4}): the plane page gets 0x20001, the flags of the stock
// popup page (0x100f8cf1 / 0x100f8d01) whose open, state and layout calls it
// copies, and of 309 of the 607 stock pages; anything else runs the original
// lookup. 0x1 (130 stock pages) ran without freezing but drew a garbled screen.
global_asm!(
    ".section .text.page_flags_entry,\"ax\",%progbits",
    ".global page_flags_entry",
    ".type page_flags_entry,%function",
    ".thumb_func",
    "page_flags_entry:",
    "    ldr   r2, =plane_page",
    "    cmp   r0, r2",
    "    beq   1f",
    "    ldr   r2, =0x100afbf1",
    "    ldr   r4, =0x101bf408",
    "    movs  r3, #0",
    "    movw  r1, #0x25f",
    "    ldr.w pc, 2f",
    "1:",
    "    ldr   r0, =0x20001",
    "    pop   {{r4}}",
    "    bx    lr",
    ".ltorg",
    ".balign 4",
    "2:",
    "    .word 0x100ae219",
);
