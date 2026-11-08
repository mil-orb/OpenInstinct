//! Plane lookup page. Hold SET (the button the firmware calls BACK) on the
//! watch face while the 12 o'clock points at an aircraft: this page opens,
//! asks api.adsb.lol (through the phone, Gadgetbridge's internet access) for
//! the traffic around the watch's last known position, samples the compass
//! for about a second, and shows the most likely aircraft.
//!
//! Keys on the card: UP/DOWN other candidates, GPS map, SET back to the face,
//! hold SET look again. On the map: UP/DOWN zoom 5/10/25 nm, GPS card.
//!
//! Lifetime and cost: the compass (120 ms sensor rate) and a 250 ms page
//! timer run only while this page is open. The page closes itself after 2 min
//! without a key press. The card does not refetch; the map refetches every
//! 10 s. Repaints happen only when something on screen moved.
//!
//! Routes: while the card shows a flight with an airline callsign, one GET
//! to api.adsbdb.com asks where it's going (once per flight per session).
//! The route shows only if the aircraft is plausibly on it.
//!
//! Threads: the HTTP reply arrives on the phone link task. It is scanned
//! there (integers only) into a heap block (`Net`, or `Rt` for a route) that
//! the page owns jointly with the request; an atomic state word says who
//! frees it.
//!
//! Off switch: a file named PLANEOFF in GARMIN/FACES on the watch drive.

use super::planecore::*;
use super::{exists, f0, f1, f2, FREE, GET_DC, MALLOC, THEME_PP, TICK_MS};
use core::mem::{size_of, transmute};
use core::ptr::{read_unaligned, write_bytes};
use core::sync::atomic::{AtomicU32, Ordering};

// ---- firmware (19.10) ---------------------------------------------------------
const FILL_RECT: usize = 0x0006_b358; // (dc, x, y, w, h), colour dc[0x31..0x33]
const FILL_POLY: usize = 0x0006_acf8; // (dc, n, pts, bufsz, buf) -> int
const TEXT_POS: usize = 0x0006_c264; // (dc, x, y)
const DRAW_TEXT: usize = 0x0006_c1a0; // (dc, font, text, 0, align, 0)
const TEXT_WIDTH: usize = 0x0006_c188; // (font, text, flags, dc) -> px
const FONT: usize = 0x0004_8310; // (id) -> font
const FONT_MED: u32 = 0x8000;
const FONT_SMALL: u32 = 0x4000;

const PAGE_OPEN: usize = 0x0004_c480; // (pos, handler, 0, flags)
const PAGE_CTL: usize = 0x0004_bf4c; // (1, 0, 0, 0xb0, 0) closes the current page
const PAGE_STATE: usize = 0x0004_c4f0; // (node, size, 0, 1) -> state block (node + 0xc)
/// (node, layout or 0 for the default empty one): clears the widget slot
/// flags PAGE_STATE allocated (uninitialised heap) and loads the layout. The
/// stock popup (0x100f85ee) calls it right after PAGE_STATE. Without it the
/// garbage flag byte read as a dirty widget: the dispatcher's after-event step
/// (0x39e6c) posted 0x5f + 0xee to the page forever, and destroy ran widget
/// teardown on garbage (the freezes, err_log errnum 0x34).
const PAGE_LAYOUT: usize = 0x0004_7a44;
/// Offscreen bitmap descriptor; word 0 is the pixel pointer.
const OFFSCREEN: usize = 0x2002_13cc;
const PAGE_TIMER: usize = 0x0004_c664; // (node, ms): 0xf0 every ms while visible
const PAGE_POST: usize = 0x0004_bf24; // (node, event)
const CUR_NODE: usize = 0x2000_9214;
const EVT: usize = 0x2000_8cec; // +4 u16 event id, +6 u8 key code
const EV_CREATE: u16 = 0x82;
const EV_DESTROY: u16 = 0x53;
const EV_PAINT: u16 = 0x5f;
const EV_TICK: u16 = 0xf0;
const EV_KEY: u16 = 0x2a;
const EV_SHOWN: u16 = 0xd0;
pub const KEY_HOLD_SET: u8 = 0x21; // hold BACK, enabled by the 0x5bd46 patch
const KEY_SET: u8 = 0x05;
const KEY_GPS: u8 = 0x04;
const KEY_UP: u8 = 0x0d;
const KEY_DOWN: u8 = 0x08;
/// BACK let go after a hold: the button table 0x5c258 gives each button a
/// short, a down and a release code (BACK 0x05, 0x45, 0x85); once the hold
/// code went out, the release posts only 0x85 (0x5bab8), no short press.
const KEY_REL_SET: u8 = 0x85;
/// BACK's button entry (table 0x2000a60c, 8 bytes each, BACK index 2) +3:
/// set when the hold code is sent (0x5bb54), cleared on release (0x5bad6).
const BACK_HELD: usize = 0x2000_a61f;
/// Inset painter state (FUN_00057d94): mode 0x1f with a null callback draws
/// nothing (0x5803a), so the page's own pixels show in the porthole.
const INSET_STATE: usize = 0x2000_9874;

const COMPASS_REG: usize = 0x000e_cc4c; // (period ms) -> slot 0..9, or -1
const COMPASS_GET: usize = 0x000e_cd0c; // (&f32 rad, true north by default) -> valid
const COMPASS_REL: usize = 0x000e_ce8c; // (slot)
const POSITION: usize = 0x000d_39f0; // (&pos, 1): last known, semicircles
const PHONE: usize = 0x000a_ee78; // () -> phone connected
/// Asks the phone where it is (FUN_000c9534, which the altimeter auto-cal
/// sends on connect, at most daily): a protobuf GetLocationRequest.
/// Gadgetbridge answers with Android's last known fix and FUN_000c9274
/// stores it for PHONE_POS.
const LOC_ASK: usize = 0x000c_9534; // ()
/// Protobuf link handle, 0xffff with none; the firmware's own caller checks
/// it before LOC_ASK (0xc957e).
const PB_LINK: usize = 0x2002_2678;
const PHONE_POS: usize = 0x000d_36d4; // (&PhonePos): copy of the last phone fix
const GARMIN_NOW: usize = 0x000e_e9e0; // (&u32): Garmin time, s
const HTTP_INIT: usize = 0x1017_bfd0; // (req[0x80]): zero + defaults, GET
const HTTP_SEND: usize = 0x101a_fc24; // (req) -> request id, -1 when no phone HTTP
const REQ_URL: usize = 0x04;
const REQ_CB: usize = 0x68; // cb(id, status 0 ok, body, len, http code, ctx)
const REQ_CTX: usize = 0x70;

