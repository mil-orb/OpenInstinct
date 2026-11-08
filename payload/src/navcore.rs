//! Pure logic for the nav screen: text and number parsing, the buzz and
//! session timing rules, and the screen layout and icons drawn through the
//! Draw trait. No firmware calls, no pointers, no allocation. lib.rs includes it
//! for the watch build; the host tests (host/navtest.rs, host/navfuzz.rs) and
//! the mock-up driver (host/navmock.rs) include the same file and run it with
//! overflow checks on, so any input that could panic or wrap shows up there.
//!
//! Rules for anything added here: index only through get()/get(..) or after an
//! explicit length check, no u64 division (the watch sysroot has no
//! __aeabi_uldivmod), and saturating or bounded arithmetic on numbers that come
//! from the phone.

/// App id prefix of Gadgetbridge's nav notifications; the action number follows.
pub const PREFIX: &[u8] = b"gadgetbridge.nav.v1.";

// approach bands, metres
pub const NEAR: u32 = 300;
pub const NOW: u32 = 80;

// record padding byte 0x85
pub const F_HELD: u8 = 0x80; // delete held back
pub const F_VIBE: u8 = 0x40; // vibrate on the next popup create
pub const F_SHOWN: u8 = 0x20; // shown at least once
pub const F_BAND: u8 = 0x03; // approach band last buzzed for

pub const F_TIME: u8 = 0x1c; // bits 8..10 of the event time (low bits in byte 0x87)

// Time of the last nav event (held delete or alert) in 4.096 s units
// (ms >> 12), 11 bits (byte 0x87 and F_TIME), wrapping every 2.3 hours.
// 0 means never; a stored time is never 0.
pub const TIME_MASK: u16 = 0x7ff;
pub const HELD_DONE: u16 = 2; // a delete held this long (4.1..8.2 s) is real
pub const GAP_FRESH: u16 = 15; // no nav event for this long (61..65 s): new session

/// The 11-bit event time stored in the flags byte and the time byte.
pub fn time_of(flags: u8, lo: u8) -> u16 {
    (((flags & F_TIME) as u16) << 6) | lo as u16
}

/// Flags and time byte with time `t` stored (0 is stored as 1).
pub fn with_time(flags: u8, t: u16) -> (u8, u8) {
    let t = match t & TIME_MASK {
        0 => 1,
        v => v,
    };
    ((flags & !F_TIME) | (((t >> 6) as u8) & F_TIME), t as u8)
}

/// Clock ticks from t to now, modulo the 11-bit wrap.
pub fn age(now: u16, t: u16) -> u16 {
    now.wrapping_sub(t) & TIME_MASK
}

/// A phone delete of the nav record: true to hold it back (an update is
/// coming as the following add), false to let it delete the record. It goes
/// through when an earlier delete is still held (that one had no add after
/// it: the session ended, this is a new one) or when nothing happened for
/// GAP_FRESH. Either way the add that follows creates a new, zeroed record,
/// so no flag from the old session can keep the popup closed.
pub fn hold_delete(flags: u8, lo: u8, now: u16) -> bool {
    if flags & F_HELD != 0 {
        return false;
    }
    let t = time_of(flags, lo);
    t == 0 || age(now, t) < GAP_FRESH
}

/// A held delete with no add behind it after HELD_DONE: navigation ended.
pub fn held_done(flags: u8, lo: u8, now: u16) -> bool {
    flags & F_HELD != 0 && age(now, time_of(flags, lo)) >= HELD_DONE
}

/// Radius of the visible circle around the screen centre (measured r 70..74
/// on the watch; one pixel inside the smallest).
pub const VIS_R: i32 = 69;

/// Largest n <= max that does not split a UTF-8 sequence. On invalid UTF-8
/// (a run of continuation bytes) it can return 0; never more than s.len().
pub fn utf8_cut(s: &[u8], max: usize) -> usize {
    if s.len() <= max {
        return s.len();
    }
    let mut n = max;
    while n > 0 && matches!(s.get(n), Some(c) if c & 0xc0 == 0x80) {
        n -= 1;
    }
    n
}

/// "gadgetbridge.nav.v1.<digits>" -> action number (wrapping on absurd lengths;
/// unknown actions just draw no icon).
pub fn parse_action(app: &[u8]) -> Option<u32> {
    let rest = app.strip_prefix(PREFIX)?;
    if rest.is_empty() {
        return None;
    }
    let mut v: u32 = 0;
    for &c in rest {
        if !c.is_ascii_digit() {
            return None;
        }
        v = v.wrapping_mul(10).wrapping_add((c - b'0') as u32);
    }
    Some(v)
}

/// Strip a leading arrow glyph (Gadgetbridge's optional nav symbol, multi-byte
/// UTF-8) and spaces. Only for the title and the ETA/remaining lines, where a
/// leading non-ASCII byte is the arrow and never a letter.
pub fn trim_lead(s: &[u8]) -> &[u8] {
    let mut i = 0;
    while i < s.len() && (s[i] >= 0x80 || s[i] == b' ') {
        i += 1;
    }
    s.get(i..).unwrap_or(&[])
}

/// Strip leading spaces only.
pub fn trim_spaces(s: &[u8]) -> &[u8] {
    let mut i = 0;
    while i < s.len() && s[i] == b' ' {
        i += 1;
    }
    s.get(i..).unwrap_or(&[])
}

/// Line `k` (0-based) of a '\n'-separated text; empty if there is no such line.
pub fn line(s: &[u8], k: usize) -> &[u8] {
    let mut start = 0;
    let mut n = 0;
    let mut i = 0;
    while i <= s.len() {
        if matches!(s.get(i), None | Some(&b'\n')) {
            if n == k {
                return s.get(start..i).unwrap_or(&[]);
            }
            n += 1;
            start = i + 1;
        }
        i += 1;
    }
    &[]
}

