//! Native navigation screen for the Garmin Instinct (006-B3126) firmware 19.10.
//!
//! Gadgetbridge (without the Connect IQ nav app) sends turn-by-turn updates as
//! ordinary notifications: app id "gadgetbridge.nav.v1.<action>", title = distance
//! to the next turn, message = "instruction\nETA ...\nremaining". Every update is
//! sent as delete + add of the same phone UID.
//!
//! Five hooks (addresses in tools/build.sh):
//!   paint  0x100f8496 -> nav_entry   draw the nav screen on the popup's paint event
//!   event  0x000b3080 -> evt_entry   within a live session, hold back deletes of
//!                                    the nav record and turn the following add into
//!                                    a modify, so the record (and its uid) survives
//!                                    every update; a new session (earlier delete
//!                                    never followed by its add, or 61 s quiet) lets
//!                                    the delete through so it starts on a fresh,
//!                                    zeroed record; finish nav deletes held 4..8 s
//!   alert  0x00054b84 -> alert_entry nav popup already on top: repaint in place
//!                                    instead of rebuilding it; user closed it: stay
//!                                    quiet until the next manoeuvre or approach band
//!   tick   0x100f84f2 -> tick_entry  no popup timeout while navigating; finish a
//!                                    held-back delete after 4..8 s (navigation ended)
//!   vibe   0x100f8a36 -> vibe_entry  vibrate only for a new manoeuvre or approach
//!
//! State lives in three padding bytes of the notification record (0x85..0x87,
//! nothing in the firmware reads or writes them; previews end at 0x84) and in
//! byte 0x64 of the popup page state. No writable statics: the linker script
//! rejects .data/.bss. Text logic, timing rules and the whole screen layout are
//! in navcore.rs (drawn through its Draw trait, implemented here on the firmware
//! dc), which the host tests (host/navtest.rs, host/navfuzz.rs) and the mock-up
//! driver (host/navmock.rs) also build.

#![no_std]
#![allow(clippy::missing_safety_doc)]

use core::arch::global_asm;
use core::mem::transmute;
use core::ptr::{read_unaligned, write_unaligned};

#[path = "navcore.rs"]
#[allow(dead_code)]
mod navcore;
use navcore::*;

// ---- firmware entry points (Thumb, called with |1) -------------------------
const GET_DC: usize = 0x0004_79c8; // () -> dc
const FILL_RECT: usize = 0x0006_b358; // (dc, x, y, w, h), colour dc[0x31..0x33]
const FILL_POLY: usize = 0x0006_acf8; // (dc, n, pts:i16[2n], bufsz, buf) -> int
const TEXT_POS: usize = 0x0006_c264; // (dc, x, y)
// FUN_0006c1a0(dc, font, text, maxw, flags, ...): draws `text` as is (UTF-8,
// code page 0xfde9 added by the wrapper); it is not printf, so the text goes in
// r2 itself. maxw 0 = measure the text. flags bits 0-1: 0 left, 1 centre,
// 2 right of x; bits 2-3 background fill; bit 30 takes colours from the
// varargs (we leave it clear). Stock callers pass (dc, font, buf, 0, align, 0).
// The y set by FUN_0006c264 is the baseline: FUN_0006b8ec puts the top at
// y + 1 - ascent (font +0x18); font +0x16 is the cell height.
const DRAW_TEXT: usize = 0x0006_c1a0;
const TEXT_WIDTH: usize = 0x0006_c188; // (font, text, flags, dc) -> px, code page added by the wrapper
const FONT: usize = 0x0004_8310; // (font_id) -> font
const INSET_ICON: usize = 0x0005_7b10; // (flag, icon_id)
const CAT_ICON: usize = 0x1011_b520; // (category, 1) -> icon_id
const THEME_PP: usize = 0x0003_b440; // *THEME_PP -> theme bytes, [3]=fg [4]=bg

