//! IFC1 face renderer: a port of faces/facekit.py `render`, op for op.
//!
//! Portable (core only, no firmware calls) so the same file builds for the
//! watch and for the PC test harness (payload/host/facetest.rs), which checks
//! it pixel for pixel against the Python reference.
//!
//! Every read of the face file goes through `Buf`, which returns 0 past the
//! end, and every canvas write is clipped, so a damaged or hostile file can
//! only produce a wrong picture. Loops are bounded (MAX_OPS, fixed point
//! counts), so it always finishes.

pub const W: i32 = 128;
pub const H: i32 = 128;
pub const STRIDE: usize = 16;
pub const CANVAS: usize = STRIDE * H as usize;

const MAX_OPS: u32 = 4096;
/// Work units (pixels visited plus scanlines and edges) a face may spend per
/// draw. The heaviest stock face design uses well under a tenth of this; a
/// file that goes over stops drawing early instead of stalling the UI.
pub const BUDGET: u32 = 400_000;
const MAX_STR: usize = 160;

// ops
const OP_RECT: u8 = 1;
const OP_BITMAP: u8 = 2;
const OP_TEXT: u8 = 3;
const OP_HAND: u8 = 4;
const OP_TICKS: u8 = 5;
const OP_BAR: u8 = 6;
const OP_IF: u8 = 7;
const OP_STRINGS: u8 = 8;
const OP_INSET: u8 = 9;
const OP_FRAMES: u8 = 10;
const OP_DOT: u8 = 11;
const OP_OWN: u8 = 12;

// sources
pub const HOUR: usize = 1;
pub const HOUR24: usize = 2;
pub const HOUR12: usize = 3;
pub const MINUTE: usize = 4;
pub const SECOND: usize = 5;
pub const AMPM: usize = 6;
pub const DAY: usize = 7;
pub const MONTH: usize = 8;
pub const YEAR: usize = 9;
pub const WEEKDAY: usize = 10;
pub const MINUTE_OF_DAY: usize = 11;
pub const HR: usize = 20;
pub const STEPS: usize = 21;
pub const STEP_GOAL: usize = 22;
pub const STEP_PCT: usize = 23;
pub const BATTERY: usize = 24;
pub const CALORIES: usize = 25;
pub const FLOORS: usize = 26;
pub const INTENSITY: usize = 27;
pub const ALTITUDE: usize = 28;
pub const TEMP: usize = 29;
pub const SUNRISE: usize = 30;
pub const SUNSET: usize = 31;
pub const NOTIFS: usize = 32;
pub const BT: usize = 33;
pub const ALARM: usize = 34;
pub const DND: usize = 35;
pub const DAYLIGHT: usize = 36;
pub const SUN_PROGRESS: usize = 37;
pub const MOON_AGE: usize = 38;
pub const IS24H: usize = 40;
/// The watch's inset setting (face cfg[7]); 7 = weather temperature.
pub const INSET_TYPE: usize = 41;
pub const MUSIC_TITLE: usize = 90; // 91 artist, 92 album (strings)
pub const MUSIC_PLAYING: usize = 93;
pub const MUSIC_POS: usize = 94;
pub const MUSIC_DUR: usize = 95;
pub const MUSIC_PCT: usize = 96;
pub const NSRC: usize = 100;

// formats
const F_ZPAD: u8 = 1;
const F_CLOCK: u8 = 2;
const F_NAME: u8 = 3;
const F_COMPACT: u8 = 4;
const F_LIT: u8 = 5;
const F_SPAD: u8 = 6;
const F_STR: u8 = 7;
const F_HEX: u8 = 8;
const F_MMSS: u8 = 9;

// bitmap modes
const M_INK: u8 = 0;
const M_OPAQUE: u8 = 1;
const M_ERASE: u8 = 2;
const M_XOR: u8 = 3;

const PATTERNS: [[u8; 8]; 11] = [
    [0xff; 8],
    [0xaa, 0x55, 0xaa, 0x55, 0xaa, 0x55, 0xaa, 0x55],
    [0x88, 0x00, 0x22, 0x00, 0x88, 0x00, 0x22, 0x00],
    [0xaa, 0x00, 0xaa, 0x00, 0xaa, 0x00, 0xaa, 0x00],
    [0xff, 0x00, 0xff, 0x00, 0xff, 0x00, 0xff, 0x00],
    [0xff, 0x00, 0x00, 0xff, 0x00, 0x00, 0xff, 0x00],
    [0x11, 0x22, 0x44, 0x88, 0x11, 0x22, 0x44, 0x88],
    [0x0f, 0x1e, 0x3c, 0x78, 0xf0, 0xe1, 0xc3, 0x87],
    [0xaa; 8],
    [0x80, 0x00, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00],
    [0x77, 0xdd, 0x77, 0xdd, 0x77, 0xdd, 0x77, 0xdd],
];