/// "350 m", "1.2 km", "0,4 mi", "500 ft", "1,200 ft", "80 yd", with an
/// optional leading arrow -> metres. A whole part of more than 5 digits reads
/// as 99999 and the fraction keeps 3 digits, so the products stay
/// far below u32::MAX; the arithmetic saturates anyway. A unit starting with
/// "m" that is not "mi" counts as metres.
pub fn metres(s: &[u8]) -> Option<u32> {
    let s = trim_lead(s);
    let mut i = 0;
    let mut whole: u32 = 0;
    let mut frac: u32 = 0;
    let mut fd: u32 = 1;
    let mut any = false;
    while i < s.len() && s[i].is_ascii_digit() {
        whole = if whole < 10_000 { whole * 10 + (s[i] - b'0') as u32 } else { 99_999 };
        any = true;
        i += 1;
    }
    // "1,200 ft": a comma followed by exactly three digits groups thousands
    // (a decimal comma comes with one or two digits: "0,4 mi", "1,25 km")
    if s.get(i) == Some(&b',') && any {
        let g = s.get(i + 1..i + 4).unwrap_or(&[]);
        if g.len() == 3 && g.iter().all(u8::is_ascii_digit) && !s.get(i + 4).is_some_and(u8::is_ascii_digit) {
            let x = (g[0] - b'0') as u32 * 100 + (g[1] - b'0') as u32 * 10 + (g[2] - b'0') as u32;
            whole = whole.saturating_mul(1000).saturating_add(x).min(99_999);
            i += 4;
        }
    }
    if i < s.len() && (s[i] == b'.' || s[i] == b',') {
        i += 1;
        while i < s.len() && s[i].is_ascii_digit() {
            if fd < 1000 {
                frac = frac * 10 + (s[i] - b'0') as u32;
                fd *= 10;
            }
            any = true;
            i += 1;
        }
    }
    if !any {
        return None;
    }
    // spaces between number and unit, including UTF-8 no-break and thin spaces
    // (Google Maps titles can use U+00A0; any non-ASCII byte is skipped here)
    while i < s.len() && (s[i] == b' ' || s[i] >= 0x80) {
        i += 1;
    }
    let u0 = s.get(i).copied().unwrap_or(0) | 0x20;
    let u1 = s.get(i + 1).copied().unwrap_or(0) | 0x20;
    // (numerator, denominator) of metres per unit
    let (f, d): (u32, u32) = match (u0, u1) {
        (b'k', b'm') => (1000, 1),
        (b'm', b'i') => (1609, 1),
        (b'f', b't') => (3048, 10_000),
        (b'y', b'd') => (9144, 10_000),
        (b'm', _) => (1, 1),
        _ => return None,
    };
    // checked_div: fd and d are never 0, but this keeps a panic path out
    let part = frac.saturating_mul(f).checked_div(fd).unwrap_or(0);
    whole.saturating_mul(f).saturating_add(part).checked_div(d)
}

/// Approach band: 0 far or unknown, 1 within NEAR, 2 within NOW.
pub fn band(m: Option<u32>) -> u8 {
    match m {
        Some(d) if d <= NOW => 2,
        Some(d) if d <= NEAR => 1,
        _ => 0,
    }
}

/// One-byte manoeuvre id: FNV-1a over the action and the instruction line,
/// folded to 8 bits.
pub fn hash(action: u32, s: &[u8]) -> u8 {
    let mut h: u32 = 0x811c_9dc5 ^ action;
    for &c in s {
        h = (h ^ c as u32).wrapping_mul(0x0100_0193);
    }
    (h ^ (h >> 8) ^ (h >> 16) ^ (h >> 24)) as u8
}

/// Whether this update deserves a buzz, and the new flags byte. A buzz comes
/// with the first showing, a new manoeuvre, or a closer approach band.
pub fn decide(flags: u8, old_hash: u8, h: u8, b: u8) -> (bool, u8) {
    let b = b & F_BAND;
    let vib = flags & F_SHOWN == 0 || old_hash != h || b > flags & F_BAND;
    (vib, (flags & !F_BAND) | F_SHOWN | b)
}

/// The distance at the start of `s`: number, separator and unit letters
/// ("2.5km left" -> "2.5km"). Empty if `s` does not start with a distance.
pub fn dist_prefix(s: &[u8]) -> &[u8] {
    let s = trim_lead(s);
    if metres(s).is_none() {
        return &[];
    }
    let mut i = 0;
    while i < s.len() && (s[i].is_ascii_digit() || s[i] == b'.' || s[i] == b',') {
        i += 1;
    }
    while i < s.len() && (s[i] == b' ' || s[i] >= 0x80) {
        i += 1;
    }
    while i < s.len() && s[i].is_ascii_alphabetic() {
        i += 1;
    }
    s.get(..i).unwrap_or(&[])
}

/// Gadgetbridge's body is the instruction, then "ETA: <time>" if known, then
/// "<distance> left" if known (both translated). Returns the (eta, left)
/// lines; a lone second line is the distance if it starts with one.
fn tail_lines(msg: &[u8]) -> (&[u8], &[u8]) {
    let l1 = line(msg, 1);
    let l2 = line(msg, 2);
    if trim_lead(l2).is_empty() && !dist_prefix(l1).is_empty() {
        (&[], l1)
    } else {
        (l1, l2)
    }
}

/// Arrival time: arrow, spaces, "ETA" (any case), ':' and spaces removed, at
/// most 8 bytes.
pub fn eta(msg: &[u8]) -> &[u8] {
    let mut e = trim_lead(tail_lines(msg).0);
    if e.len() >= 3 && (e[0] | 0x20) == b'e' && (e[1] | 0x20) == b't' && (e[2] | 0x20) == b'a' {
        e = e.get(3..).unwrap_or(&[]);
    }
    while let [b':' | b' ', rest @ ..] = e {
        e = rest;
    }
    e.get(..utf8_cut(e, 8)).unwrap_or(&[])
}