const NOTIF_LOCK: usize = 0x000b_2354; // recursive: FUN_1011b248 holds it around callbacks that lock again
const NOTIF_UNLOCK: usize = 0x000b_2360;
const NOTIF_BY_ID: usize = 0x000b_2274; // (internal id) -> record*
const NOTIF_LIST: usize = 0x2000_736c; // -> first node; node[0] next, record at node + 0x10
const NOTIF_BY_PHONE: usize = 0x000b_2298; // (phone uid, source) -> record*
const NOTIF_ACTIVE: usize = 0x000b_208c; // (id) -> record* if state 1..3
const NOTIF_VIEW: usize = 0x000b_2798; // (id, &rec) -> checked-out copy if it is this id, else the record
const COPY_RELEASE: usize = 0x000b_2704; // drop the checked-out copy (and its full text)
const COPY_ACTIVE: usize = 0x2001_f4de; // u8
const COPY: usize = 0x2000_73c8; // 0x88-byte record copy + full-text pointer at +0x88
const NOW_MS: usize = 0x0000_5c48; // () -> ms
const VIBRATE: usize = 0x1018_03c8; // (6, -1): the notification alert
const PAGE_FIND: usize = 0x0004_bd00; // (handler) -> page node in the page list
const PAGE_ON_TOP: usize = 0x0004_bd5c; // (node) -> 1 if it is the top visible page
const PAGE_POST: usize = 0x0004_bf24; // (node, event)
const CUR_PAGE: usize = 0x2000_9214; // *CUR_PAGE = page being dispatched; +0xc = its state
const POPUP_ALERT: usize = 0x100f_8d01; // popup opened by an alert (times out)
const POPUP_LIST: usize = 0x100f_8cf1; // popup opened from the notification list
const ALERTS_ON: usize = 0x2001_f514; // u8, same gates as FUN_00054b84
const ALERTS_DND: usize = 0x2001_f517;
const ALERT_OK: usize = 0x0005_4078; // (0) -> allowed now
const DND_OK: usize = 0x0005_fe08; // () -> allowed despite do-not-disturb

const FONT_MED: u32 = 0x8000; // the stock face's main text font
const FONT_SMALL: u32 = 0x4000;
const EV_PAINT: u32 = 0x5f;

// record padding
const R_FLAGS: usize = 0x85; // bit7 delete held, bit6 vibrate on next popup create, bit5 shown,
                             // bits2-4 event time bits 8-10, bits0-1 band
const R_HASH: usize = 0x86; // manoeuvre hash (action + instruction)
const R_TIME: usize = 0x87; // last nav event (held delete or alert), ms >> 12, low 8 bits

// page state tail
const S_MARK: usize = 0x64; // 'N' once a nav screen was painted on this popup
const MARK: u8 = b'N';

#[inline(always)]
unsafe fn f0<R>(a: usize) -> extern "C" fn() -> R {
    transmute::<usize, extern "C" fn() -> R>(a | 1)
}
#[inline(always)]
unsafe fn lock() {
    f0::<()>(NOTIF_LOCK)()
}
#[inline(always)]
unsafe fn unlock() {
    f0::<()>(NOTIF_UNLOCK)()
}
/// Clock for the record's event time: 4.096 s units, 11 bits.
#[inline(always)]
unsafe fn now11() -> u16 {
    ((f0::<u32>(NOW_MS)() >> 12) as u16) & TIME_MASK
}

unsafe fn rec_time(rec: *const u8) -> (u8, u8) {
    (*rec.add(R_FLAGS), *rec.add(R_TIME))
}

/// Stamp the record with the current time (keeps the other flags).
unsafe fn stamp(rec: *mut u8) {
    let (f, lo) = with_time(*rec.add(R_FLAGS), now11());
    *rec.add(R_FLAGS) = f;
    *rec.add(R_TIME) = lo;
}

struct Gfx {
    dc: usize,
    fg: u8,
    bg: u8,
    fs: Font,
    fm: Font,
}

impl Gfx {
    unsafe fn colour(&self, c: u8) {
        let p = self.dc as *mut u8;
        *p.add(0x31) = c;
        *p.add(0x32) = c;
    }
    fn font(&self, f: u8) -> &Font {
        if f == MED {
            &self.fm
        } else {
            &self.fs
        }
    }
    /// Copy at most 63 bytes (whole characters) into a NUL-terminated buffer.
    fn cstr(s: &[u8], z: &mut [u8; 64]) {
        let n = utf8_cut(s, MEASURE_MAX);
        for (d, &c) in z.iter_mut().zip(s.iter().take(n)) {
            *d = c;
        }
    }
}