// round(sin(d) * 1024), d = 0..90
const SIN: [i16; 91] = [
    0, 18, 36, 54, 71, 89, 107, 125, 143, 160, 178, 195, 213, 230, 248, 265, 282, 299, 316, 333,
    350, 367, 384, 400, 416, 433, 449, 465, 481, 496, 512, 527, 543, 558, 573, 587, 602, 616, 630,
    644, 658, 672, 685, 698, 711, 724, 737, 749, 761, 773, 784, 796, 807, 818, 828, 839, 849, 859,
    868, 878, 887, 896, 904, 912, 920, 928, 935, 943, 949, 956, 962, 968, 974, 979, 984, 989, 994,
    998, 1002, 1005, 1008, 1011, 1014, 1016, 1018, 1020, 1022, 1023, 1023, 1024, 1024,
];

fn isin(d: i32) -> i32 {
    let d = d.rem_euclid(360);
    let s = |i: i32| SIN[i as usize] as i32;
    if d <= 90 {
        s(d)
    } else if d <= 180 {
        s(180 - d)
    } else if d <= 270 {
        -s(d - 180)
    } else {
        -s(360 - d)
    }
}
fn icos(d: i32) -> i32 {
    isin(d + 90)
}
/// Python floor division for any sign of b.
fn pdiv(a: i32, b: i32) -> i32 {
    if b == 0 {
        return 0;
    }
    let q = a.wrapping_div(b);
    if (a % b != 0) && ((a < 0) != (b < 0)) {
        q - 1
    } else {
        q
    }
}
/// Python modulo (result has the sign of b).
fn pmod(a: i32, b: i32) -> i32 {
    if b == 0 {
        return 0;
    }
    a.wrapping_sub(pdiv(a, b).wrapping_mul(b))
}

// ---- inputs -------------------------------------------------------------------

/// Live values. `has` bit i set = source i present (Python: not None).
/// String sources (STR0..7 = 80..87, music 90..92) live in `strs`.
pub struct Vals<'a> {
    pub num: [i32; NSRC],
    pub has: [u32; 4],
    pub strs: [&'a [u8]; 16], // index = src - 80
    pub str_has: u16,
}

impl<'a> Vals<'a> {
    pub const fn new() -> Self {
        Vals { num: [0; NSRC], has: [0; 4], strs: [&[]; 16], str_has: 0 }
    }
    pub fn set(&mut self, src: usize, v: i32) {
        if src < NSRC {
            self.num[src] = v;
            self.has[src >> 5] |= 1 << (src & 31);
        }
    }
    pub fn set_str(&mut self, src: usize, s: &'a [u8]) {
        if (80..96).contains(&src) {
            self.strs[src - 80] = s;
            self.str_has |= 1 << (src - 80);
        }
    }
    fn is_str(&self, src: usize) -> bool {
        (80..96).contains(&src) && self.str_has & (1 << (src - 80)) != 0
    }
    pub fn get(&self, src: usize) -> Option<i32> {
        if src < NSRC && self.has[src >> 5] & (1 << (src & 31)) != 0 {
            Some(self.num[src])
        } else {
            None
        }
    }
    fn get_str(&self, src: usize) -> Option<&'a [u8]> {
        if self.is_str(src) {
            Some(self.strs[src - 80])
        } else {
            None
        }
    }
    fn setdefault(&mut self, src: usize, v: i32) {
        if self.get(src).is_none() {
            self.set(src, v);
        }
    }
    /// Python truthiness of vals.get(src).
    fn truthy(&self, src: usize) -> bool {
        if let Some(s) = self.get_str(src) {
            return !s.is_empty();
        }
        matches!(self.get(src), Some(v) if v != 0)
    }
}

pub fn days_from_civil(y: i32, m: i32, d: i32) -> i32 {
    let y = y - (m <= 2) as i32;
    let era = pdiv(y, 400);
    let yoe = y - era * 400;
    let doy = pdiv(153 * (m + if m > 2 { -3 } else { 9 }) + 2, 5) + d - 1;
    let doe = yoe * 365 + pdiv(yoe, 4) - pdiv(yoe, 100) + doy;
    era * 146097 + doe - 719468
}

/// facekit.derived()
pub fn derive(v: &mut Vals) {
    let is24 = v.truthy(IS24H);
    if let Some(h) = v.get(HOUR24) {
        let h12 = if pmod(h, 12) == 0 { 12 } else { pmod(h, 12) };
        v.setdefault(HOUR12, h12);
        v.setdefault(AMPM, (h >= 12) as i32);
        v.setdefault(HOUR, if is24 { h } else { h12 });
    }
    if let (Some(s), Some(g)) = (v.get(STEPS), v.get(STEP_GOAL)) {
        if g != 0 {
            v.setdefault(STEP_PCT, pdiv(s.saturating_mul(100), g).min(100));
        }
    }
    if let (Some(h), Some(m)) = (v.get(HOUR24), v.get(MINUTE)) {
        let md = h * 60 + m;
        v.setdefault(MINUTE_OF_DAY, md);
        if let (Some(sr), Some(ss)) = (v.get(SUNRISE), v.get(SUNSET)) {
            if ss > sr {
                if sr <= md && md < ss {
                    v.setdefault(DAYLIGHT, 1);
                    v.setdefault(SUN_PROGRESS, pdiv((md - sr) * 1000, ss - sr));
                } else {
                    let night = 1440 - (ss - sr);
                    v.setdefault(DAYLIGHT, 0);
                    v.setdefault(SUN_PROGRESS, 1000 + pdiv(pmod(md - ss, 1440) * 1000, night));
                }
            }
        }
    }
    if let (Some(p), Some(d)) = (v.get(MUSIC_POS), v.get(MUSIC_DUR)) {
        if d != 0 {
            v.setdefault(MUSIC_PCT, pdiv(p.saturating_mul(100), d).min(100));
        }
    }
    if let (Some(y), Some(m), Some(d)) = (v.get(YEAR), v.get(MONTH), v.get(DAY)) {
        let dd = days_from_civil(y, m, d) - days_from_civil(2000, 1, 6);
        // (dd * 1000 - 760) mod 29531, without 64-bit maths
        v.setdefault(MOON_AGE, pdiv(pmod(pmod(dd, 29531) * 1000 - 760, 29531), 1000));
    }
}