pub const P_PLANE_OFF: &[u8] = b"0:/GARMIN/FACES/PLANEOFF\0";

// ---- tuning -------------------------------------------------------------------
const TICK_MS_PAGE: u32 = 250;
const AIM_FROM: u32 = 300; // ms after the aim starts: compass filter settled
const QUICK_REL: u32 = 500; // let go this soon after the screen opened: wait, no scan
const HOLD_MAX: u32 = 30_000; // still held after this: look up anyway
const WIN: u32 = 1200; // heading samples up to this long before the release...
const WIN_SKIP: u32 = 120; // ...but not the last moment (the thumb lifting)
const NSMP: usize = 8; // ring of heading samples, 4 a second
const LOOKUP_TIMEOUT: u32 = 20_000;
const MAP_REFRESH: u32 = 15_000; // after the last reply
const IDLE_CLOSE: u32 = 120_000;
const LOOKUP_NM: u32 = 25;
/// Busy sky near a hub: when the 25 nm reply fails or can't be read, one
/// retry at this radius (about a sixth of the area)
const RETRY_NM: u32 = 10;
/// The phone's fix beats the watch's last one (often from the last activity,
/// hours or days back) when it is at most this old, s, and this good, m.
const PHONE_AGE_S: i32 = 1800;
const PHONE_ACC_M: f32 = 1000.0;
/// Ask the phone again at most this often, ms
const LOC_EVERY: u32 = 30_000;

// screens
const S_AIM: u8 = 0;
const S_CARD: u8 = 1;
const S_MAP: u8 = 2;
const S_ERR: u8 = 3;
// errors
const E_POS: u8 = 1;
const E_PHONE: u8 = 2;
const E_HTTP: u8 = 3;
const E_TIMEOUT: u8 = 4;
const E_CODE: u8 = 5;
const E_EMPTY: u8 = 6;
const E_NONE_THERE: u8 = 7;
const E_MEM: u8 = 8;
// flags
const F_AIM_DONE: u8 = 1;
const F_HEAD: u8 = 2;
const F_DATA: u8 = 4;
const F_LIVE: u8 = 8;
const F_DIRTY: u8 = 16; // repaint at the next tick
const F_HELD: u8 = 32; // BACK held: aiming, sampling the compass
const F_SEEN: u8 = 64; // BACK_HELD was seen set during this hold
const F_PHONE_POS: u8 = 128; // st.own is the phone's fix

// ---- shared request block -------------------------------------------------------
const NET_MAGIC: u32 = 0x504c_4e31; // "PLN1"
const N_IDLE: u32 = 0;
const N_PENDING: u32 = 1;
const N_DONE: u32 = 2;
const N_ORPHAN: u32 = 3; // page gone while pending: the callback frees it

const RT_MAGIC: u32 = 0x504c_5254; // "PLRT"

/// Start of both request blocks: the callback only needs this much to know
/// which one it has and who frees it.
#[repr(C)]
struct Hdr {
    magic: u32,
    state: AtomicU32,
    t_done: u32,
    code: i32, // HTTP code, or -1 transport error, -2 unreadable reply
}

/// Traffic around the watch.
#[repr(C)]
struct Net {
    magic: u32,
    state: AtomicU32,
    t_done: u32,
    code: i32,
    n: u32,
    url: [u8; 80],
    ac: [Ac; MAX_AC],
}

/// One flight's route, from api.adsbdb.com.
#[repr(C)]
struct Rt {
    magic: u32,
    state: AtomicU32,
    t_done: u32,
    code: i32,
    hex: u32,
    got: u32,
    url: [u8; 48],
    route: Route,
}

/// Reply callback, on the phone link task.
extern "C" fn http_cb(_id: i32, status: i32, body: *const u8, len: u32, code: i32, ctx: usize) {
    unsafe {
        if ctx < 0x2000_0000 || ctx >= 0x2004_0000 - size_of::<Rt>() || ctx & 3 != 0 {
            return;
        }
        let h = &mut *(ctx as *mut Hdr);
        let big = h.magic == NET_MAGIC;
        if !big && h.magic != RT_MAGIC || big && ctx >= 0x2004_0000 - size_of::<Net>() {
            return;
        }
        // one load: two would leave a window where destroy orphans it
        // between them and nobody frees it
        match h.state.load(Ordering::Acquire) {
            N_ORPHAN => {
                free_req(ctx);
                return;
            }
            N_PENDING => {}
            _ => return,
        }
        // a failed status with a 2xx code (a reply too big for the phone
        // link) must not read as an empty sky
        h.code = if status != 0 { if code > 0 && !(200..300).contains(&code) { code } else { -1 } } else { code };
        let b = if status == 0 && !body.is_null() { core::slice::from_raw_parts(body, len.min(0xffff) as usize) } else { &[] };
        if big {
            let net = &mut *(ctx as *mut Net);
            net.n = 0;
            if status == 0 {
                match scan(b, &mut net.ac) {
                    Some(n) => net.n = n as u32,
                    None => net.code = -2,
                }
            }
        } else {
            let rt = &mut *(ctx as *mut Rt);
            rt.got = 0;
            if let Some(r) = parse_route(b) {
                rt.route = r;
                rt.got = 1;
            }
        }
        h.t_done = f0::<u32>(TICK_MS)();
        if h.state.compare_exchange(N_PENDING, N_DONE, Ordering::AcqRel, Ordering::Acquire).is_err() {
            // the page let go while we were scanning
            free_req(ctx);
        }
    }
}

unsafe fn free_req(p: usize) {
    *(p as *mut u32) = 0;
    f1::<*mut u8, ()>(FREE)(p as *mut u8);
}

/// The page lets go of a request block: a pending one goes to its callback,
/// anything else is freed now.
unsafe fn drop_req(p: usize) {
    if p != 0 && (*(p as *const Hdr)).state.compare_exchange(N_PENDING, N_ORPHAN, Ordering::AcqRel, Ordering::Acquire).is_err() {
        free_req(p);
    }
}

// ---- page state ---------------------------------------------------------------------
const ST_MAGIC: u32 = 0x504c_4e53; // "PLNS"