impl Draw for Gfx {
    fn rect(&mut self, ink: bool, x: i32, y: i32, w: i32, h: i32) {
        if w <= 0 || h <= 0 {
            return;
        }
        unsafe {
            self.colour(if ink { self.fg } else { self.bg });
            let f: extern "C" fn(usize, i32, i32, i32, i32) = transmute(FILL_RECT | 1);
            f(self.dc, x, y, w, h);
        }
    }
    fn poly(&mut self, ink: bool, pts: &[i16]) {
        unsafe {
            self.colour(if ink { self.fg } else { self.bg });
            // FUN_0006acf8 needs ~14 bytes per vertex (2-byte sort index + 12-byte
            // edge entry) and returns -1 without writing past the end if short.
            let mut buf = [0u8; 256];
            let f: extern "C" fn(usize, i32, *const i16, i32, *mut u8) -> i32 = transmute(FILL_POLY | 1);
            f(self.dc, (pts.len() / 2) as i32, pts.as_ptr(), buf.len() as i32, buf.as_mut_ptr());
        }
    }
    /// The string goes to FUN_0006c1a0 as the text itself: it is drawn as is,
    /// with no format processing (passing "%s" plus an argument drew "%s").
    fn text(&mut self, font: u8, x: i32, top: i32, align: u32, s: &[u8]) {
        let f = self.font(font);
        if s.is_empty() || f.p == 0 {
            return;
        }
        let (fp, asc) = (f.p, f.asc);
        let mut z = [0u8; 64];
        Self::cstr(s, &mut z);
        unsafe {
            let pos: extern "C" fn(usize, i32, i32) = transmute(TEXT_POS | 1);
            let draw: extern "C" fn(usize, usize, *const u8, i32, u32, u32) = transmute(DRAW_TEXT | 1);
            let p = self.dc as *mut u8;
            *p.add(0x26) = 0xff; // transparent background
            *p.add(0x27) = self.fg;
            pos(self.dc, x, top + asc - 1);
            draw(self.dc, fp, z.as_ptr(), 0, align, 0);
        }
    }
    fn width(&mut self, font: u8, s: &[u8]) -> i32 {
        let fp = self.font(font).p;
        if s.is_empty() || fp == 0 {
            return 0;
        }
        let mut z = [0u8; 64];
        Self::cstr(s, &mut z);
        unsafe {
            let w: extern "C" fn(usize, *const u8, u32, usize) -> i32 = transmute(TEXT_WIDTH | 1);
            w(fp, z.as_ptr(), 1, self.dc)
        }
    }
    fn metrics(&mut self, font: u8) -> (i32, i32) {
        let f = self.font(font);
        (f.h, f.asc)
    }
}

/// A firmware font with the metrics the layout needs.
struct Font {
    p: usize,
    h: i32,   // cell height
    asc: i32, // rows from the top to the baseline
}

unsafe fn font(id: u32) -> Font {
    let font_of: extern "C" fn(u32) -> usize = transmute(FONT | 1);
    let p = font_of(id);
    if p == 0 {
        return Font { p: 0, h: 12, asc: 10 };
    }
    let h = read_unaligned((p + 0x16) as *const u16) as i32;
    let asc = read_unaligned((p + 0x18) as *const u16) as i32;
    // sanity: an unexpected layout falls back to a 12 px cell
    if !(4..=48).contains(&h) || !(1..=h).contains(&asc) {
        return Font { p, h: 12, asc: 10 };
    }
    Font { p, h, asc }
}

// ---- text helpers ----------------------------------------------------------

/// Copy a C string into `out`, stopping at NUL, at `lim` bytes if non-zero, or
/// when `out` is full. Returns the byte count.
unsafe fn copy_c(src: *const u8, lim: usize, out: &mut [u8]) -> usize {
    if src.is_null() {
        return 0;
    }
    let mut n = 0;
    while n < out.len() && (lim == 0 || n < lim) {
        let c = *src.add(n);
        if c == 0 {
            break;
        }
        out[n] = c;
        n += 1;
    }
    n
}

unsafe fn is_nav(rec: *const u8) -> bool {
    let mut app = [0u8; 32];
    let n = copy_c(*(rec as *const *const u8), 0, &mut app);
    parse_action(app.get(..n).unwrap_or(&[])).is_some()
}