// ---- file access ----------------------------------------------------------------

#[derive(Clone, Copy)]
struct Buf<'a>(&'a [u8]);
impl<'a> Buf<'a> {
    fn u8(&self, o: usize) -> u8 {
        *self.0.get(o).unwrap_or(&0)
    }
    fn i8(&self, o: usize) -> i32 {
        self.u8(o) as i8 as i32
    }
    fn u16(&self, o: usize) -> i32 {
        self.u8(o) as i32 | (self.u8(o + 1) as i32) << 8
    }
    fn i16(&self, o: usize) -> i32 {
        self.u16(o) as u16 as i16 as i32
    }
    fn u32(&self, o: usize) -> usize {
        self.u16(o) as usize | (self.u16(o + 2) as usize) << 16
    }
    /// NUL-terminated string at o (empty if out of range).
    fn cstr(&self, o: usize) -> &'a [u8] {
        let s = self.0.get(o..).unwrap_or(&[]);
        let n = s.iter().position(|&c| c == 0).unwrap_or(s.len());
        &s[..n.min(MAX_STR)]
    }
}

// ---- canvas ------------------------------------------------------------------

pub struct Canvas<'c> {
    pub px: &'c mut [u8; CANVAS],
    pub work: u32,
}

impl<'c> Canvas<'c> {
    pub fn new(px: &'c mut [u8; CANVAS]) -> Self {
        Canvas { px, work: 0 }
    }
    fn spend(&mut self, n: i32) {
        self.work = self.work.saturating_add(n.max(0) as u32 + 1);
    }
    fn put(&mut self, x: i32, y: i32, ink: u8) {
        if x < 0 || x >= W || y < 0 || y >= H {
            return;
        }
        let i = y as usize * STRIDE + (x as usize >> 3);
        let m = 0x80u8 >> (x & 7);
        if let Some(b) = self.px.get_mut(i) {
            match ink {
                0 => *b &= !m,
                2 => *b ^= m,
                _ => *b |= m,
            }
        }
    }
    fn span(&mut self, y: i32, x0: i32, x1: i32, ink: u8, pat: u8) {
        if y < 0 || y >= H {
            return;
        }
        let x0 = x0.max(0);
        let x1 = x1.min(W);
        self.spend(x1 - x0);
        let p = PATTERNS.get(pat as usize).unwrap_or(&PATTERNS[0]);
        let bits = p[(y & 7) as usize];
        let mut x = x0;
        while x < x1 {
            if bits & (0x80 >> (x & 7)) != 0 {
                self.put(x, y, ink);
            }
            x += 1;
        }
    }
}

fn fill_poly(cv: &mut Canvas, pts: &[(i32, i32)], ink: u8) {
    let n = pts.len();
    if n == 0 {
        return;
    }
    let mut lo = i32::MAX;
    let mut hi = i32::MIN;
    // Points past +-1024 px only come from a broken file; skipping them keeps
    // the edge maths inside 32 bits.
    if pts.iter().any(|p| p.0.abs() > 16383 || p.1.abs() > 16383) {
        return;
    }
    for p in pts {
        lo = lo.min(p.1);
        hi = hi.max(p.1);
    }
    let y0 = (pdiv(lo, 16) - 1).max(0);
    let y1 = (pdiv(hi, 16) + 1).min(H - 1);
    let mut y = y0;
    while y <= y1 {
        cv.spend(n as i32);
        let sy = y * 16 + 8;
        let mut xs = [0i32; 16];
        let mut k = 0;
        for i in 0..n {
            let (mut ax, mut ay) = pts[i];
            let (mut bx, mut by) = pts[(i + 1) % n];
            if ay == by {
                continue;
            }
            if ay > by {
                core::mem::swap(&mut ax, &mut bx);
                core::mem::swap(&mut ay, &mut by);
            }
            if ay <= sy && sy < by && k < xs.len() {
                xs[k] = ax + pdiv((sy - ay) * (bx - ax), by - ay);
                k += 1;
            }
        }
        let xs = &mut xs[..k];
        // insertion sort: k <= 16, and core's sort_unstable costs 2 KB of flash
        for i in 1..k {
            let mut j = i;
            while j > 0 && xs[j - 1] > xs[j] {
                xs.swap(j - 1, j);
                j -= 1;
            }
        }
        let mut i = 0;
        while i + 1 < k {
            let p0 = -pdiv(-(xs[i] - 8), 16);
            let p1 = -pdiv(-(xs[i + 1] - 8), 16);
            cv.span(y, p0, p1, ink, 0);
            i += 2;
        }
        y += 1;
    }
}