#[repr(C)]
struct St {
    magic: u32,
    // tripwire: events in the current 1 s window, and the last event id
    tw_t: u32,
    tw_n: u32,
    last_ev: u32,
    net: *mut Net,
    rt: *mut Rt,
    t_rt: u32,
    t_open: u32,
    t_key: u32,
    t_fetch: u32,
    t_data: u32,
    t_paint: u32,
    t_aim: u32,
    t_loc: u32, // last LOC_ASK
    smp: [f32; NSMP],
    smp_t: [u32; NSMP],
    ns: u32,
    head: f32,
    spread: f32,
    live: f32,
    drawn_live: f32,
    own: Own,
    slot: i32,
    sel_hex: u32,
    code: i32,
    screen: u8,
    err: u8,
    zoom: u8,
    sel: u8,
    n_ac: u8,
    n_cand: u8,
    flags: u8,
    phase: u8,
    pending_lookup: u8,
    /// lookup radius, nm: LOOKUP_NM, or RETRY_NM after a reply too big to read
    nm: u8,
    rt_next: u8, // cache slot the next route goes in
    /// routes asked for this session, by aircraft: one request per flight
    routes: [Seen; ROUTES],
    ac: [Ac; MAX_AC],
    cand: [Cand; MAX_AC],
    blips: [Blip; MAX_AC],
    /// FILL_POLY scratch and C strings for the text calls, here and not on
    /// the UI task stack
    scratch: [u8; SCRATCH],
}

/// FILL_POLY needs about 14 bytes per vertex; shapes have 10 at most
const SCRATCH: usize = 160;

const ROUTES: usize = 6;
const R_NONE: u8 = 0; // adsbdb doesn't know the callsign
const R_OK: u8 = 1;
const R_FAR: u8 = 2; // known, but the aircraft isn't on it (another leg, or stale)
const R_ERR: u8 = 3; // no answer: phone, firewall, timeout or an HTTP error

#[derive(Clone, Copy)]
#[repr(C)]
struct Seen {
    hex: u32, // 0: empty slot
    got: u8,
    _pad: [u8; 3],
    route: Route,
}

/// What the route cache says for this aircraft: None if never asked.
fn route_of(st: &St, hex: u32) -> Option<&Seen> {
    st.routes.iter().find(|s| s.hex == hex && hex != 0)
}

/// On the card: take a finished route reply, then ask for the selected
/// aircraft's route if it has an airline callsign and wasn't asked yet.
/// One small request per flight shown, none while another is in flight.
/// True when the card should repaint.
#[inline(never)]
unsafe fn route_tick(st: &mut St, a: &Ac) -> bool {
    let t = now();
    let mut dirty = false;
    if !st.rt.is_null() {
        let rt = &mut *st.rt;
        let s = rt.state.load(Ordering::Acquire);
        let late = s == N_PENDING && t.wrapping_sub(st.t_rt) > LOOKUP_TIMEOUT;
        if s == N_DONE || late {
            let mut seen = Seen { hex: rt.hex, got: R_ERR, _pad: [0; 3], route: rt.route };
            let ok = s == N_DONE && rt.code >= 200 && rt.code < 300;
            if ok && rt.got != 0 {
                // judged against where the aircraft is now
                seen.got = R_FAR;
                if let Some(p) = st.ac[..st.n_ac as usize].iter().find(|p| p.hex == rt.hex) {
                    if route_fits(&rt.route, p.lat, p.lon) {
                        seen.got = R_OK;
                    }
                }
            } else if ok || s == N_DONE && rt.code == 404 {
                seen.got = R_NONE;
            }
            let k = st.rt_next as usize % ROUTES;
            st.routes[k] = seen;
            st.rt_next = st.rt_next.wrapping_add(1);
            if late {
                drop_req(st.rt as usize);
                st.rt = core::ptr::null_mut();
            } else {
                rt.state.store(N_IDLE, Ordering::Release);
            }
            dirty = true;
        }
    }
    if !airline_callsign(&a.flight) || route_of(st, a.hex).is_some() {
        return dirty;
    }
    if st.rt.is_null() {
        let p = f1::<u32, *mut u8>(MALLOC)(size_of::<Rt>() as u32);
        if p.is_null() {
            return dirty;
        }
        write_bytes(p, 0, size_of::<Rt>());
        st.rt = p as *mut Rt;
        (*st.rt).magic = RT_MAGIC;
    }
    let rt = &mut *st.rt;
    if rt.state.load(Ordering::Acquire) != N_IDLE {
        return dirty;
    }
    const URL: &[u8] = b"https://api.adsbdb.com/v0/callsign/";
    rt.url[..URL.len()].copy_from_slice(URL);
    let n = flen(&a.flight);
    rt.url[URL.len()..URL.len() + n].copy_from_slice(&a.flight[..n]);
    rt.url[URL.len() + n] = 0;
    rt.hex = a.hex;
    st.t_rt = t;
    if !send(rt.url.as_ptr(), st.rt as usize) {
        rt.state.store(N_DONE, Ordering::Release); // recorded as no route next tick
        rt.code = -1;
    }
    dirty
}

/// GET `url` (NUL-terminated) through the phone; the reply goes to http_cb
/// with `blk`, a request block whose header is marked pending here. False
/// when the phone has no HTTP (the block is left pending: the caller resets
/// it).
#[inline(never)]
unsafe fn send(url: *const u8, blk: usize) -> bool {
    let mut req = [0u32; 32];
    let r = req.as_mut_ptr() as *mut u8;
    f1::<*mut u8, ()>(HTTP_INIT)(r);
    *(r.add(REQ_URL) as *mut usize) = url as usize;
    *(r.add(REQ_CB) as *mut usize) = http_cb as *const () as usize | 1;
    *(r.add(REQ_CTX) as *mut usize) = blk;
    (*(blk as *const Hdr)).state.store(N_PENDING, Ordering::Release);
    f1::<*mut u8, i32>(HTTP_SEND)(r) >= 0
}

unsafe fn now() -> u32 {
    f0::<u32>(TICK_MS)()
}

unsafe fn node() -> usize {
    *(CUR_NODE as *const usize)
}

unsafe fn state() -> Option<&'static mut St> {
    let n = node();
    if n == 0 {
        return None;
    }
    let st = *((n + 0xc) as *const usize);
    if st == 0 {
        return None;
    }
    let st = &mut *(st as *mut St);
    if st.magic != ST_MAGIC {
        return None;
    }
    Some(st)
}