/// Distance left: the distance at the start of the line ("2.5km left" ->
/// "2.5km"), else the line itself, at most 8 bytes.
pub fn left(msg: &[u8]) -> &[u8] {
    let l = trim_lead(tail_lines(msg).1);
    let d = dist_prefix(l);
    let l = if d.is_empty() { l } else { d };
    l.get(..utf8_cut(l, 8)).unwrap_or(&[])
}

/// Integer square root (floor); 0 for v <= 0.
pub fn isqrt(v: i32) -> i32 {
    if v <= 0 {
        return 0;
    }
    let mut x = v as u32;
    let mut r: u32 = 0;
    let mut bit: u32 = 1 << 30;
    while bit > x {
        bit >>= 2;
    }
    while bit != 0 {
        if x >= r + bit {
            x -= r + bit;
            r = (r >> 1) + bit;
        } else {
            r >>= 1;
        }
        bit >>= 2;
    }
    r as i32
}

/// Half the visible width of pixel row y behind the round bezel; 0 outside.
pub fn halfw(y: i32) -> i32 {
    let d = 2 * y.clamp(-1000, 1000) + 1 - 128;
    isqrt(4 * VIS_R * VIS_R - d * d) / 2
}

/// Usable width for text spanning rows top..=bottom: the narrower end of the
/// visible circle, less a 3 px margin each side.
pub fn row_width(top: i32, bottom: i32) -> i32 {
    (2 * halfw(top).min(halfw(bottom)) - 6).max(0)
}

/// Seven-segment mask for a digit: bit 0 top, 1 top right, 2 bottom right,
/// 3 bottom, 4 bottom left, 5 top left, 6 middle. 0 for anything else.
pub fn seg_mask(c: u8) -> u8 {
    match c {
        b'0' => 0x3f,
        b'1' => 0x06,
        b'2' => 0x5b,
        b'3' => 0x4f,
        b'4' => 0x66,
        b'5' => 0x6d,
        b'6' => 0x7d,
        b'7' => 0x07,
        b'8' => 0x7f,
        b'9' => 0x6f,
        _ => 0,
    }
}

/// A distance title split for drawing: the number ("1,200") and the unit
/// ("ft", at most 4 letters). None when the title does not start with a
/// distance or the number is longer than 7 characters.
pub fn split_dist(title: &[u8]) -> Option<(&[u8], &[u8])> {
    let d = dist_prefix(title);
    if d.is_empty() {
        return None;
    }
    let mut i = 0;
    while i < d.len() && (d[i].is_ascii_digit() || d[i] == b'.' || d[i] == b',') {
        i += 1;
    }
    if i == 0 || i > 7 {
        return None;
    }
    let mut j = i;
    while j < d.len() && !d[j].is_ascii_alphabetic() {
        j += 1;
    }
    let unit = d.get(j..).unwrap_or(&[]);
    let unit = unit.get(..unit.len().min(4)).unwrap_or(&[]);
    Some((d.get(..i).unwrap_or(&[]), unit))
}

/// Width of a number drawn with seven-segment digits `w` wide, `t` thick,
/// `gap` apart; '1' (a single bar), '.' and ',' take t + gap.
pub fn digits_width(num: &[u8], w: i32, t: i32, gap: i32) -> i32 {
    let mut x = 0;
    for &c in num {
        x += match c {
            b'1' => t + gap,
            b'0'..=b'9' => w + gap,
            _ => t + gap,
        };
    }
    (x - gap).max(0)
}

/// The instruction broken over at most MAX_LINES lines by measured pixel
/// width. Line i is s[start[i]..end[i]] for i < n; the last line is followed
/// by ".." when `dots`.
pub struct Wrap {
    pub start: [usize; MAX_LINES],
    pub end: [usize; MAX_LINES],
    pub n: usize,
    pub dots: bool,
}

pub const MAX_LINES: usize = 3;

/// Longest text the watch measures in one go (its draw buffer is 64 bytes).
pub const MEASURE_MAX: usize = 63;

/// Largest char boundary n in 1..=len(s) (and <= MEASURE_MAX) with
/// width(s[..n]) + extra <= w; at least one character so a line always makes
/// progress; 0 only for empty s.
fn fit_chars(s: &[u8], w: i32, extra: i32, width: &mut dyn FnMut(&[u8]) -> i32) -> usize {
    let mut n = utf8_cut(s, MEASURE_MAX);
    while n > 0 {
        if width(s.get(..n).unwrap_or(&[])).saturating_add(extra) <= w {
            return n;
        }
        n -= 1;
        while n > 0 && matches!(s.get(n), Some(c) if c & 0xc0 == 0x80) {
            n -= 1;
        }
    }
    // not even one character fits: take one anyway
    let mut k = 1;
    while k < s.len() && matches!(s.get(k), Some(c) if c & 0xc0 == 0x80) {
        k += 1;
    }
    k.min(s.len())
}

/// Words after which a street or place name starts ("onto Main St"). A line
/// break right after one keeps the name together when it fits on the next line.
const NAME_LEAD: [&[u8]; 9] = [b"onto", b"on", b"into", b"to", b"toward", b"towards", b"at", b"via", b"for"];

fn ends_with_lead(s: &[u8]) -> bool {
    let w = match s.iter().rposition(|&c| c == b' ') {
        Some(i) => s.get(i + 1..).unwrap_or(&[]),
        None => s,
    };
    NAME_LEAD.iter().any(|k| w.eq_ignore_ascii_case(k))
}