fn polar(cx: i32, cy: i32, deg: i32, r16: i32) -> (i32, i32) {
    (cx * 16 + ((r16 * isin(deg)) >> 10), cy * 16 - ((r16 * icos(deg)) >> 10))
}

fn thick_line(cv: &mut Canvas, a: (i32, i32), b: (i32, i32), deg: i32, hw: i32, ink: u8) {
    let px = (hw * icos(deg)) >> 10;
    let py = (hw * isin(deg)) >> 10;
    fill_poly(cv, &[(a.0 + px, a.1 + py), (b.0 + px, b.1 + py), (b.0 - px, b.1 - py), (a.0 - px, a.1 - py)], ink);
}

fn blit(cv: &mut Canvas, f: Buf, off: usize, x: i32, y: i32, w: i32, h: i32, mode: u8, ink: u8) {
    if w <= 0 || h <= 0 {
        return;
    }
    let stride = ((w + 7) / 8) as usize;
    let (r0, r1) = ((-y).max(0), h.min(H - y));
    let (c0, c1) = ((-x).max(0), w.min(W - x));
    if c1 <= c0 {
        return;
    }
    for r in r0..r1 {
        if cv.work > BUDGET {
            return;
        }
        cv.spend(c1 - c0);
        let yy = y + r;
        let row = off + r as usize * stride;
        for c in c0..c1 {
            let xx = x + c;
            let on = f.u8(row + (c as usize >> 3)) & (0x80 >> (c & 7)) != 0;
            if on {
                if mode == M_XOR || ink == 2 {
                    cv.put(xx, yy, 2);
                } else {
                    cv.put(xx, yy, if mode == M_ERASE { 0 } else { ink });
                }
            } else if mode == M_OPAQUE {
                cv.put(xx, yy, if ink == 0 { 1 } else { 0 });
            }
        }
    }
}

// ---- text --------------------------------------------------------------------

/// Small fixed string buffer.
pub struct Str {
    b: [u8; MAX_STR],
    n: usize,
}
impl Str {
    const fn new() -> Self {
        Str { b: [0; MAX_STR], n: 0 }
    }
    fn push(&mut self, c: u8) {
        if self.n < MAX_STR {
            self.b[self.n] = c;
            self.n += 1;
        }
    }
    fn extend(&mut self, s: &[u8]) {
        for &c in s {
            self.push(c);
        }
    }
    fn as_slice(&self) -> &[u8] {
        &self.b[..self.n]
    }
    fn int(&mut self, v: i32) {
        let mut d = [0u8; 12];
        let mut k = 0;
        let neg = v < 0;
        let mut u = v.unsigned_abs();
        loop {
            d[k] = b'0' + (u % 10) as u8;
            k += 1;
            u /= 10;
            if u == 0 {
                break;
            }
        }
        if neg {
            self.push(b'-');
        }
        while k > 0 {
            k -= 1;
            self.push(d[k]);
        }
    }
    /// "%02d" for 0 <= v
    fn int2(&mut self, v: i32) {
        if (0..10).contains(&v) {
            self.push(b'0');
        }
        self.int(v);
    }
    fn hex(&mut self, v: u32) {
        let mut d = [0u8; 8];
        let mut k = 0;
        let mut u = v;
        loop {
            d[k] = b"0123456789ABCDEF"[(u & 15) as usize];
            k += 1;
            u >>= 4;
            if u == 0 {
                break;
            }
        }
        while k > 0 {
            k -= 1;
            self.push(d[k]);
        }
    }
    /// str.rjust(pad, c) applied to the current content.
    fn rjust(&mut self, pad: usize, c: u8) {
        let pad = pad.min(MAX_STR);
        if self.n >= pad {
            return;
        }
        let k = pad - self.n;
        let mut i = self.n;
        while i > 0 {
            i -= 1;
            self.b[i + k] = self.b[i];
        }
        for j in 0..k {
            self.b[j] = c;
        }
        self.n = pad;
    }
}