unsafe fn repaint() {
    let n = node();
    if n != 0 {
        f2::<usize, u32, ()>(PAGE_POST)(n, EV_PAINT as u32);
    }
}

unsafe fn close_page() {
    let ctl: extern "C" fn(i32, u32, u32, u32, u32) = transmute(PAGE_CTL | 1);
    ctl(1, 0, 0, 0xb0, 0);
}

/// Dispatcher byte: 1 while a page handler runs; a handler that opened
/// another page sets 2 so the dispatcher skips its after-event work for the
/// page being replaced.
const HANDOFF: usize = 0x2001_f4f4;

/// Opens the plane page from the face's key handler, the way the face's GPS
/// key opens the activity list (0x56264 then 0x56370): position -4 (switch
/// to the page if it is already open, else open it), op 0x10, then mark the
/// hand-off. Position -1 without the mark left the dispatcher running its
/// widget pass on the face while it was replaced: a hang, then a watchdog
/// reset (err_log errnum 0x34, stack through 0x4b8ad and 0x47d55).
pub unsafe fn open() {
    let open: extern "C" fn(i32, usize, u32, u32) = transmute(PAGE_OPEN | 1);
    open(-4, plane_page as *const () as usize | 1, 0, 0x10);
    *(HANDOFF as *mut u8) = 2;
}

pub unsafe fn enabled() -> bool {
    !exists(P_PLANE_OFF)
}

// ---- requests ---------------------------------------------------------------------

/// Append `v` (1e-6 deg) with 4 decimals.
fn put_deg(u: &mut [u8; 80], n: &mut usize, v: i32) {
    let mut push = |c: u8| {
        if *n < 79 {
            u[*n] = c;
            *n += 1;
        }
    };
    if v < 0 {
        push(b'-');
    }
    let a = (v.unsigned_abs() + 50) / 100; // 1e-4 deg
    let (int, frac) = (a / 10_000, a % 10_000);
    let mut d = [0u8; 4];
    let mut k = 0;
    let mut x = int;
    loop {
        d[k] = b'0' + (x % 10) as u8;
        x /= 10;
        k += 1;
        if x == 0 || k == 4 {
            break;
        }
    }
    while k > 0 {
        k -= 1;
        push(d[k]);
    }
    push(b'.');
    push(b'0' + (frac / 1000) as u8);
    push(b'0' + (frac / 100 % 10) as u8);
    push(b'0' + (frac / 10 % 10) as u8);
    push(b'0' + (frac % 10) as u8);
}

/// Start a fetch of the traffic within `nm` of the watch. False when one is
/// already running or the phone has no HTTP.
unsafe fn fetch(st: &mut St, nm: u32) -> Result<(), u8> {
    if st.net.is_null() {
        let p = f1::<u32, *mut u8>(MALLOC)(size_of::<Net>() as u32);
        if p.is_null() {
            return Err(E_MEM);
        }
        write_bytes(p, 0, size_of::<Net>());
        let net = p as *mut Net;
        (*net).magic = NET_MAGIC;
        st.net = net;
    }
    if (*st.net).state.load(Ordering::Acquire) == N_PENDING && now().wrapping_sub(st.t_fetch) > LOOKUP_TIMEOUT {
        // lost on the way: hand it to its callback (if one ever comes) and
        // start a fresh block
        if (*st.net).state.compare_exchange(N_PENDING, N_ORPHAN, Ordering::AcqRel, Ordering::Acquire).is_ok() {
            st.net = core::ptr::null_mut();
            return fetch(st, nm);
        }
    }
    let net = &mut *st.net;
    if net.state.load(Ordering::Acquire) != N_IDLE {
        return Ok(()); // the one in flight will do
    }
    let mut n = 0;
    for &c in b"https://api.adsb.lol/v2/point/" {
        net.url[n] = c;
        n += 1;
    }
    put_deg(&mut net.url, &mut n, st.own.lat);
    net.url[n] = b'/';
    n += 1;
    put_deg(&mut net.url, &mut n, st.own.lon);
    net.url[n] = b'/';
    n += 1;
    let nm = nm.min(99);
    if nm >= 10 {
        net.url[n] = b'0' + (nm / 10) as u8;
        n += 1;
    }
    net.url[n] = b'0' + (nm % 10) as u8;
    net.url[n + 1] = 0;

    if !send(net.url.as_ptr(), st.net as usize) {
        net.state.store(N_IDLE, Ordering::Release);
        return Err(E_HTTP);
    }
    st.t_fetch = now();
    Ok(())
}

/// Take a finished reply, if there is one. Some(true) when new data arrived.
unsafe fn take(st: &mut St) -> Option<bool> {
    if st.net.is_null() {
        return None;
    }
    let net = &mut *st.net;
    if net.state.load(Ordering::Acquire) != N_DONE {
        return None;
    }
    st.code = net.code;
    let ok = net.code >= 200 && net.code < 300;
    if ok {
        let n = (net.n as usize).min(MAX_AC);
        st.ac[..n].copy_from_slice(&net.ac[..n]);
        st.n_ac = n as u8;
        // the server's snapshot is taken soon after the send; a slow BLE
        // reply arrives seconds later, so anchor between the two
        let span = net.t_done.wrapping_sub(st.t_fetch);
        st.t_data = if span < 60_000 { st.t_fetch.wrapping_add((span / 2).min(1500)) } else { net.t_done.wrapping_sub(1000) };
        st.flags |= F_DATA;
    }
    net.state.store(N_IDLE, Ordering::Release);
    Some(ok)
}

// ---- compass ------------------------------------------------------------------------

unsafe fn heading_now() -> Option<f32> {
    let mut h = 0f32;
    let ok = f1::<*mut f32, u8>(COMPASS_GET)(&mut h);
    if ok != 0 && h.is_finite() && abs(h) < 7.0 {
        Some(h)
    } else {
        None
    }
}

// ---- logic --------------------------------------------------------------------------

/// Seconds from the reply to watch time `t`.
fn since_data(st: &St, t: u32) -> f32 {
    t.wrapping_sub(st.t_data) as i32 as f32 * 0.001
}