/// Byte index of the break for a line that starts at `s` and is `w` px wide,
/// when s does not fit whole: the last space that fits, moved back to just
/// after a NAME_LEAD word when the name behind it fits on the next line
/// (`next` px, 0 when this is the last line). 0 when no space fits.
fn line_break(s: &[u8], w: i32, next: i32, width: &mut dyn FnMut(&[u8]) -> i32) -> usize {
    let lim = utf8_cut(s, MEASURE_MAX);
    let mut i = lim.min(s.len().saturating_sub(1));
    let mut greedy = 0;
    while i > 0 {
        if s.get(i) == Some(&b' ') && s.get(i - 1) != Some(&b' ') && width(s.get(..i).unwrap_or(&[])) <= w {
            greedy = i;
            break;
        }
        i -= 1;
    }
    if greedy == 0 || next <= 0 {
        return greedy;
    }
    // look back for "... onto" whose following name would otherwise be split
    let mut j = greedy;
    while j > 0 {
        if s.get(j) == Some(&b' ') && s.get(j - 1) != Some(&b' ') && ends_with_lead(s.get(..j).unwrap_or(&[])) {
            let mut k = j;
            while s.get(k) == Some(&b' ') {
                k += 1;
            }
            let tail = s.get(k..).unwrap_or(&[]);
            // only when the name itself spans the greedy break
            if k < greedy && tail.len() <= MEASURE_MAX && width(tail) <= next {
                return j;
            }
            return greedy;
        }
        j -= 1;
    }
    greedy
}

pub fn wrap_lines(s: &[u8], widths: &[i32], dots_w: i32, width: &mut dyn FnMut(&[u8]) -> i32) -> Wrap {
    let len = s.len();
    let mut r = Wrap { start: [len; MAX_LINES], end: [len; MAX_LINES], n: 0, dots: false };
    let lines = widths.len().min(MAX_LINES);
    let mut st = 0;
    while st < len && r.n < lines {
        let w = widths.get(r.n).copied().unwrap_or(0);
        let last = r.n + 1 == lines;
        let rest = s.get(st..).unwrap_or(&[]);
        let i = r.n;
        if let (Some(a), Some(b)) = (r.start.get_mut(i), r.end.get_mut(i)) {
            *a = st;
            *b = len;
        }
        r.n += 1;
        if rest.len() <= MEASURE_MAX && width(rest) <= w {
            return r;
        }
        let e = if last {
            let mut n = fit_chars(rest, w, dots_w, width);
            while n > 0 && rest.get(n - 1) == Some(&b' ') {
                n -= 1;
            }
            r.dots = true;
            n
        } else {
            let next = widths.get(i + 1).copied().unwrap_or(0);
            match line_break(rest, w, next, width) {
                0 => fit_chars(rest, w, 0, width),
                b => b,
            }
        };
        if let Some(b) = r.end.get_mut(i) {
            *b = st + e;
        }
        if last {
            return r;
        }
        st += e;
        while s.get(st) == Some(&b' ') {
            st += 1;
        }
    }
    r
}

// ---- screen ----------------------------------------------------------------
//
// The nav screen is drawn through this trait so the watch (lib.rs, firmware
// calls) and the host mock-up and fuzz (host/navmock.rs, host/navfuzz.rs) run
// the same layout code.

pub const SMALL: u8 = 0;
pub const MED: u8 = 1;
pub const CENTRE: u32 = 1;

pub trait Draw {
    /// Filled rectangle; ink true = foreground.
    fn rect(&mut self, ink: bool, x: i32, y: i32, w: i32, h: i32);
    /// Filled polygon, (x, y) pairs in pixels.
    fn poly(&mut self, ink: bool, pts: &[i16]);
    /// Text in font SMALL or MED with its top row at `top`; align 1 centres on
    /// x, 0 starts at x.
    fn text(&mut self, font: u8, x: i32, top: i32, align: u32, s: &[u8]);
    /// Pixel width of `s` (at most MEASURE_MAX bytes) in a font.
    fn width(&mut self, font: u8, s: &[u8]) -> i32;
    /// (cell height, ascent) of a font.
    fn metrics(&mut self, font: u8) -> (i32, i32);
}

// Gadgetbridge NavigationInfoSpec actions
pub const A_CONTINUE: u32 = 1;
pub const A_TURN_LEFT: u32 = 2;
pub const A_LEFT_SLIGHTLY: u32 = 3;
pub const A_LEFT_SHARPLY: u32 = 4;
pub const A_TURN_RIGHT: u32 = 5;
pub const A_RIGHT_SLIGHTLY: u32 = 6;
pub const A_RIGHT_SHARPLY: u32 = 7;
pub const A_KEEP_LEFT: u32 = 8;
pub const A_KEEP_RIGHT: u32 = 9;
pub const A_UTURN_LEFT: u32 = 10;
pub const A_UTURN_RIGHT: u32 = 11;
pub const A_OFFROUTE: u32 = 12;
pub const A_RB_RIGHT: u32 = 13;
pub const A_RB_LEFT: u32 = 14;
pub const A_RB_STRAIGHT: u32 = 15;
pub const A_RB_UTURN: u32 = 16;
pub const A_FINISH: u32 = 17;
pub const A_MERGE: u32 = 18;
/// Not a Gadgetbridge action: a junction ahead whose direction is unknown.
pub const A_JUNCTION: u32 = 19;

// sin(k * 22.5 deg) * 1024, k = 0..15. Heading k: 0 = up, clockwise.
const SIN: [i32; 16] = [0, 392, 724, 946, 1024, 946, 724, 392, 0, -392, -724, -946, -1024, -946, -724, -392];

pub fn dir(k: i32) -> (i32, i32) {
    let s = |i: i32| SIN.get((i & 15) as usize).copied().unwrap_or(0);
    (s(k), -s(k + 4)) // (sin, -cos): screen y grows downward
}

fn pt(v: i32) -> i16 {
    v.clamp(-1000, 1000) as i16
}

/// Thick segment from p0 to p1 along heading k, half-width hw.
#[allow(clippy::too_many_arguments)]
fn bar(d: &mut dyn Draw, ink: bool, x0: i32, y0: i32, x1: i32, y1: i32, k: i32, hw: i32) {
    let (sx, sy) = dir(k + 4); // perpendicular
    let ox = (sx * hw) >> 10;
    let oy = (sy * hw) >> 10;
    let p = [
        pt(x0 + ox), pt(y0 + oy),
        pt(x1 + ox), pt(y1 + oy),
        pt(x1 - ox), pt(y1 - oy),
        pt(x0 - ox), pt(y0 - oy),
    ];
    d.poly(ink, &p);
}