/// What a nav notification says, read from the checked-out copy (full text) or
/// the record's previews.
struct Nav {
    action: u32,
    cat: u8,
    title: [u8; 40],
    nt: usize,
    msg: [u8; 160],
    nm: usize,
}

impl Nav {
    fn title(&self) -> &[u8] {
        trim_lead(self.title.get(..self.nt).unwrap_or(&[]))
    }
    fn msg(&self) -> &[u8] {
        self.msg.get(..self.nm).unwrap_or(&[])
    }
    fn hash(&self) -> u8 {
        hash(self.action, line(self.msg(), 0))
    }
}

/// Read nav record `id`. Caller holds the lock.
/// Fills `n` in place (it is 200+ bytes; returning it by value kept two
/// copies on the UI task's stack). Returns false if `id` is not a nav record.
unsafe fn read_nav(id: u32, n: &mut Nav) -> bool {
    let view: extern "C" fn(u32, *mut *const u8) -> u32 = transmute(NOTIF_VIEW | 1);
    let mut rec: *const u8 = core::ptr::null();
    view(id, &mut rec);
    if rec.is_null() {
        return false;
    }
    let mut app = [0u8; 32];
    let na = copy_c(*(rec as *const *const u8), 0, &mut app);
    let action = match parse_action(app.get(..na).unwrap_or(&[])) {
        Some(a) => a,
        None => return false,
    };
    n.action = action;
    n.cat = *rec.add(0x18);
    let full = *(rec.add(0x88) as *const *const u8);
    if !full.is_null() {
        // {title*, subtitle*, message*, u16 title len, u16 subtitle len, u16 message len}
        let t = *(full as *const *const u8);
        let m = *(full.add(8) as *const *const u8);
        n.nt = copy_c(t, *(full.add(0xc) as *const u16) as usize, &mut n.title);
        n.nm = copy_c(m, *(full.add(0x10) as *const u16) as usize, &mut n.msg);
    } else {
        n.nt = copy_c(rec.add(0x19), 0x1c, &mut n.title);
        n.nm = copy_c(rec.add(0x5d), 0x28, &mut n.msg);
    }
    true
}

impl Nav {
    fn new() -> Nav {
        Nav { action: 0, cat: 0, title: [0; 40], nt: 0, msg: [0; 160], nm: 0 }
    }
}

/// Decide whether this update deserves a buzz and remember it in the record.
unsafe fn decide(rec: *mut u8, n: &Nav) -> bool {
    let h = n.hash();
    let (vib, f) = navcore::decide(*rec.add(R_FLAGS), *rec.add(R_HASH), h, band(metres(n.title())));
    *rec.add(R_HASH) = h;
    *rec.add(R_FLAGS) = f;
    vib
}

unsafe fn alerts_allowed() -> bool {
    if *(ALERTS_ON as *const u8) == 0 {
        return false;
    }
    let ok: extern "C" fn(u32) -> u32 = transmute(ALERT_OK | 1);
    if ok(0) == 0 {
        return false;
    }
    *(ALERTS_DND as *const u8) == 0 || f0::<u32>(DND_OK)() != 0
}

unsafe fn vibrate() {
    let v: extern "C" fn(i32, i32) = transmute(VIBRATE | 1);
    v(6, -1);
}

/// The notification popup page node if it is on top and showing record `id`.
unsafe fn popup_on_top(id: u32) -> usize {
    let find: extern "C" fn(usize) -> usize = transmute(PAGE_FIND | 1);
    let top: extern "C" fn(usize) -> u32 = transmute(PAGE_ON_TOP | 1);
    for h in [POPUP_ALERT, POPUP_LIST] {
        let node = find(h);
        if node != 0 && top(node) != 0 {
            let st = *((node + 0xc) as *const *const u32);
            if !st.is_null() && *st == id {
                return node;
            }
        }
    }
    0
}

unsafe fn cur_state() -> *mut u8 {
    let page = *(CUR_PAGE as *const usize);
    if page == 0 {
        return core::ptr::null_mut();
    }
    *((page + 0xc) as *const *mut u8)
}

// ---- hooks -----------------------------------------------------------------

extern "C" {
    fn orig_3080(src: u32, pkt: *const u8);
}