/// Rank the current list for the aim, keeping the selected aircraft if it is
/// still there.
fn rerank(st: &mut St) {
    let dt = since_data(st, st.t_aim);
    let head = if st.flags & F_HEAD != 0 { Some(st.head) } else { None };
    let n = (st.n_ac as usize).min(MAX_AC);
    let (c, _) = rank(&st.ac[..n], &st.own, dt, head, st.spread, &mut st.cand);
    st.n_cand = c as u8;
    st.sel = 0;
    if st.sel_hex != 0 {
        for (i, k) in st.cand[..c].iter().enumerate() {
            if st.ac[k.idx as usize].hex == st.sel_hex {
                st.sel = i as u8;
            }
        }
    }
}

fn select(st: &mut St, i: usize) {
    st.sel = i as u8;
    st.sel_hex = match st.cand.get(i) {
        Some(c) if i < st.n_cand as usize => st.ac[c.idx as usize].hex,
        _ => 0,
    };
}

/// Hold BACK: aim. Samples the compass until BACK is let go.
unsafe fn begin_aim(st: &mut St) {
    let t = now();
    st.t_open = t;
    st.t_key = t;
    st.ns = 0;
    st.flags &= !(F_AIM_DONE | F_HEAD | F_SEEN);
    st.flags |= F_HELD | F_DIRTY;
    st.sel_hex = 0;
    st.screen = S_AIM;
    st.err = 0;
    st.pending_lookup = 0;
    st.nm = LOOKUP_NM as u8;
    if st.slot < 0 {
        st.slot = f1::<u32, i32>(COMPASS_REG)(120);
    }
    // the reply lands in a second or so, well before the release
    ask_phone_pos(st);
}

/// BACK let go. Straight after the screen opened it only waits (the user
/// let go to aim with a second hold); otherwise the heading is the mean of
/// the samples from the second before the release, and the lookup starts.
unsafe fn release(st: &mut St) {
    if st.screen != S_AIM || st.flags & F_HELD == 0 {
        return;
    }
    st.flags &= !F_HELD;
    st.flags |= F_DIRTY;
    let t = now();
    st.t_key = t;
    if t.wrapping_sub(st.t_open) < QUICK_REL {
        return;
    }
    if f0::<u8>(PHONE)() == 0 {
        fail(st, E_PHONE);
        return;
    }
    // the position can have moved since the screen opened
    if !read_pos(st) {
        fail(st, E_POS);
        return;
    }
    let (mut ss, mut sc, mut n) = (0.0f32, 0.0f32, 0u32);
    for k in 0..(st.ns as usize).min(NSMP) {
        let age = t.wrapping_sub(st.smp_t[k]);
        if (WIN_SKIP..=WIN).contains(&age) {
            ss += sin(st.smp[k]);
            sc += cos(st.smp[k]);
            n += 1;
        }
    }
    st.t_aim = t.wrapping_sub((WIN + WIN_SKIP) / 2);
    if n == 0 {
        // let go before a sample landed in the window: the heading now
        if let Some(h) = if st.slot >= 0 { heading_now() } else { None } {
            ss = sin(h);
            sc = cos(h);
            n = 1;
            st.t_aim = t;
        }
    }
    if let Some((h, sp)) = circ_mean(ss, sc, n) {
        st.head = h;
        st.spread = sp;
        st.flags |= F_HEAD;
    }
    st.flags |= F_AIM_DONE;
    st.pending_lookup = 1;
    if let Err(e) = fetch(st, st.nm as u32) {
        fail(st, e);
    }
}

fn fail(st: &mut St, e: u8) {
    st.screen = S_ERR;
    st.err = e;
    st.pending_lookup = 0;
    st.flags &= !F_HELD;
    // an error screen needs no compass (battery); hold BACK registers again
    if st.slot >= 0 {
        unsafe { f1::<i32, ()>(COMPASS_REL)(st.slot) };
        st.slot = -1;
    }
}

/// The phone's last fix as the firmware keeps it (0x20017a5c, filled by
/// FUN_000c9274 from GetLocationResponse.location_data).
#[repr(C)]
struct PhonePos {
    lat: i32, // semicircles
    lon: i32,
    alt: f32,
    time: u32, // Garmin time of the fix, s
    acc: f32,  // horizontal, m; -1 when not sent
    vacc: f32,
    valid: u8, // 1 when the reply had a timestamp
    _pad: [u8; 3],
}
// FUN_000d36d4 copies exactly 0x1c bytes
const _: () = assert!(size_of::<PhonePos>() == 0x1c);

fn semi_ok(lat: i32, lon: i32) -> bool {
    lat != 0x7fff_ffff && lon != 0x7fff_ffff && !(lat == 0 && lon == 0) && lat.unsigned_abs() <= 0x4000_0000
}

/// Ask the phone where it is, at most every LOC_EVERY. One small BLE
/// message; the phone answers from Android's cache without starting its GPS.
unsafe fn ask_phone_pos(st: &mut St) {
    let t = now();
    if (st.t_loc != 0 && t.wrapping_sub(st.t_loc) < LOC_EVERY) || f0::<u8>(PHONE)() == 0 || *(PB_LINK as *const u16) == 0xffff {
        return;
    }
    st.t_loc = t | 1;
    f0::<()>(LOC_ASK)();
}

/// Where the user is, into st.own: the phone's fix when it is recent and
/// good enough, else the watch's last GPS fix. False when there is neither.
unsafe fn read_pos(st: &mut St) -> bool {
    let mut p: PhonePos = core::mem::zeroed();
    f1::<*mut PhonePos, ()>(PHONE_POS)(&mut p);
    let mut t = 0u32;
    f1::<*mut u32, ()>(GARMIN_NOW)(&mut t);
    // a static location from Gadgetbridge's settings has no real fix time,
    // so it fails the age test
    let age = t.wrapping_sub(p.time) as i32;
    if p.valid == 1 && (-60..=PHONE_AGE_S).contains(&age) && p.acc >= 0.0 && p.acc <= PHONE_ACC_M && semi_ok(p.lat, p.lon) {
        st.own = Own::from_semi(p.lat, p.lon);
        st.own.unc = p.acc;
        st.flags |= F_PHONE_POS;
        return true;
    }
    let mut pos = [0i32; 4];
    f2::<*mut i32, u32, i32>(POSITION)(pos.as_mut_ptr(), 1);
    if !semi_ok(pos[0], pos[1]) {
        return false;
    }
    st.own = Own::from_semi(pos[0], pos[1]);
    st.flags &= !F_PHONE_POS;
    true
}