struct Font<'a> {
    f: Buf<'a>,
    off: usize,
    hgt: i32,
    sp: i32,
    n: usize,
    /// glyph table entry for each byte (lower case falls back to upper
    /// case, as facekit's look()); 0xff = no glyph
    idx: &'a [u8; 256],
}
impl<'a> Font<'a> {
    fn new(f: Buf<'a>, off: usize, idx: &'a mut [u8; 256]) -> Self {
        let n = f.u8(off + 2) as usize;
        idx.fill(0xff);
        // Python builds a dict from the table, so the last entry for a code wins.
        for i in 0..n {
            idx[f.u8(off + 4 + 8 * i) as usize] = i as u8;
        }
        for c in b'a'..=b'z' {
            if idx[c as usize] == 0xff {
                idx[c as usize] = idx[(c - 32) as usize];
            }
        }
        let idx: &'a [u8; 256] = idx;
        Font { f, off, hgt: f.u8(off) as i32, sp: f.i8(off + 1), n, idx }
    }
    fn look(&self, c: u8) -> Option<(i32, usize)> {
        let i = self.idx[c as usize];
        if i == 0xff {
            return None;
        }
        let e = self.off + 4 + 8 * i as usize;
        Some((self.f.u8(e + 1) as i32, self.f.u32(e + 4)))
    }
    /// advance of one byte: glyph width + spacing, 0 if no glyph
    fn adv(&self, c: u8) -> i32 {
        match self.look(c) {
            Some((w, _)) => w + self.sp,
            None => 0,
        }
    }
    fn tsum(&self, s: &[u8]) -> i32 {
        s.iter().map(|&c| self.adv(c)).sum()
    }
    /// facekit width(): max(0, sum(w + sp for found glyphs) - sp), from a tsum
    fn wt(&self, t: i32) -> i32 {
        (t - self.sp).max(0)
    }
}

/// facekit.wrap(): line k of the greedy word wrap, and whether more lines
/// follow. Linear in the string length (Python's version is cubic for long
/// words; the result is the same).
fn wrap_line(s: &[u8], maxw: i32, font: &Font, k: usize, out: &mut Str, cur: &mut Str) -> bool {
    cur.n = 0;
    let mut cur_t = 0;
    let sp_t = font.adv(b' ');
    let mut line = 0usize;
    out.n = 0;
    // emit a line; stop once line k+1 is known to exist
    macro_rules! emit {
        ($l:expr) => {{
            if line == k {
                out.n = 0;
                out.extend($l);
            } else if line > k {
                return true;
            }
            #[allow(unused_assignments)]
            {
                line += 1;
            }
        }};
    }
    for word in s.split(|&c| c == b' ') {
        if word.is_empty() {
            continue;
        }
        let word_t = font.tsum(word);
        let cand_t = if cur.n > 0 { cur_t + sp_t + word_t } else { word_t };
        if font.wt(cand_t) <= maxw {
            if cur.n > 0 {
                cur.push(b' ');
            }
            cur.extend(word);
            cur_t = cand_t;
            continue;
        }
        if cur.n > 0 {
            emit!(cur.as_slice());
        }
        let mut w = word;
        let mut w_t = word_t;
        let mut guard = 0;
        while font.wt(w_t) > maxw && guard < MAX_STR {
            guard += 1;
            // largest n >= 1 with width(w[..n]) <= maxw, else 1
            let mut n = w.len();
            let mut t = w_t;
            while n > 1 && font.wt(t) > maxw {
                n -= 1;
                t -= font.adv(w[n]);
            }
            emit!(&w[..n]);
            w = &w[n..];
            w_t -= t;
        }
        cur.n = 0;
        cur.extend(w);
        cur_t = w_t;
    }
    if cur.n > 0 {
        emit!(cur.as_slice());
    }
    false
}

fn fmt_value(v: &Vals, src: usize, fmt: u8, pad: u8, tables: &[(usize, usize); 3], f: Buf, out: &mut Str) {
    if let Some(s) = v.get_str(src) {
        out.extend(s);
        return;
    }
    let Some(x) = v.get(src) else {
        out.extend(b"--");
        return;
    };
    match fmt {
        F_STR => out.int(x),
        F_MMSS => {
            out.int(pdiv(x, 60));
            out.push(b':');
            out.int2(pmod(x, 60));
        }
        F_HEX => {
            out.hex(x as u32);
            out.rjust(pad as usize, b'0');
        }
        F_ZPAD => {
            out.int(x);
            out.rjust(pad as usize, b'0');
        }
        F_SPAD => {
            out.int(x);
            out.rjust(pad as usize, b' ');
        }
        F_CLOCK => {
            let mut h = pdiv(x, 60);
            let m = pmod(x, 60);
            if !v.truthy(IS24H) {
                h = pmod(h, 12);
                if h == 0 {
                    h = 12;
                }
            }
            out.int(h);
            out.push(b':');
            out.int2(m);
        }
        F_NAME => {
            let kind = match src {
                WEEKDAY => 0,
                MONTH => 1,
                AMPM => 2,
                _ => 9,
            };
            let i = if src == MONTH { x - 1 } else { x };
            let (off, n) = if kind < 3 { tables[kind] } else { (0, 0) };
            if i >= 0 && (i as usize) < n {
                let mut o = off;
                for _ in 0..i {
                    o += f.cstr(o).len() + 1;
                }
                out.extend(f.cstr(o));
            } else {
                out.extend(b"--");
            }
        }
        F_COMPACT => {
            if x >= 10000 {
                out.int(pdiv(x, 1000));
                out.push(b'.');
                out.int(pdiv(pmod(x, 1000), 100));
                out.push(b'k');
            } else {
                out.int(x);
            }
        }
        _ => out.int(x),
    }
}