fn disc(d: &mut dyn Draw, ink: bool, x: i32, y: i32, r: i32) {
    let mut p = [0i16; 32];
    for (k, c) in p.chunks_exact_mut(2).enumerate() {
        let (dx, dy) = dir(k as i32);
        if let [a, b] = c {
            *a = pt(x + ((dx * r) >> 10));
            *b = pt(y + ((dy * r) >> 10));
        }
    }
    d.poly(ink, &p);
}

/// Icon drawing at a scale: lengths are given for the 64 px icon (sc 16).
struct Pen<'a> {
    d: &'a mut dyn Draw,
    sc: i32,
}

impl Pen<'_> {
    fn l(&self, v: i32) -> i32 {
        (v * self.sc + 8) >> 4
    }
    /// Segment of length `len` from (x, y) along heading k; returns the end point.
    fn arm(&mut self, x: i32, y: i32, k: i32, len: i32, hw: i32) -> (i32, i32) {
        let (dx, dy) = dir(k);
        let len = self.l(len);
        let hw = self.l(hw).max(1);
        let ex = x + ((dx * len) >> 10);
        let ey = y + ((dy * len) >> 10);
        bar(self.d, true, x, y, ex, ey, k, hw);
        disc(self.d, true, x, y, hw);
        (ex, ey)
    }
    /// Arrow head whose base centre is (x, y), pointing along heading k.
    fn head(&mut self, x: i32, y: i32, k: i32, len: i32, half: i32) {
        let (dx, dy) = dir(k);
        let (nx, ny) = dir(k + 4);
        let (len, half) = (self.l(len), self.l(half));
        let p = [
            pt(x + ((dx * len) >> 10)), pt(y + ((dy * len) >> 10)),
            pt(x + ((nx * half) >> 10)), pt(y + ((ny * half) >> 10)),
            pt(x - ((nx * half) >> 10)), pt(y - ((ny * half) >> 10)),
        ];
        self.d.poly(true, &p);
    }
    fn turn(&mut self, k: i32, cx: i32, cy: i32) {
        let (x, y) = self.arm(cx, cy + self.l(28), 0, 28, 5);
        let (x, y) = self.arm(x, y, k, 14, 5);
        self.head(x, y, k, 16, 13);
    }
}


/// Draw the manoeuvre icon centred on (cx, cy) at scale sc / 16.
pub fn icon(d: &mut dyn Draw, action: u32, cx: i32, cy: i32, sc: i32) {
    const HW: i32 = 5; // shaft half-width
    let mut g = Pen { d, sc: sc.clamp(4, 32) };
    match action {
        A_CONTINUE => g.turn(0, cx, cy),
        A_LEFT_SLIGHTLY => g.turn(-2, cx, cy),
        A_TURN_LEFT => g.turn(-4, cx, cy),
        A_LEFT_SHARPLY => g.turn(-6, cx, cy),
        A_RIGHT_SLIGHTLY => g.turn(2, cx, cy),
        A_TURN_RIGHT => g.turn(4, cx, cy),
        A_RIGHT_SHARPLY => g.turn(6, cx, cy),
        A_KEEP_LEFT | A_KEEP_RIGHT => {
            let s = if action == A_KEEP_LEFT { -1 } else { 1 };
            // the road you leave, drawn thin, then the branch you take
            let (x, y) = g.arm(cx, cy + g.l(28), 0, 20, HW);
            let (bx, by, bw) = (g.l(12), g.l(26), g.l(2).max(1));
            bar(g.d, true, x, y, x - s * bx, y - by, -s, bw);
            let (x, y) = g.arm(x, y, s, 22, HW);
            g.head(x, y, s, 16, 13);
        }
        A_UTURN_LEFT | A_UTURN_RIGHT => {
            let s = if action == A_UTURN_LEFT { -1 } else { 1 };
            let (x, y) = g.arm(cx - s * g.l(11), cy + g.l(28), 0, 40, HW);
            let (x, y) = g.arm(x, y, 4 * s, 22, HW);
            let (x, y) = g.arm(x, y, 8, 12, HW);
            g.head(x, y, 8, 14, 12);
        }
        A_RB_RIGHT | A_RB_LEFT | A_RB_STRAIGHT | A_RB_UTURN => {
            let k = match action {
                A_RB_RIGHT => 4,
                A_RB_LEFT => -4,
                A_RB_STRAIGHT => 0,
                _ => 8,
            };
            let (rx, ry) = (cx, cy - g.l(2));
            g.arm(cx, cy + g.l(30), 0, 18, HW);
            let (r1, r2) = (g.l(15), g.l(8));
            disc(g.d, true, rx, ry, r1);
            disc(g.d, false, rx, ry, r2);
            let (dx, dy) = dir(k);
            let r = g.l(12);
            let (x, y) = (rx + ((dx * r) >> 10), ry + ((dy * r) >> 10));
            let (x, y) = g.arm(x, y, k, 10, HW);
            g.head(x, y, k, 13, 11);
        }
        A_MERGE => {
            let (a, b, c, w) = (g.l(22), g.l(28), g.l(2), g.l(3).max(1));
            bar(g.d, true, cx - a, cy + b, cx, cy + c, 2, w);
            let (x, y) = g.arm(cx, cy + b, 0, 30, HW);
            g.head(x, y, 0, 16, 13);
        }
        A_FINISH => {
            let sc = g.sc;
            let l = |v: i32| (v * sc + 8) >> 4;
            g.d.rect(true, cx - l(18), cy - l(26), l(4).max(1), l(56)); // pole
            g.d.rect(true, cx - l(14), cy - l(26), l(32), l(24)); // flag
            let q = l(7).max(1);
            for j in 0..3 {
                for i in 0..4 {
                    if (i + j) & 1 == 1 {
                        g.d.rect(false, cx - l(12) + i * q, cy - l(24) + j * q, q, q);
                    }
                }
            }
        }
        A_JUNCTION => {
            // diamond road sign: outline and a centre dot
            let (r, w) = (g.l(24), g.l(5).max(1));
            let (ri, rd) = (r - w - w / 2, g.l(6).max(1));
            g.d.poly(true, &[pt(cx), pt(cy - r), pt(cx + r), pt(cy), pt(cx), pt(cy + r), pt(cx - r), pt(cy)]);
            g.d.poly(false, &[pt(cx), pt(cy - ri), pt(cx + ri), pt(cy), pt(cx), pt(cy + ri), pt(cx - ri), pt(cy)]);
            disc(g.d, true, cx, cy, rd);
        }
        A_OFFROUTE => {
            let (a, w) = (g.l(20), g.l(HW).max(1));
            bar(g.d, true, cx - a, cy - a, cx + a, cy + a, 6, w);
            bar(g.d, true, cx + a, cy - a, cx - a, cy + a, 10, w);
        }
        _ => {}
    }
}