/// Source event from the phone: pkt[0] 0 add / 1 modify / 2 remove, pkt[4..8]
/// phone uid. Returns 1 to swallow the event, 0 to let the stock handler run
/// (possibly on a rewritten packet).
#[no_mangle]
pub unsafe extern "C" fn nav_evt(src: u32, pkt: *mut u8) -> u32 {
    let ev = *pkt;
    let uid = read_unaligned(pkt.add(4) as *const u32);
    let by_phone: extern "C" fn(u32, u32) -> *mut u8 = transmute(NOTIF_BY_PHONE | 1);
    lock();
    let mut rec = core::ptr::null_mut();
    if ev == 0 || ev == 2 {
        rec = by_phone(uid, src);
        if !rec.is_null() && !is_nav(rec) {
            rec = core::ptr::null_mut();
        }
    }
    // A delete held while the nav popup was closed has no tick to finish it;
    // any later phone event does it instead.
    let stale = stale_held(rec);
    let mut ret = 0;
    if !rec.is_null() {
        let (f, lo) = rec_time(rec);
        if ev == 2 {
            if hold_delete(f, lo, now11()) {
                // Hold the delete: Gadgetbridge follows it with an add for
                // every update. A delete with no add behind it (navigation
                // stopped) is carried out after HELD_DONE, by nav_tick while
                // the popup is open, else by the next phone event.
                *rec.add(R_FLAGS) = f | F_HELD;
                stamp(rec);
                ret = 1;
            }
            // else a new session (an earlier delete was never followed by its
            // add, or nothing for GAP_FRESH): the delete goes through, and the
            // add behind it creates a new record with zeroed state (the
            // allocator FUN_000b2210 memsets it), so the popup opens and
            // buzzes as for the first update.
        } else if f & F_HELD != 0 {
            // The add that follows a held delete: keep the record and its id, and
            // let the stock code refetch it as a modification.
            *rec.add(R_FLAGS) = f & !F_HELD;
            let id = *(rec.add(4) as *const u32);
            if *(COPY_ACTIVE as *const u8) != 0 && *((COPY + 4) as *const u32) == id {
                // the checked-out copy holds the old full text; drop it so the
                // alert path fetches the new one
                f0::<()>(COPY_RELEASE)();
            }
            *pkt = 1;
        }
    }
    unlock();
    if let Some((u, s)) = stale {
        finish_delete(u, s);
    }
    ret
}

/// First nav record other than `skip` whose delete has been held for
/// HELD_DONE: (phone uid, source). Walks the list the way FUN_000b2274 does
/// (node[0] is the next node, the record is at node + 0x10, the last node is
/// the tail and holds no record). The firmware keeps at most 20; the walk
/// stops at 64. Caller holds the lock.
unsafe fn stale_held(skip: *const u8) -> Option<(u32, u8)> {
    let now = now11();
    let mut node = *(NOTIF_LIST as *const *const usize);
    let mut k = 0;
    while !node.is_null() && k < 64 {
        let next = *node as *const usize;
        if next.is_null() {
            break;
        }
        let rec = (node as *const u8).add(0x10);
        let (f, lo) = rec_time(rec);
        if rec != skip && held_done(f, lo, now) && is_nav(rec) {
            return Some((*(rec.add(0xb8) as *const u32), *rec.add(0xbc)));
        }
        node = next;
        k += 1;
    }
    None
}

/// Carry out a held delete. Caller does not hold the lock.
unsafe fn finish_delete(uid: u32, src: u8) {
    let mut pkt = [0u8; 12];
    pkt[0] = 2;
    write_unaligned(pkt.as_mut_ptr().add(4) as *mut u32, uid);
    orig_3080(src as u32, pkt.as_ptr());
}