fn hand_deg(v: &Vals, src: u8) -> i32 {
    let h24 = v.get(HOUR24).unwrap_or(0);
    let m = v.get(MINUTE).unwrap_or(0);
    let s = v.get(SECOND).unwrap_or(0);
    match src {
        0 => pdiv(pmod(h24, 12) * 60 + m, 2),
        1 => m * 6 + pdiv(s, 10),
        _ => s * 6,
    }
}

/// Number value for ops that only take numbers (strings count as missing).
fn num(v: &Vals, src: usize) -> Option<i32> {
    if v.is_str(src) {
        None
    } else {
        v.get(src)
    }
}

/// Quick header check; returns the ops range.
pub fn check(file: &[u8]) -> Option<(usize, usize)> {
    let f = Buf(file);
    if file.len() < 16 || &file[..4] != b"IFC1" || f.u32(12) != file.len() {
        return None;
    }
    let hl = f.u16(6) as usize;
    let ol = f.u32(8);
    if hl < 16 || hl + ol > file.len() {
        return None;
    }
    Some((hl, hl + ol))
}

/// Working buffers for render(). The watch keeps them on the heap: the UI
/// task's stack is small. All-zero bytes are a valid initial state.
pub struct Scratch {
    pub inset: Str,
    /// The face draws the inset circle itself (OP_OWN matched INSET_TYPE).
    pub own: bool,
    s: Str,
    line: Str,
    cur: Str,
    idx: [u8; 256],
}
impl Scratch {
    pub const fn new() -> Self {
        Scratch { inset: Str::new(), own: false, s: Str::new(), line: Str::new(), cur: Str::new(), idx: [0; 256] }
    }
}