/// Seven-segment number (digits, '.', ',') with its top-left at (x, y).
#[allow(clippy::too_many_arguments)]
#[inline(never)]
pub fn digits(d: &mut dyn Draw, num: &[u8], x: i32, y: i32, w: i32, h: i32, t: i32, gap: i32) {
    let mut x = x;
    let hu = (h + t) / 2; // upper vertical segment length
    let ml = (h - t) / 2; // middle bar top
    for &c in num {
        if c.is_ascii_digit() {
            let m = seg_mask(c);
            let w = if c == b'1' { t } else { w };
            // segment k: 0 top, 1 top right, 2 bottom right, 3 bottom,
            // 4 bottom left, 5 top left, 6 middle
            for k in 0..7 {
                if m & (1 << k) == 0 {
                    continue;
                }
                let (sx, sy, sw, sh) = match k {
                    0 => (x, y, w, t),
                    1 => (x + w - t, y, t, hu),
                    2 => (x + w - t, y + ml, t, h - ml),
                    3 => (x, y + h - t, w, t),
                    4 => (x, y + ml, t, h - ml),
                    5 => (x, y, t, hu),
                    _ => (x, y + ml, w, t),
                };
                d.rect(true, sx, sy, sw, sh);
            }
            x += w + gap;
        } else {
            // '.' sits on the baseline; ',' gets a short tail below it
            d.rect(true, x, y + h - t, t, t);
            if c == b',' {
                d.rect(true, x + t / 2, y + h, (t + 1) / 2, t);
            }
            x += t + gap;
        }
    }
}

/// Inset circle (the second display, top right) the layout keeps clear of.
pub const INSET_X: i32 = 105;
pub const INSET_Y: i32 = 22;
// The stock inset: a disc of r 27, but its ring and tick marks show to about
// r 32 on the watch, so layout keeps out to r 33.
pub const INSET_R: i32 = 33;

/// Leftmost column the inset covers on rows top..=bottom; 128 if none.
pub fn inset_left(top: i32, bottom: i32) -> i32 {
    let mut x = 128;
    let mut y = top.max(INSET_Y - INSET_R);
    while y <= bottom.min(INSET_Y + INSET_R) {
        let dy = y - INSET_Y;
        x = x.min(INSET_X - isqrt(INSET_R * INSET_R - dy * dy) - 1);
        y += 1;
    }
    x
}

/// Usable columns x0..x1 (exclusive) for something on rows top..=bottom:
/// inside the visible circle with a 3 px margin, left of the inset with 2.
pub fn span(top: i32, bottom: i32) -> (i32, i32) {
    let h = halfw(top).min(halfw(bottom)) - 3;
    let x0 = 64 - h;
    let x1 = (64 + h).min(inset_left(top, bottom) - 1);
    (x0, x1.max(x0))
}

/// Centre column for a box of width w on rows top..=bottom: the screen centre
/// when the box fits there, else the middle of the usable span.
pub fn centre(top: i32, bottom: i32, w: i32) -> i32 {
    let (x0, x1) = span(top, bottom);
    if 64 - (w + 1) / 2 >= x0 && 64 + (w + 1) / 2 <= x1 {
        64
    } else {
        (x0 + x1) / 2
    }
}

const TOP: i32 = 4; // first row used
const BOT: i32 = 121; // last row used
const ARROW_MIN: i32 = 22;
const ARROW_OK: i32 = 34; // large digits only when the icon keeps this much
const GAP: i32 = 3;
// seven-segment digit sizes, large then small: (height, width, thickness, gap)
const SEG: [(i32, i32, i32, i32); 2] = [(26, 15, 4, 3), (19, 11, 3, 2)];

/// Inked extent of each icon (actions 1..=18) relative to (cx, cy), in sc 16
/// units: (x0, y0, x1, y1), the outer bound over sc 4..16 scaled back to 16.
/// host/navtest.rs measures icon() and checks it against this table.
pub const ICON_BOX: [[i8; 4]; 19] = [
    [-14, -32, 14, 36], [-32, -14, 7, 36], [-24, -24, 7, 36], [-24, -7, 7, 36],
    [-7, -14, 32, 36], [-7, -24, 21, 36], [-7, -7, 21, 36], [-22, -32, 14, 36],
    [-14, -32, 21, 36], [-26, -20, 18, 36], [-18, -20, 26, 36], [-27, -27, 27, 27],
    [-16, -20, 38, 36], [-38, -20, 16, 36], [-16, -40, 16, 36], [-16, -20, 16, 36],
    [-20, -28, 18, 30], [-25, -20, 14, 36], [-26, -26, 26, 26],
];