/// Alert launcher (FUN_00054b84) entry. Returns 1 when handled here.
#[no_mangle]
pub unsafe extern "C" fn nav_alert(id: u32) -> u32 {
    let active: extern "C" fn(u32) -> *mut u8 = transmute(NOTIF_ACTIVE | 1);
    let node = popup_on_top(id);
    lock();
    let rec = active(id);
    let mut n = Nav::new();
    if rec.is_null() || !read_nav(id, &mut n) {
        unlock();
        return 0;
    }
    let shown_before = *rec.add(R_FLAGS) & F_SHOWN != 0;
    let vib = decide(rec, &n);
    if *rec.add(R_FLAGS) & F_HELD == 0 {
        stamp(rec); // session activity, for the GAP_FRESH rule
    }
    if node == 0 && vib {
        *rec.add(R_FLAGS) |= F_VIBE; // vibe_entry buzzes once the popup exists
    }
    unlock();
    if node != 0 {
        // Already on screen: repaint in place, no rebuild, no stock alert.
        if vib && alerts_allowed() {
            vibrate();
        }
        let post: extern "C" fn(usize, u32) = transmute(PAGE_POST | 1);
        post(node, EV_PAINT);
        return 1;
    }
    if shown_before && !vib {
        // The user closed the nav screen in this session: keep it closed
        // until the next manoeuvre or approach band. A new session starts
        // from a new record (nav_evt), so this never carries over.
        return 1;
    }
    0
}

/// Popup create, alert mode, about to vibrate. Returns 1 to vibrate.
#[no_mangle]
pub unsafe extern "C" fn nav_vibe() -> u32 {
    let st = cur_state();
    if st.is_null() {
        return 1;
    }
    let by_id: extern "C" fn(u32) -> *mut u8 = transmute(NOTIF_BY_ID | 1);
    lock();
    let rec = by_id(*(st as *const u32));
    let mut v = 1;
    if !rec.is_null() && is_nav(rec) {
        let f = *rec.add(R_FLAGS);
        v = (f & F_VIBE != 0) as u32;
        *rec.add(R_FLAGS) = f & !F_VIBE;
    }
    unlock();
    v
}

/// Popup tick. 0: not navigation, stock behaviour. 1: navigating, skip the
/// timeout. 2: navigation ended, run the stock timeout (closes the popup).
#[no_mangle]
pub unsafe extern "C" fn nav_tick() -> u32 {
    let st = cur_state();
    if st.is_null() {
        return 0;
    }
    let by_id: extern "C" fn(u32) -> *mut u8 = transmute(NOTIF_BY_ID | 1);
    lock();
    let rec = by_id(*(st as *const u32));
    if rec.is_null() {
        unlock();
        return if *st.add(S_MARK) == MARK { 2 } else { 0 };
    }
    if !is_nav(rec) {
        unlock();
        return 0;
    }
    let (f, lo) = rec_time(rec);
    if held_done(f, lo, now11()) {
        let uid = *(rec.add(0xb8) as *const u32);
        let src = *rec.add(0xbc);
        unlock();
        finish_delete(uid, src);
        return 1; // the next tick finds the record gone and closes
    }
    unlock();
    1
}

/// Paint event. Returns 1 if it painted the nav screen. Runs only when the
/// firmware repaints the popup (new data posts EV_PAINT, or the popup comes
/// back on top); nothing here sets a timer or polls.
#[no_mangle]
pub unsafe extern "C" fn nav_paint(state: *mut u8) -> u32 {
    let id = *(state as *const u32);
    lock();
    let mut n = Nav::new();
    let ok = read_nav(id, &mut n);
    unlock();
    if !ok {
        return 0;
    }
    *state.add(S_MARK) = MARK;

    let theme = *(THEME_PP as *const *const u8);
    let mut g = Gfx {
        dc: f0::<usize>(GET_DC)(),
        fg: *theme.add(3),
        bg: *theme.add(4),
        fs: font(FONT_SMALL),
        fm: font(FONT_MED),
    };

    // keep the stock inset icon for this notification category
    let cat_icon: extern "C" fn(u32, u32) -> u32 = transmute(CAT_ICON | 1);
    let inset: extern "C" fn(u32, u32) = transmute(INSET_ICON | 1);
    inset(1, cat_icon(n.cat as u32, 1));

    navcore::paint(&mut g, n.action, n.title(), n.msg());
    1
}