/// Draw a face. None: not a valid face file. Some(true): `sc.inset` holds the
/// inset text. Some(false): the face has no inset op.
pub fn render(file: &[u8], v: &Vals, cv: &mut Canvas, sc: &mut Scratch) -> Option<bool> {
    let (mut pos, end) = check(file)?;
    let f = Buf(file);
    let mut tables = [(0usize, 0usize); 3];
    sc.inset.n = 0;
    sc.own = false;
    let mut has_inset = false;
    let mut count = 0;
    while pos < end && count < MAX_OPS && cv.work <= BUDGET {
        count += 1;
        let code = f.u8(pos);
        if code == 0 {
            break;
        }
        let size = f.u8(pos + 1) as usize;
        if size < 2 {
            break;
        }
        let b = pos + 2;
        match code {
            OP_RECT => {
                let (x, y, w, h) = (f.i16(b), f.i16(b + 2), f.u16(b + 4), f.u16(b + 6));
                let (ink, pat) = (f.u8(b + 8), f.u8(b + 9));
                let mut yy = y.max(-1);
                while yy < (y + h).min(H) {
                    cv.span(yy, x, x + w, ink, pat);
                    yy += 1;
                }
            }
            OP_BITMAP => {
                let (x, y, w, h) = (f.i16(b), f.i16(b + 2), f.u16(b + 4), f.u16(b + 6));
                blit(cv, f, f.u32(b + 10), x, y, w, h, f.u8(b + 8), 1);
            }
            OP_STRINGS => {
                let kind = f.u8(b) as usize;
                if kind < 3 {
                    tables[kind] = (f.u32(b + 2), f.u8(b + 1) as usize);
                }
            }
            OP_TEXT => {
                let (x, y) = (f.i16(b), f.i16(b + 2));
                let (align, src, fmt, pad, ink, mw) =
                    (f.u8(b + 4), f.u8(b + 5) as usize, f.u8(b + 6), f.u8(b + 7), f.u8(b + 8), f.u8(b + 9));
                let Scratch { s, line, cur, idx, .. } = &mut *sc;
                let font = Font::new(f, f.u32(b + 10), idx);
                cv.spend(256 + font.n as i32);
                s.n = 0;
                if fmt == F_LIT {
                    s.extend(f.cstr(f.u32(b + 14)));
                } else {
                    fmt_value(v, src, fmt, pad, &tables, f, s);
                }
                let maxw = mw as i32 * 2;
                let mut force = false;
                if fmt == F_STR && pad & 0x7f != 0 && maxw != 0 {
                    let k = (pad & 0x7f) as usize - 1;
                    let more = wrap_line(s.as_slice(), maxw, &font, k, line, cur);
                    force = pad & 0x80 != 0 && more;
                    s.n = 0;
                    s.extend(line.as_slice());
                }
                if maxw != 0 && (force || font.wt(font.tsum(s.as_slice())) > maxw) {
                    // while s and width(s + "..") > maxw: s = s[:-1]
                    let dots = 2 * font.adv(b'.');
                    let mut t = font.tsum(s.as_slice());
                    while s.n > 0 && font.wt(t + dots) > maxw {
                        s.n -= 1;
                        t -= font.adv(s.b[s.n]);
                    }
                    while s.n > 0 && matches!(s.b[s.n - 1], b' ' | 0x09..=0x0d | 0x1c..=0x1f) {
                        s.n -= 1;
                    }
                    s.extend(b"..");
                }
                let mut tw = 0;
                let mut ng = 0;
                for &c in s.as_slice() {
                    if let Some((w, _)) = font.look(c) {
                        tw += w;
                        ng += 1;
                    }
                }
                tw += font.sp * (ng - 1).max(0);
                let mut xx = x - match align {
                    1 => pdiv(tw, 2),
                    2 => tw,
                    _ => 0,
                };
                for &c in s.as_slice() {
                    if let Some((w, go)) = font.look(c) {
                        blit(cv, f, go, xx, y, w, font.hgt, M_INK, ink);
                        xx += w + font.sp;
                    }
                }
            }
            OP_HAND => {
                let (cx, cy, src, ink) = (f.i16(b), f.i16(b + 2), f.u8(b + 4), f.u8(b + 5));
                let (ln, tail, hwb, hwt) =
                    (f.u8(b + 6) as i32, f.u8(b + 7) as i32, f.u8(b + 8) as i32, f.u8(b + 9) as i32);
                let deg = hand_deg(v, src.min(2));
                let (bx, by) = polar(cx, cy, deg + 180, tail * 16);
                let (tx, ty) = polar(cx, cy, deg, ln * 16);
                let (pxb, pyb) = ((hwb * icos(deg)) >> 10, (hwb * isin(deg)) >> 10);
                let (pxt, pyt) = ((hwt * icos(deg)) >> 10, (hwt * isin(deg)) >> 10);
                fill_poly(cv, &[(bx + pxb, by + pyb), (tx + pxt, ty + pyt), (tx - pxt, ty - pyt), (bx - pxb, by - pyb)], ink);
            }
            OP_TICKS => {
                let (cx, cy) = (f.i16(b), f.i16(b + 2));
                let (ri, ro, n, hw) = (f.u8(b + 4) as i32, f.u8(b + 5) as i32, f.u8(b + 6) as i32, f.u8(b + 7) as i32);
                let (st, sw) = (f.i16(b + 8), f.i16(b + 10));
                let (src, ink, mx, unlit) = (f.u8(b + 12) as usize, f.u8(b + 13), f.u16(b + 14), f.u8(b + 16));
                let mut lit = n;
                if src != 0 {
                    lit = match num(v, src) {
                        Some(x) if mx != 0 => pdiv(x.saturating_mul(n), mx).clamp(0, n),
                        _ => 0,
                    };
                }
                for i in 0..n {
                    let d = st + if sw == 360 || n < 2 { pdiv(sw * i, n) } else { pdiv(sw * i, n - 1) };
                    let a = polar(cx, cy, d, ri * 16);
                    let e = polar(cx, cy, d, ro * 16);
                    if i < lit {
                        thick_line(cv, a, e, d, hw, ink);
                    } else if unlit != 0 {
                        thick_line(cv, a, e, d, 6, ink);
                    }
                }
            }
            OP_BAR => {
                let (x, y, w, h) = (f.i16(b), f.i16(b + 2), f.u16(b + 4), f.u16(b + 6));
                let (src, d, lo, hi) = (f.u8(b + 8) as usize, f.u8(b + 9), f.i16(b + 10), f.i16(b + 12));
                let (ink, pat) = (f.u8(b + 14), f.u8(b + 15));
                let fr = match num(v, src) {
                    Some(x) if hi > lo => pdiv((x - lo).saturating_mul(1024), hi - lo).clamp(0, 1024),
                    _ => 0,
                };
                if d < 2 {
                    let n = (w * fr) >> 10;
                    let xs = if d == 0 { x } else { x + w - n };
                    let mut yy = y.max(-1);
                    while yy < (y + h).min(H) {
                        cv.span(yy, xs, xs + n, ink, pat);
                        yy += 1;
                    }
                } else {
                    let n = (h * fr) >> 10;
                    let ys = if d == 2 { y + h - n } else { y };
                    let mut yy = ys.max(-1);
                    while yy < (ys + n).min(H) {
                        cv.span(yy, x, x + w, ink, pat);
                        yy += 1;
                    }
                }
            }
            OP_IF => {
                let (src, cmp, val, skip) = (f.u8(b) as usize, f.u8(b + 1), f.i16(b + 2), f.u16(b + 4) as usize);
                let n = num(v, src);
                let ok = match cmp {
                    0 => v.truthy(src),
                    1 => !v.truthy(src),
                    2 => matches!(n, Some(x) if x >= val),
                    3 => matches!(n, Some(x) if x < val),
                    4 => matches!(n, Some(x) if x == val),
                    5 => n.is_none(), // no number: missing (e.g. no weather yet)
                    _ => true,
                };
                if !ok {
                    pos += size + skip;
                    continue;
                }
            }
            OP_FRAMES => {
                let (x, y, w, h) = (f.i16(b), f.i16(b + 2), f.u16(b + 4), f.u16(b + 6));
                let (mode, src, lo, hi) = (f.u8(b + 8), f.u8(b + 9) as usize, f.i16(b + 10), f.i16(b + 12));
                let (n, fl) = (f.u8(b + 14) as i32, f.u8(b + 15));
                if n > 0 {
                    let i = match num(v, src) {
                        Some(x) if hi > lo => {
                            if fl & 1 != 0 {
                                pdiv(pmod(x - lo, hi - lo) * n, hi - lo)
                            } else {
                                pdiv((x - lo).saturating_mul(n), hi - lo).clamp(0, n - 1)
                            }
                        }
                        _ => 0,
                    };
                    let i = i.clamp(0, n - 1) as usize;
                    blit(cv, f, f.u32(b + 16 + 4 * i), x, y, w, h, mode, 1);
                }
            }
            OP_DOT => {
                let (cx, cy, src, ink) = (f.i16(b), f.i16(b + 2), f.u8(b + 4), f.u8(b + 5));
                let (rad, r16, mx, st) = (f.u8(b + 6) as i32, f.u8(b + 7) as i32, f.u16(b + 8), f.i16(b + 10));
                let deg = if src < 3 {
                    Some(hand_deg(v, src))
                } else {
                    match num(v, src as usize) {
                        Some(x) if mx != 0 => Some(pdiv(x.saturating_mul(360), mx)),
                        _ => None,
                    }
                };
                if let Some(d) = deg {
                    let d = d + st;
                    let (px, py) = polar(cx, cy, d, rad * 16);
                    let mut pts = [(0i32, 0i32); 15];
                    for (k, p) in pts.iter_mut().enumerate() {
                        let a = k as i32 * 24;
                        *p = (px + ((r16 * isin(a)) >> 10), py - ((r16 * icos(a)) >> 10));
                    }
                    fill_poly(cv, &pts, ink);
                }
            }
            OP_OWN => {
                if let Some(t) = v.get(INSET_TYPE) {
                    if (0..32).contains(&t) && f.u32(b) >> t & 1 != 0 {
                        sc.own = true;
                    }
                }
            }
            OP_INSET => {
                let (src, fmt) = (f.u8(b) as usize, f.u8(b + 1));
                let inset = &mut sc.inset;
                inset.n = 0;
                has_inset = true;
                if fmt == F_LIT {
                    inset.extend(f.cstr(f.u32(b + 4)));
                } else {
                    fmt_value(v, src, fmt, 0, &tables, f, inset);
                }
            }
            _ => {}
        }
        pos += size;
    }
    Some(has_inset)
}