/// The action to draw. Gadgetbridge sends 0 (or 255 after the first time)
/// when it cannot match Google Maps' icon, and has no icons for sharp turns,
/// so an unknown action is read from the instruction's words; when they say
/// nothing about direction the screen shows a neutral junction sign.
pub fn resolve_action(action: u32, ins: &[u8]) -> u32 {
    if (1..=18).contains(&action) {
        return action;
    }
    const RULES: &[(&[u8], u32)] = &[
        (b"u-turn", A_UTURN_LEFT),
        (b"u turn", A_UTURN_LEFT),
        (b"roundabout", A_RB_STRAIGHT),
        (b"traffic circle", A_RB_STRAIGHT),
        (b"rotary", A_RB_STRAIGHT),
        (b"sharp left", A_LEFT_SHARPLY),
        (b"sharp right", A_RIGHT_SHARPLY),
        (b"slight left", A_LEFT_SLIGHTLY),
        (b"slight right", A_RIGHT_SLIGHTLY),
        (b"bear left", A_LEFT_SLIGHTLY),
        (b"bear right", A_RIGHT_SLIGHTLY),
        (b"keep left", A_KEEP_LEFT),
        (b"keep right", A_KEEP_RIGHT),
        (b"stay left", A_KEEP_LEFT),
        (b"stay right", A_KEEP_RIGHT),
        (b"fork left", A_KEEP_LEFT),
        (b"fork right", A_KEEP_RIGHT),
        (b"turn left", A_TURN_LEFT),
        (b"turn right", A_TURN_RIGHT),
        (b"merge", A_MERGE),
        (b"destination", A_FINISH),
        (b"arrive", A_FINISH),
        (b"continue", A_CONTINUE),
        (b"head ", A_CONTINUE),
        (b"straight", A_CONTINUE),
    ];
    for &(w, a) in RULES {
        if has_word(ins, w) {
            return a;
        }
    }
    A_JUNCTION
}

/// `w` (lower case ASCII) in `s`, case-insensitive, not inside a longer word.
fn has_word(s: &[u8], w: &[u8]) -> bool {
    if w.is_empty() || s.len() < w.len() {
        return false;
    }
    let alnum = |c: Option<&u8>| c.is_some_and(|c| c.is_ascii_alphanumeric());
    (0..=s.len() - w.len()).any(|i| {
        s.get(i..i + w.len()).is_some_and(|t| t.eq_ignore_ascii_case(w))
            && !(i > 0 && alnum(s.get(i - 1)))
            && !(w.last().is_some_and(|c| c.is_ascii_alphanumeric()) && alnum(s.get(i + w.len())))
    })
}

/// Copy `parts` into `buf` with `sep` between non-empty parts; returns the length.
fn join(buf: &mut [u8], parts: &[&[u8]], sep: &[u8]) -> usize {
    let mut n = 0;
    for p in parts.iter().filter(|p| !p.is_empty()) {
        if n > 0 {
            for &c in sep {
                if let Some(b) = buf.get_mut(n) {
                    *b = c;
                    n += 1;
                }
            }
        }
        for &c in p.iter() {
            if let Some(b) = buf.get_mut(n) {
                *b = c;
                n += 1;
            }
        }
    }
    n.min(buf.len())
}

/// Top row of line i of n lines of height h ending on row `bottom`.
fn line_top(bottom: i32, n: usize, i: usize, h: i32) -> i32 {
    bottom - (n as i32 - i as i32) * h + 1
}

/// One line of text cut at a wrap end, with ".." when `dots`.
fn put_line(d: &mut dyn Draw, f: u8, top: i32, h: i32, seg: &[u8], dots: bool) {
    let mut buf = [0u8; MEASURE_MAX];
    let len = if dots {
        join(&mut buf, &[seg.get(..utf8_cut(seg, MEASURE_MAX - 2)).unwrap_or(&[]), b".."], b"")
    } else {
        join(&mut buf, &[seg.get(..utf8_cut(seg, MEASURE_MAX)).unwrap_or(&[])], b"")
    };
    let s = buf.get(..len).unwrap_or(&[]);
    let w = d.width(f, s);
    let (x0, x1) = span(top, top + h - 1);
    if w <= x1 - x0 {
        d.text(f, centre(top, top + h - 1, w), top, CENTRE, s);
    }
}

/// Paint the whole nav screen (background included) for one notification,
/// laid out from the bottom up: arrival time and distance left on the last
/// row, the instruction above it on up to three lines, the distance to the
/// manoeuvre in large digits, and the manoeuvre icon in what is left at the
/// top. Everything stays inside the visible circle and clear of the inset.
pub fn paint(d: &mut dyn Draw, action: u32, title: &[u8], msg: &[u8]) {
    // Each step is its own function (not inlined) so only one step's buffers
    // are on the UI task's stack at a time, under the firmware's own drawing.
    d.rect(false, 0, 0, 128, 128);
    let (hs, _) = d.metrics(SMALL);
    let (hm, ascm) = d.metrics(MED);
    let (hs, hm) = (hs.clamp(4, 48), hm.clamp(4, 48));
    let ascm = ascm.clamp(1, hm);
    let bottom = paint_eta(d, msg, hs);
    let above = paint_instruction(d, msg, bottom, hs, hm) - GAP;
    let dist_top = paint_distance(d, title, above, hm, ascm);
    paint_icon(d, resolve_action(action, line(msg, 0)), dist_top - GAP - TOP);
}

/// Bottom row: "14:32  3.4 km", or whichever of the two fits. Returns the last
/// row free above it.
#[inline(never)]
fn paint_eta(d: &mut dyn Draw, msg: &[u8], hs: i32) -> i32 {
    let (e, l) = (eta(msg), left(msg));
    if e.is_empty() && l.is_empty() {
        return BOT;
    }
    let top = BOT - hs + 1;
    let (x0, x1) = span(top, BOT);
    let w = x1 - x0;
    let mut buf = [0u8; 24];
    let mut n = join(&mut buf, &[e, l], b"  ");
    if d.width(SMALL, buf.get(..n).unwrap_or(&[])) > w {
        n = join(&mut buf, &[if e.is_empty() { l } else { e }], b"");
    }
    let s = buf.get(..n).unwrap_or(&[]);
    if d.width(SMALL, s) > w {
        return BOT;
    }
    d.text(SMALL, 64, top, CENTRE, s);
    top - GAP
}