// Entry from the 10-byte patch at 0x100f8496 (ldr.w pc,[pc,#4]; nop; .word nav_entry).
// Replays the overwritten "ldr r3,=0x20009214; ldr r3,[r3]; ldr r4,[r3,#0xc];
// movs r1,#1; ldrb r0,[r4,#0xa]" and resumes at 0x100f84a0, or skips to the
// paint epilogue at 0x100f84fc when nav_paint drew the screen. r4 is callee-saved
// so it survives the call.
global_asm!(
    ".section .text.nav_entry,\"ax\",%progbits",
    ".global nav_entry",
    ".type nav_entry,%function",
    ".thumb_func",
    "nav_entry:",
    "    ldr   r3, =0x20009214",
    "    ldr   r3, [r3]",
    "    ldr   r4, [r3, #0xc]",
    "    mov   r0, r4",
    "    bl    nav_paint",
    "    cbnz  r0, 1f",
    "    movs  r1, #1",
    "    ldrb  r0, [r4, #0xa]",
    "    ldr   r12, =0x100f84a1",
    "    bx    r12",
    "1:  ldr   r12, =0x100f84fd",
    "    bx    r12",
    "    .ltorg",
);

// FUN_000b3080(src, pkt) entry, patched to "b.w evt_entry" over its first two
// instructions (push {r4,r5,r6,lr}; adds r3,r1,#4).
global_asm!(
    ".section .text.evt_entry,\"ax\",%progbits",
    ".global evt_entry",
    ".type evt_entry,%function",
    ".thumb_func",
    "evt_entry:",
    "    push  {{r0, r1, r2, lr}}",
    "    bl    nav_evt",
    "    mov   r3, r0",
    "    pop   {{r0, r1, r2, lr}}",
    "    cbnz  r3, 1f",
    "    b     orig_3080",
    "1:  bx    lr",
    ".global orig_3080",
    ".type orig_3080,%function",
    ".thumb_func",
    "orig_3080:",
    "    push  {{r4, r5, r6, lr}}",
    "    adds  r3, r1, #4",
    "    ldr   r12, =0x000b3085",
    "    bx    r12",
    "    .ltorg",
);

// FUN_00054b84(id) entry, patched to "b.w alert_entry" over its first two
// instructions (push {r4,r5,r6,r7,lr}; mov r4,r0).
global_asm!(
    ".section .text.alert_entry,\"ax\",%progbits",
    ".global alert_entry",
    ".type alert_entry,%function",
    ".thumb_func",
    "alert_entry:",
    "    push  {{r0, lr}}",
    "    bl    nav_alert",
    "    mov   r3, r0",
    "    pop   {{r0, lr}}",
    "    cbnz  r3, 1f",
    "    push  {{r4, r5, r6, r7, lr}}",
    "    mov   r4, r0",
    "    ldr   r12, =0x00054b89",
    "    bx    r12",
    "1:  bx    lr",
    "    .ltorg",
);

// Popup tick (event 0xf0) at 0x100f84f2, patched to ldr.w pc,[pc,#4]; nop;
// .word tick_entry. Stock code there: if the popup was opened by an alert
// (*0x2001f524 == 0) run the timeout at 0x100f8756 with r4 = 0, else return
// via 0x100f84fc.
global_asm!(
    ".section .text.tick_entry,\"ax\",%progbits",
    ".global tick_entry",
    ".type tick_entry,%function",
    ".thumb_func",
    "tick_entry:",
    "    bl    nav_tick",
    "    cmp   r0, #1",
    "    beq   2f",
    "    cmp   r0, #2",
    "    beq   1f",
    "    ldr   r3, =0x2001f524",
    "    ldrb  r4, [r3]",
    "    cmp   r4, #0",
    "    bne   2f",
    "1:  movs  r4, #0",
    "    ldr   r12, =0x100f8757",
    "    bx    r12",
    "2:  ldr   r12, =0x100f84fd",
    "    bx    r12",
    "    .ltorg",
);

// Popup create, alert-mode vibration at 0x100f8a36 (mov.w r1,#-1; movs r0,#6;
// bl 0x101803c8; b 0x100f89de), patched to ldr.w pc,[pc,#4]; nop; .word vibe_entry.
global_asm!(
    ".section .text.vibe_entry,\"ax\",%progbits",
    ".global vibe_entry",
    ".type vibe_entry,%function",
    ".thumb_func",
    "vibe_entry:",
    "    bl    nav_vibe",
    "    cbz   r0, 1f",
    "    mov.w r1, #-1",
    "    movs  r0, #6",
    "    ldr   r12, =0x101803c9",
    "    blx   r12",
    "1:  ldr   r12, =0x100f89df",
    "    bx    r12",
    "    .ltorg",
);

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}