impl Str {
    pub fn bytes(&self) -> &[u8] {
        self.as_slice()
    }
    pub const fn empty() -> Self {
        Str::new()
    }
}

/// Sources a face reads, as a bitmask (bit i = source i), so the watch only
/// asks the firmware for values the face shows. Derived sources pull in what
/// they are computed from.
pub fn uses(file: &[u8]) -> [u32; 4] {
    let mut m = [0u32; 4];
    let mut add = |s: usize| {
        if s < 128 {
            m[s >> 5] |= 1 << (s & 31);
        }
    };
    let Some((mut pos, end)) = check(file) else {
        return [0; 4];
    };
    let f = Buf(file);
    let mut count = 0;
    while pos < end && count < MAX_OPS {
        count += 1;
        let code = f.u8(pos);
        let size = f.u8(pos + 1) as usize;
        if code == 0 || size < 2 {
            break;
        }
        let b = pos + 2;
        match code {
            OP_TEXT => {
                add(f.u8(b + 5) as usize);
                if f.u8(b + 6) == F_CLOCK {
                    add(IS24H);
                }
            }
            OP_HAND => {
                add(HOUR24);
                add(MINUTE);
                add(SECOND);
            }
            OP_TICKS => add(f.u8(b + 12) as usize),
            OP_BAR => add(f.u8(b + 8) as usize),
            OP_IF | OP_INSET => add(f.u8(b) as usize),
            OP_OWN => add(INSET_TYPE),
            OP_FRAMES => add(f.u8(b + 9) as usize),
            OP_DOT => {
                let s = f.u8(b + 4) as usize;
                if s < 3 {
                    add(HOUR24);
                    add(MINUTE);
                    add(SECOND);
                } else {
                    add(s);
                }
            }
            _ => {}
        }
        pos += size;
    }
    let has = |m: &[u32; 4], s: usize| m[s >> 5] & (1 << (s & 31)) != 0;
    let mut extra = [0usize; 8];
    let mut k = 0;
    if has(&m, STEP_PCT) {
        extra[k] = STEPS;
        extra[k + 1] = STEP_GOAL;
        k += 2;
    }
    if has(&m, DAYLIGHT) || has(&m, SUN_PROGRESS) {
        extra[k] = SUNRISE;
        extra[k + 1] = SUNSET;
        k += 2;
    }
    if has(&m, MUSIC_PCT) {
        extra[k] = MUSIC_POS;
        extra[k + 1] = MUSIC_DUR;
        k += 2;
    }
    for &s in &extra[..k] {
        m[s >> 5] |= 1 << (s & 31);
    }
    m
}

pub fn uses_src(m: &[u32; 4], s: usize) -> bool {
    s < 128 && m[s >> 5] & (1 << (s & 31)) != 0
}