/// The instruction above row `bottom`: the fewest medium lines that hold it
/// whole and leave room above for the distance and a small icon, else three
/// small lines, else as many medium lines as fit, cut with ".." at the end.
/// Returns the top row of the first line (bottom + GAP when there is none).
#[inline(never)]
fn paint_instruction(d: &mut dyn Draw, msg: &[u8], bottom: i32, hs: i32, hm: i32) -> i32 {
    let ins = trim_spaces(line(msg, 0));
    let ins = match ins.iter().position(|&c| c == b'\r') {
        Some(i) => ins.get(..i).unwrap_or(&[]),
        None => ins,
    };
    if ins.is_empty() {
        return bottom + GAP;
    }
    let nfit = ((bottom - TOP + 1) / hm).clamp(0, MAX_LINES as i32) as usize;
    let tries: [(u8, i32, usize); 5] = [(MED, hm, 1), (MED, hm, 2), (MED, hm, 3), (SMALL, hs, 3), (MED, hm, nfit)];
    for (k, &(f, h, n)) in tries.iter().enumerate() {
        if n == 0 || line_top(bottom, n, 0, h) < TOP {
            continue;
        }
        let mut ws = [0i32; MAX_LINES];
        for (i, w) in ws.iter_mut().enumerate().take(n) {
            let t = line_top(bottom, n, i, h);
            let (x0, x1) = span(t, t + h - 1);
            *w = x1 - x0;
        }
        let dots_w = d.width(f, b"..");
        let r = wrap_lines(ins, ws.get(..n).unwrap_or(&[]), dots_w, &mut |t: &[u8]| d.width(f, t));
        let room = line_top(bottom, n, 0, h) - GAP - TOP - ARROW_MIN - GAP - SEG[1].0;
        if (!r.dots && room >= 0) || k + 1 == tries.len() {
            // at the positions the widths were measured for; a wrap that
            // came out shorter leaves its last rows empty
            for i in 0..r.n {
                let t = line_top(bottom, n, i, h);
                let a = r.start.get(i).copied().unwrap_or(0);
                let b = r.end.get(i).copied().unwrap_or(0);
                put_line(d, f, t, h, ins.get(a..b).unwrap_or(&[]), r.dots && i + 1 == r.n);
            }
            return line_top(bottom, n, 0, h);
        }
    }
    bottom + GAP
}

/// Distance to the manoeuvre ending on row `above`: seven-segment digits with
/// the unit in the medium font on their baseline, else the title as text.
/// Returns its top row (above + 1 when nothing was drawn).
#[inline(never)]
fn paint_distance(d: &mut dyn Draw, title: &[u8], above: i32, hm: i32, ascm: i32) -> i32 {
    let title = trim_lead(title);
    if let Some((num, unit)) = split_dist(title) {
        let uw = if unit.is_empty() { 0 } else { d.width(MED, unit) + 3 };
        for (j, &(sh, sw, st, sg)) in SEG.iter().enumerate() {
            let top = above - sh + 1;
            let last = j + 1 == SEG.len();
            let total = digits_width(num, sw, st, sg) + uw;
            // the digits span rows top..=above, the unit its own text cell on
            // their baseline; each is kept inside the circle and left of the
            // inset on its own rows
            let (x0, x1) = span(top, above);
            if (!last && top - GAP - TOP < ARROW_OK) || total > x1 - x0 || top < TOP {
                continue;
            }
            let dw = total - uw;
            let utop = above + 1 - ascm;
            let (ux0, ux1) = if uw > 0 { span(utop, utop + hm - 1) } else { (x0, x1) };
            let mut x = 64 - total / 2;
            x -= (x + dw - x1).max(x + total - ux1).max(0);
            if x < x0 || x + dw + 3 < ux0 {
                continue;
            }
            digits(d, num, x, top, sw, sh, st, sg);
            if uw > 0 {
                // centred on its measured width: the watch's align 0 put the
                // text's right edge at x (seen on the watch), so only CENTRE
                // is trusted
                d.text(MED, x + dw + 3 + (uw - 3) / 2, utop, CENTRE, unit);
            }
            return top;
        }
    }
    let top = above - hm + 1;
    if title.is_empty() || top < TOP {
        return above + 1;
    }
    let (x0, x1) = span(top, above);
    let r = wrap_lines(title, &[x1 - x0], d.width(MED, b".."), &mut |t: &[u8]| d.width(MED, t));
    put_line(d, MED, top, hm, title.get(..r.end[0]).unwrap_or(&[]), r.dots);
    top
}

/// The icon in `room` rows from TOP, as large as fits inside the circle and
/// clear of the inset (its whole box, so this is conservative).
#[inline(never)]
fn paint_icon(d: &mut dyn Draw, action: u32, room: i32) {
    let bx = match ICON_BOX.get((action as usize).wrapping_sub(1)) {
        Some(b) => b,
        None => return, // nothing to draw for this action
    };
    let [x0, y0, x1, y1] = bx.map(|v| v as i32);
    let mut sc = 16;
    while sc >= 4 {
        let (l, t) = ((x0 * sc) >> 4, (y0 * sc) >> 4);
        let (r, b) = ((x1 * sc) >> 4, (y1 * sc) >> 4);
        let h = b - t + 1;
        if h <= room {
            let top = TOP + (room - h) / 2;
            let (sx0, sx1) = span(top, top + h - 1);
            // cx + l >= sx0 and cx + r < sx1
            let cx = (64 - (l + r) / 2).min(sx1 - 1 - r);
            if cx + l >= sx0 {
                icon(d, action, cx, top - t, sc);
                return;
            }
        }
        sc -= 1;
    }
}