// separate frames: only one handler's locals are on the stack at a time
#[inline(never)]
unsafe fn create() {
    let n = node();
    if n == 0 {
        return;
    }
    let alloc: extern "C" fn(usize, u32, u32, u32) -> *mut u8 = transmute(PAGE_STATE | 1);
    let p = alloc(n, size_of::<St>() as u32, 0, 1);
    if p.is_null() {
        return;
    }
    f2::<usize, u32, ()>(PAGE_LAYOUT)(n, 0);
    // The default layout is one type-7 canvas, slot 0, 128 x 128 (0x4774c).
    // The paint pass copies the 1-bit bitmap in the slot's parameter
    // (node+0x14) to the screen, but the layout loader (0x4775c) fills only
    // the descriptor and flag, and nothing here sends the canvas its create.
    // The parameter stayed uninitialised and the screen showed a RAM dump
    // (seen on the watch). Do what the canvas create does for a full-screen
    // canvas (0x100a55fc..0x100a561c): point it at the offscreen buffer we
    // draw into, which the drawing context uses outside the paint pass, and
    // set the slot flag to 3.
    if *((n + 0x35) as *const u8) >= 1 {
        **((n + 0x14) as *const *mut usize) = *(OFFSCREEN as *const usize);
        **((n + 0x18) as *const *mut u8) = 3;
    }
    write_bytes(p, 0, size_of::<St>());
    let st = &mut *(p as *mut St);
    st.magic = ST_MAGIC;
    st.zoom = 1;
    st.slot = f1::<u32, i32>(COMPASS_REG)(120);

    f2::<usize, u32, ()>(PAGE_TIMER)(n, TICK_MS_PAGE);
    // no position yet is fine: the aim asks the phone for one, and the
    // release checks
    if f0::<u8>(PHONE)() == 0 {
        fail(st, E_PHONE);
    } else {
        begin_aim(st);
    }
    st.t_key = now();
    st.flags |= F_DIRTY;
}

// separate frames: only one handler's locals are on the stack at a time
#[inline(never)]
unsafe fn destroy() {
    let Some(st) = state() else { return };
    if st.slot >= 0 {
        f1::<i32, ()>(COMPASS_REL)(st.slot);
        st.slot = -1;
    }
    drop_req(st.net as usize);
    drop_req(st.rt as usize);
    st.net = core::ptr::null_mut();
    st.rt = core::ptr::null_mut();
    st.magic = 0;
}

/// After the aim window and the reply are both in: rank and show.
fn finish_lookup(st: &mut St) {
    st.pending_lookup = 0;
    // a phone fix that came in after the release still counts for ranking
    unsafe { read_pos(st) };
    rerank(st);
    if st.n_ac == 0 || !st.ac[..st.n_ac as usize].iter().any(|a| a.airborne()) {
        fail(st, E_EMPTY);
    } else if st.n_cand == 0 && st.flags & F_HEAD != 0 {
        // nothing within 60 deg of the aim (compass off, magnetic north,
        // a car nearby): show the nearest, labelled as such
        st.flags &= !F_HEAD;
        rerank(st);
        if st.n_cand == 0 {
            fail(st, E_NONE_THERE);
        } else {
            select(st, 0);
            st.screen = S_CARD;
        }
    } else if st.n_cand == 0 {
        fail(st, E_NONE_THERE);
    } else {
        select(st, 0);
        st.screen = S_CARD;
    }
}

// separate frames: only one handler's locals are on the stack at a time
#[inline(never)]
unsafe fn tick() {
    let Some(st) = state() else { return };
    let t = now();
    if t.wrapping_sub(st.t_key) > IDLE_CLOSE {
        close_page();
        return;
    }
    st.phase = st.phase.wrapping_add(1);
    let live = if st.slot >= 0 { heading_now() } else { None };
    if let Some(h) = live {
        st.live = h;
        st.flags |= F_LIVE;
    } else {
        st.flags &= !F_LIVE;
    }
    let got = take(st);
    let mut dirty = false;

    match st.screen {
        S_AIM => {
            let age = t.wrapping_sub(st.t_open);
            if st.flags & F_HELD != 0 {
                dirty = true; // lock ring
                if let Some(h) = live {
                    if age >= AIM_FROM {
                        let k = st.ns as usize % NSMP;
                        st.smp[k] = h;
                        st.smp_t[k] = t;
                        st.ns = st.ns.wrapping_add(1);
                    }
                }
                // the release event is the signal; the button entry is the
                // fallback when that event went to another page
                let held = *(BACK_HELD as *const u8) != 0;
                if held {
                    st.flags |= F_SEEN;
                }
                if age > HOLD_MAX || (!held && st.flags & F_SEEN != 0) {
                    release(st);
                }
            } else if st.flags & F_AIM_DONE != 0 {
                dirty = true; // seconds counter
                match got {
                    Some(false) if st.code < 0 && st.nm as u32 > RETRY_NM => {
                        st.nm = RETRY_NM as u8;
                        if let Err(e) = fetch(st, RETRY_NM) {
                            fail(st, e);
                        }
                    }
                    Some(false) => fail(st, E_CODE),
                    Some(true) if st.pending_lookup != 0 => finish_lookup(st),
                    _ => {
                        if t.wrapping_sub(st.t_fetch) > LOOKUP_TIMEOUT {
                            fail(st, E_TIMEOUT);
                        }
                    }
                }
            }
        }
        S_CARD => {
            if got == Some(true) {
                rerank(st);
                dirty = true;
            }
            let n = st.n_cand as usize;
            if n > 0 {
                let a = st.ac[st.cand[st.sel as usize % n].idx as usize % MAX_AC];
                dirty |= route_tick(st, &a);
            }
        }
        S_MAP => {
            if got == Some(true) {
                rerank(st);
                dirty = true;
            }
            if t.wrapping_sub(st.t_fetch.max(st.t_data)) > MAP_REFRESH {
                let _ = fetch(st, st.nm as u32);
            }
        }
        _ => {}
    }
    if st.screen == S_CARD || st.screen == S_MAP {
        // things move: once a second, or sooner when the compass turned
        let turned = st.flags & F_LIVE != 0 && abs(wrap(st.live - st.drawn_live)) > 5.0 * DEG;
        if turned || t.wrapping_sub(st.t_paint) >= 1000 {
            dirty = true;
        }
    }
    if dirty || st.flags & F_DIRTY != 0 {
        st.flags &= !F_DIRTY;
        repaint();
    }
}

// separate frames: only one handler's locals are on the stack at a time
#[inline(never)]
unsafe fn key(k: u8) {
    let Some(st) = state() else {
        if k == KEY_SET {
            close_page();
        }
        return;
    };
    st.t_key = now();
    match k {
        KEY_SET if st.screen == S_MAP && st.n_cand > 0 => st.screen = S_CARD,
        KEY_SET => {
            close_page();
            return;
        }
        KEY_HOLD_SET => {
            if f0::<u8>(PHONE)() == 0 {
                fail(st, E_PHONE);
            } else if read_pos(st) {
                begin_aim(st);
            } else {
                fail(st, E_POS);
            }
        }
        KEY_REL_SET => release(st),
        KEY_GPS => match st.screen {
            S_MAP => {
                if st.n_cand > 0 {
                    st.screen = S_CARD;
                } else {
                    fail(st, E_NONE_THERE);
                }
            }
            S_CARD | S_ERR if st.flags & F_DATA != 0 || st.err == E_EMPTY || st.err == E_NONE_THERE => {
                st.screen = S_MAP;
                if ZOOMS_NM[st.zoom as usize % 3] > 20 {
                    let _ = fetch(st, st.nm as u32);
                }
            }
            _ => return,
        },
        KEY_UP | KEY_DOWN => {
            let down = k == KEY_DOWN;
            match st.screen {
                S_CARD if st.n_cand > 0 => {
                    let n = st.n_cand as usize;
                    let i = st.sel as usize % n;
                    select(st, if down { (i + 1) % n } else { (i + n - 1) % n });
                }
                S_MAP => {
                    let z = st.zoom as usize % 3;
                    let nz = if down { (z + 1).min(2) } else { z.saturating_sub(1) };
                    st.zoom = nz as u8;
                    if ZOOMS_NM[nz] > 20 && ZOOMS_NM[z] <= 20 {
                        let _ = fetch(st, st.nm as u32);
                    }
                }
                _ => return,
            }
        }
        _ => return,
    }
    repaint();
}

// ---- drawing ------------------------------------------------------------------------

struct Font {
    p: usize,
    h: i32,
    asc: i32,
}

unsafe fn font(id: u32) -> Font {
    let p = f1::<u32, usize>(FONT)(id);
    if p == 0 {
        return Font { p: 0, h: 12, asc: 10 };
    }
    let h = read_unaligned((p + 0x16) as *const u16) as i32;
    let asc = read_unaligned((p + 0x18) as *const u16) as i32;
    if !(4..=48).contains(&h) || !(1..=h).contains(&asc) {
        return Font { p, h: 12, asc: 10 };
    }
    Font { p, h, asc }
}

struct Gfx {
    dc: usize,
    scr: *mut [u8; SCRATCH],
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
    /// `s` as a C string in the scratch buffer (at most 39 bytes)
    unsafe fn cstr(&self, s: &[u8]) -> *const u8 {
        let z = &mut *self.scr;
        let n = s.len().min(39);
        z[..n].copy_from_slice(&s[..n]);
        z[n] = 0;
        z.as_ptr()
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
            let f: extern "C" fn(usize, i32, *const i16, i32, *mut u8) -> i32 = transmute(FILL_POLY | 1);
            f(self.dc, (pts.len() / 2) as i32, pts.as_ptr(), SCRATCH as i32, self.scr as *mut u8);
        }
    }
    fn text(&mut self, font: u8, x: i32, top: i32, align: u32, s: &[u8]) {
        let (fp, asc) = {
            let f = self.font(font);
            (f.p, f.asc)
        };
        if s.is_empty() || fp == 0 {
            return;
        }
        unsafe {
            let z = self.cstr(s);
            let pos: extern "C" fn(usize, i32, i32) = transmute(TEXT_POS | 1);
            let draw: extern "C" fn(usize, usize, *const u8, i32, u32, u32) = transmute(DRAW_TEXT | 1);
            let p = self.dc as *mut u8;
            *p.add(0x26) = 0xff; // transparent background
            *p.add(0x27) = self.fg;
            pos(self.dc, x, top + asc - 1);
            draw(self.dc, fp, z, 0, align, 0);
        }
    }
    fn width(&mut self, font: u8, s: &[u8]) -> i32 {
        let fp = self.font(font).p;
        if s.is_empty() || fp == 0 {
            return 0;
        }
        unsafe {
            let z = self.cstr(s);
            let w: extern "C" fn(usize, *const u8, u32, usize) -> i32 = transmute(TEXT_WIDTH | 1);
            w(fp, z, 1, self.dc)
        }
    }
    fn metrics(&mut self, font: u8) -> (i32, i32) {
        let f = self.font(font);
        (f.h, f.asc)
    }
}

// separate frames: only one handler's locals are on the stack at a time
#[inline(never)]
unsafe fn paint() {
    let Some(st) = state() else { return };
    let theme = *(THEME_PP as *const *const u8);
    let mut g = Gfx {
        dc: f0::<usize>(GET_DC)(),
        scr: &mut st.scratch,
        fg: *theme.add(3),
        bg: *theme.add(4),
        fs: font(FONT_SMALL),
        fm: font(FONT_MED),
    };
    // take the porthole from the stock inset painter (the face does the same)
    *(INSET_STATE as *mut u8) = 0x1f;
    *((INSET_STATE + 0xc) as *mut u32) = 0;
    g.rect(false, 0, 0, 128, 128);
    let t = now();
    st.t_paint = t;
    st.drawn_live = st.live;
    let live = if st.flags & F_LIVE != 0 { Some(st.live) } else { None };
    match st.screen {
        S_AIM => {
            if st.flags & F_HELD != 0 {
                let k = (st.ns.saturating_mul(3)).min(LOCK_STEPS as u32) as u8;
                paint_status(&mut g, b"Hold steady", b"let go of BACK", b"to look up", Inset::Lock(k), Some(st.ns as u8));
            } else if st.flags & F_AIM_DONE == 0 {
                paint_status(&mut g, b"Hold BACK", b"aim 12 o'clock", b"then let go", Inset::Idle, Some(2));
            } else {
                let mut b = Buf::new();
                b.int((t.wrapping_sub(st.t_fetch) / 1000) as i32, false).t(b" s");
                let from: &[u8] = if st.flags & F_PHONE_POS != 0 { b"phone position" } else { b"watch GPS fix" };
                paint_status(&mut g, b"Searching", from, b.s(), Inset::Radar((t.wrapping_sub(st.t_fetch) / 1000) as u8), Some((t.wrapping_sub(st.t_fetch) / 1000) as u8));
            }
        }
        S_CARD => {
            let n = st.n_cand as usize;
            let Some(c) = st.cand.get(st.sel as usize % n.max(1)).copied().filter(|_| n > 0) else {
                // a refresh emptied the list
                paint_status(&mut g, b"Lost it", b"hold BACK to", b"look again", Inset::Idle, None);
                return;
            };
            let a = st.ac[c.idx as usize % MAX_AC];
            let (e, nn) = enu(&st.own, &a, since_data(st, t));
            paint_card(
                &mut g,
                &CardInfo {
                    ac: &a,
                    dist_m: sqrt(e * e + nn * nn),
                    brg: atan2(e, nn),
                    heading: live,
                    route: route_of(st, a.hex).filter(|s| s.got == R_OK).map(|s| &s.route),
                    note: match route_of(st, a.hex).map(|s| s.got) {
                        Some(R_NONE) => &b"route unknown"[..],
                        Some(R_FAR) => b"route out of date",
                        Some(R_ERR) => b"route: no reply",
                        _ => b"",
                    },
                    flip: (t / 3000) & 1 != 0,
                },
            );
        }
        S_MAP => {
            let dt = since_data(st, t);
            let sel_idx = st.cand.get(st.sel as usize).filter(|_| st.n_cand > 0).map(|c| c.idx);
            let mut nb = 0;
            for i in 0..(st.n_ac as usize).min(MAX_AC) {
                let a = st.ac[i];
                if !a.airborne() {
                    continue;
                }
                let (e, n) = enu(&st.own, &a, dt);
                st.blips[nb] = Blip {
                    e,
                    n,
                    track: a.track as f32 * 0.1 * DEG,
                    has_track: a.track != NO_TRACK,
                    alt: a.alt,
                    sel: Some(i as u8) == sel_idx,
                    idx: i as u8,
                };
                nb += 1;
            }
            // nearest first (labels)
            for i in 1..nb {
                let mut j = i;
                let d = |b: &Blip| b.e * b.e + b.n * b.n;
                while j > 0 && d(&st.blips[j]) < d(&st.blips[j - 1]) {
                    st.blips.swap(j, j - 1);
                    j -= 1;
                }
            }
            let n = (st.n_ac as usize).min(MAX_AC);
            paint_map(&mut g, &st.blips[..nb], &st.ac[..n], live, ZOOMS_NM[st.zoom as usize % 3]);
        }
        _ => {
            let mut code = Buf::new();
            let (t1, l1, l2): (&[u8], &[u8], &[u8]) = match st.err {
                E_POS => (b"No position", b"phone location", b"or a GPS fix"),
                E_PHONE => (b"No phone", b"connect phone,", b"hold BACK"),
                E_HTTP => (b"No internet", b"Gadgetbridge:", b"turn it on"),
                E_TIMEOUT => (b"No reply", b"Gadgetbridge:", b"api.adsb.lol"),
                E_CODE => {
                    if st.code == -2 {
                        code.t(b"unreadable reply");
                    } else if st.code < 0 {
                        code.t(b"link error");
                    } else {
                        code.t(b"HTTP ").int(st.code, false);
                    }
                    (b"Server error", code.s(), b"hold BACK")
                }
                E_EMPTY => (b"Clear skies", b"no aircraft", b"within 25 nm"),
                E_NONE_THERE => (b"Nothing there", b"none that way;", b"GPS for map"),
                _ => (b"Out of memory", b"", b""),
            };
            paint_status(&mut g, t1, l1, l2, Inset::Idle, None);
        }
    }
}

/// The page handler (void, reads the event like every firmware page).
#[no_mangle]
pub unsafe extern "C" fn plane_page() {
    let ev = read_unaligned((EVT + 4) as *const u16);
    let t0 = now();
    if let Some(st) = state() {
        // Normal traffic is 4 ticks, 4 paints and a few keys a second. A
        // storm means a repaint loop: fault so it lands in err_log.txt.
        if t0.wrapping_sub(st.tw_t) > 1000 {
            st.tw_t = t0;
            st.tw_n = 0;
        }
        st.tw_n += 1;
        if st.tw_n > 100 {
            trap(0x5101, ev as u32, st.last_ev);
        }
        st.last_ev = ev as u32;
    }
    // The dispatcher (0x4b818) sets the hand-off byte to 1, calls the page,
    // and skips the paint pass when it is still 1 afterwards (0x4b714). The
    // paint pass is what copies the offscreen buffer we draw into to the
    // screen (type-7 canvas widget), so a paint left at 1 never reaches the
    // screen and a later, unrelated pass blits whatever the buffer holds.
    // The stock popup (0x100f8430) stores 2 after create, paint, destroy and
    // the keys it uses; do the same.
    let handled = |_: ()| *(HANDOFF as *mut u8) = 2;
    match ev {
        EV_CREATE => handled(create()),
        EV_DESTROY => handled(destroy()),
        EV_PAINT => handled(paint()),
        EV_TICK => tick(),
        EV_KEY => {
            let k = *((EVT + 6) as *const u8);
            key(k);
            if matches!(k, KEY_SET | KEY_HOLD_SET | KEY_REL_SET | KEY_GPS | KEY_UP | KEY_DOWN) {
                *(HANDOFF as *mut u8) = 2;
            }
        }
        // stock pages do not repaint from here; ask the next tick instead
        EV_SHOWN => {
            if let Some(st) = state() {
                st.flags |= F_DIRTY;
            }
        }
        _ => {}
    }
    // one handler call that ran for seconds and returned: fault with its time
    let dt = now().wrapping_sub(t0);
    if dt > 3000 {
        trap(0x5102, ev as u32, dt);
    }
}

/// Deliberate fault for the firmware's crash log: r0 = code, r1 = 0x0badc0de,
/// r2/r3 = details. GARMIN/Debug/err_log.txt shows them.
#[inline(never)]
unsafe fn trap(code: u32, a: u32, b: u32) -> ! {
    core::arch::asm!("udf #0xfd", in("r0") code, in("r1") 0x0bad_c0de_u32, in("r2") a, in("r3") b, options(noreturn));
}
