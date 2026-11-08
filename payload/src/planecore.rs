//! Plane lookup core: ADS-B reply scanning, geometry, matching and the three
//! screens (status, card, map). Pure code with no firmware calls and no
//! allocation, so the host tests run exactly what the watch runs.
//!
//! The scanner uses integers only: it runs in the HTTP callback, on the phone
//! link task, and should not touch the FPU there. Everything after it (UI task)
//! uses f32 with its own sin/cos/atan2/sqrt, since core has no libm.
//!
//! Nothing here may panic: on the watch a panic is an endless loop. Indexing
//! goes through get() or is bounded by the loop that produced the index.

// ---- drawing ----------------------------------------------------------------

pub const SMALL: u8 = 0;
pub const MED: u8 = 1;

pub trait Draw {
    /// Filled rectangle; ink true = foreground.
    fn rect(&mut self, ink: bool, x: i32, y: i32, w: i32, h: i32);
    /// Filled polygon, (x, y) pairs in pixels.
    fn poly(&mut self, ink: bool, pts: &[i16]);
    /// Text with its top row at `top`; align 1 centres on x, 0 starts at x.
    fn text(&mut self, font: u8, x: i32, top: i32, align: u32, s: &[u8]);
    fn width(&mut self, font: u8, s: &[u8]) -> i32;
    /// (cell height, ascent)
    fn metrics(&mut self, font: u8) -> (i32, i32);
}

// ---- aircraft record --------------------------------------------------------

pub const MAX_AC: usize = 40;
pub const ALT_GROUND: i32 = i32::MIN;
pub const ALT_NONE: i32 = i32::MIN + 1;
pub const NO_TRACK: u16 = 0xffff;

pub const F_EMERG: u8 = 1; // "emergency" other than "none"
pub const F_MIL: u8 = 2; // dbFlags bit 0
pub const F_RATE: u8 = 4; // rate is valid
pub const F_POS: u8 = 8; // lat and lon both present

/// One aircraft from the reply, 48 bytes. Units as received, scaled to
/// integers: lat/lon 1e-6 deg, gs 0.1 kt, track 0.1 deg, age 0.1 s, dst
/// 0.01 nm from the query point (used only to keep the nearest MAX_AC).
#[derive(Clone, Copy)]
#[repr(C)]
pub struct Ac {
    pub hex: u32, // ICAO address; bit 24 set when the feed marks it non-ICAO (~)
    pub lat: i32,
    pub lon: i32,
    pub alt: i32, // ft (baro, else geometric), ALT_GROUND or ALT_NONE
    pub dst: u16,
    pub gs: u16,
    pub track: u16,
    pub rate: i16, // ft/min
    pub age: u16,
    pub flags: u8,
    pub cat: u8, // category "A3" -> 0xA3, 0 unknown
    pub flight: [u8; 8],
    pub typ: [u8; 4],
    pub reg: [u8; 8],
}

impl Ac {
    pub const ZERO: Ac = Ac {
        hex: 0,
        lat: 0,
        lon: 0,
        alt: ALT_NONE,
        dst: u16::MAX,
        gs: 0,
        track: NO_TRACK,
        rate: 0,
        age: 0,
        flags: 0,
        cat: 0,
        flight: [0; 8],
        typ: [0; 4],
        reg: [0; 8],
    };
    pub fn airborne(&self) -> bool {
        self.alt != ALT_GROUND
    }
}

/// Length of a NUL-padded field.
pub fn flen(f: &[u8]) -> usize {
    f.iter().position(|&c| c == 0).unwrap_or(f.len())
}

// ---- scanner ----------------------------------------------------------------

struct Cur<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Cur<'a> {
    fn peek(&self) -> u8 {
        *self.b.get(self.i).unwrap_or(&0)
    }
    fn ws(&mut self) {
        while matches!(self.peek(), b' ' | b'\n' | b'\r' | b'\t') {
            self.i += 1;
        }
    }
    fn eat(&mut self, c: u8) -> bool {
        self.ws();
        if self.peek() == c {
            self.i += 1;
            true
        } else {
            false
        }
    }
    /// A string value; returns its raw bytes (escapes left as they are).
    fn string(&mut self) -> Option<&'a [u8]> {
        self.ws();
        if self.peek() != b'"' {
            return None;
        }
        self.i += 1;
        let s = self.i;
        loop {
            let c = *self.b.get(self.i)?;
            if c == b'\\' {
                self.i += 2;
                continue;
            }
            if c == b'"' {
                let e = self.i;
                self.i += 1;
                return self.b.get(s..e);
            }
            self.i += 1;
        }
    }
    /// A number or literal token.
    fn scalar(&mut self) -> Option<&'a [u8]> {
        self.ws();
        let s = self.i;
        while !matches!(self.peek(), b',' | b'}' | b']' | b' ' | b'\n' | b'\r' | b'\t' | 0) {
            self.i += 1;
        }
        if self.i == s {
            None
        } else {
            self.b.get(s..self.i)
        }
    }
    /// Skip one value of any type.
    fn skip(&mut self) -> Option<()> {
        self.ws();
        match self.peek() {
            b'"' => {
                self.string()?;
            }
            b'{' | b'[' => {
                let mut depth = 0usize;
                loop {
                    let c = *self.b.get(self.i)?;
                    match c {
                        b'"' => {
                            self.string()?;
                            continue;
                        }
                        b'{' | b'[' => depth += 1,
                        b'}' | b']' => {
                            depth = depth.saturating_sub(1);
                            if depth == 0 {
                                self.i += 1;
                                return Some(());
                            }
                        }
                        _ => {}
                    }
                    self.i += 1;
                }
            }
            _ => {
                self.scalar()?;
            }
        }
        Some(())
    }
    /// A numeric field: Some(None) when the value is not a number (null, a
    /// string, an array), None only when the text is broken.
    fn num(&mut self, dec: u32) -> Option<Option<i32>> {
        self.ws();
        match self.peek() {
            b'"' | b'{' | b'[' => {
                self.skip()?;
                Some(None)
            }
            _ => Some(parse_fixed(self.scalar()?, dec)),
        }
    }
    fn text(&mut self, out: &mut [u8]) -> Option<()> {
        self.ws();
        if self.peek() != b'"' {
            return self.skip();
        }
        let s = self.string()?;
        copy_trim(out, s);
        Some(())
    }

    fn aircraft(&mut self) -> Option<Ac> {
        self.i += 1; // '{'
        let mut a = Ac::ZERO;
        let (mut lat, mut lon, mut geom) = (false, false, ALT_NONE);
        loop {
            if self.eat(b'}') {
                break;
            }
            let key = self.string()?;
            if !self.eat(b':') {
                return None;
            }
            match key {
                b"hex" => {
                    self.ws();
                    if self.peek() == b'"' {
                        a.hex = parse_hex(self.string()?);
                    } else {
                        self.skip()?;
                    }
                }
                b"flight" => self.text(&mut a.flight)?,
                b"r" => self.text(&mut a.reg)?,
                b"t" => self.text(&mut a.typ)?,
                b"category" => {
                    let mut c = [0u8; 2];
                    self.text(&mut c)?;
                    if c[0].is_ascii_uppercase() && c[1].is_ascii_digit() {
                        a.cat = ((c[0] - b'A' + 0xa) << 4) | (c[1] - b'0');
                    }
                }
                b"emergency" => {
                    let mut e = [0u8; 4];
                    self.text(&mut e)?;
                    if e[0] != 0 && &e != b"none" {
                        a.flags |= F_EMERG;
                    }
                }
                b"alt_baro" => {
                    self.ws();
                    if self.peek() == b'"' {
                        if self.string()? == b"ground" {
                            a.alt = ALT_GROUND;
                        }
                    } else if let Some(v) = self.num(0)? {
                        if a.alt != ALT_GROUND {
                            a.alt = v;
                        }
                    }
                }
                b"alt_geom" => {
                    if let Some(v) = self.num(0)? {
                        geom = v;
                    }
                }
                b"gs" => {
                    if let Some(v) = self.num(1)? {
                        a.gs = v.clamp(0, 30_000) as u16;
                    }
                }
                b"track" => {
                    if let Some(v) = self.num(1)? {
                        a.track = v.rem_euclid(3600) as u16;
                    }
                }
                b"baro_rate" => {
                    if let Some(v) = self.num(0)? {
                        a.rate = v.clamp(-32_000, 32_000) as i16;
                        a.flags |= F_RATE;
                    }
                }
                b"lat" => {
                    if let Some(v) = self.num(6)? {
                        if (-90_000_000..=90_000_000).contains(&v) {
                            a.lat = v;
                            lat = true;
                        }
                    }
                }
                b"lon" => {
                    if let Some(v) = self.num(6)? {
                        if (-180_000_000..=180_000_000).contains(&v) {
                            a.lon = v;
                            lon = true;
                        }
                    }
                }
                b"seen_pos" => {
                    if let Some(v) = self.num(1)? {
                        a.age = v.clamp(0, 6000) as u16;
                    }
                }
                b"dst" => {
                    if let Some(v) = self.num(2)? {
                        a.dst = v.clamp(0, 65_000) as u16;
                    }
                }
                b"dbFlags" => {
                    if let Some(v) = self.num(0)? {
                        if v & 1 != 0 {
                            a.flags |= F_MIL;
                        }
                    }
                }
                _ => self.skip()?,
            }
            self.eat(b',');
        }
        if a.alt == ALT_NONE && geom != ALT_NONE {
            a.alt = geom;
        }
        if lat && lon {
            a.flags |= F_POS;
        }
        Some(a)
    }
}

/// Copy a string field, dropping trailing spaces ("UAL123  "), NUL padded.
fn copy_trim(out: &mut [u8], s: &[u8]) {
    let mut n = s.len();
    while n > 0 && s[n - 1] == b' ' {
        n -= 1;
    }
    for (i, o) in out.iter_mut().enumerate() {
        *o = if i < n { s[i] } else { 0 };
        // keep only plain printable ASCII; the firmware fonts and our
        // widths are byte based
        if !(0x20..0x7f).contains(o) {
            *o = 0;
        }
    }
}

fn parse_hex(s: &[u8]) -> u32 {
    let mut v = 0u32;
    let mut flag = 0u32;
    for &c in s.iter().take(8) {
        let d = match c {
            b'0'..=b'9' => c - b'0',
            b'a'..=b'f' => c - b'a' + 10,
            b'A'..=b'F' => c - b'A' + 10,
            b'~' => {
                flag = 1 << 24;
                continue;
            }
            _ => continue,
        };
        v = (v << 4) | d as u32;
    }
    (v & 0x00ff_ffff) | flag
}

/// Decimal text to an integer with `dec` fraction digits, rounded half away
/// from zero. None for anything that is not a plain JSON number (null, true,
/// exponents).
pub fn parse_fixed(t: &[u8], dec: u32) -> Option<i32> {
    let (neg, body) = match t.first() {
        Some(b'-') => (true, &t[1..]),
        _ => (false, t),
    };
    let mut v: i64 = 0;
    let (mut digits, mut frac, mut dot, mut round) = (0u32, 0u32, false, false);
    for &c in body {
        match c {
            b'0'..=b'9' => {
                digits += 1;
                if !dot || frac < dec {
                    if v < 100_000_000_000_000 {
                        v = v * 10 + (c - b'0') as i64;
                    }
                    if dot {
                        frac += 1;
                    }
                } else if frac == dec && !round {
                    round = true;
                    if c >= b'5' {
                        v += 1;
                    }
                }
            }
            b'.' if !dot => dot = true,
            _ => return None,
        }
    }
    if digits == 0 {
        return None;
    }
    while frac < dec {
        v = v.saturating_mul(10);
        frac += 1;
    }
    let v = if neg { -v } else { v };
    Some(v.clamp(i32::MIN as i64 + 2, i32::MAX as i64) as i32)
}

/// Scan an adsb.lol v2 reply ({"ac":[{...},...], ...}) into `out`, keeping
/// the MAX_AC aircraft nearest the query point. Aircraft without a position
/// are dropped. Returns the count, or None when the text has no "ac" array.
/// A reply cut short keeps what was complete.
pub fn scan(body: &[u8], out: &mut [Ac]) -> Option<usize> {
    let mut c = Cur { b: body, i: 0 };
    if !c.eat(b'{') {
        return None;
    }
    let mut n = 0usize;
    let mut found = false;
    loop {
        if c.eat(b'}') {
            break;
        }
        let Some(key) = c.string() else { break };
        if !c.eat(b':') {
            break;
        }
        c.ws();
        if key == b"ac" && c.peek() == b'[' {
            c.i += 1;
            found = true;
            loop {
                if c.eat(b']') {
                    break;
                }
                c.ws();
                if c.peek() == b'{' {
                    match c.aircraft() {
                        Some(a) => n = keep(out, n, a),
                        None => return Some(n),
                    }
                } else if c.skip().is_none() {
                    return Some(n);
                }
                c.eat(b',');
            }
        } else if c.skip().is_none() {
            break;
        }
        c.eat(b',');
    }
    if found {
        Some(n)
    } else {
        None
    }
}

fn keep(out: &mut [Ac], n: usize, a: Ac) -> usize {
    // ground traffic would take slots from the planes overhead (busy airport)
    if a.flags & F_POS == 0 || !a.airborne() {
        return n;
    }
    if n < out.len() {
        out[n] = a;
        return n + 1;
    }
    // full: replace the farthest if this one is nearer
    let mut far = 0;
    for (i, o) in out.iter().enumerate() {
        if o.dst > out[far].dst {
            far = i;
        }
    }
    if let Some(f) = out.get_mut(far) {
        if a.dst < f.dst {
            *f = a;
        }
    }
    n
}

// ---- math -------------------------------------------------------------------

pub const PI: f32 = core::f32::consts::PI;
const TAU: f32 = 2.0 * PI;
pub const DEG: f32 = PI / 180.0;

fn trunc(x: f32) -> f32 {
    (x as i32) as f32
}

/// x wrapped to [-pi, pi].
pub fn wrap(x: f32) -> f32 {
    if !(-1.0e6..=1.0e6).contains(&x) {
        return 0.0; // also NaN
    }
    let k = trunc(x / TAU + if x >= 0.0 { 0.5 } else { -0.5 });
    let mut r = x - k * TAU;
    if r > PI {
        r -= TAU;
    } else if r < -PI {
        r += TAU;
    }
    r
}

pub fn sin(x: f32) -> f32 {
    let mut x = wrap(x);
    if x > PI / 2.0 {
        x = PI - x;
    } else if x < -PI / 2.0 {
        x = -PI - x;
    }
    let x2 = x * x;
    x * (1.0 + x2 * (-1.0 / 6.0 + x2 * (1.0 / 120.0 + x2 * (-1.0 / 5040.0 + x2 * (1.0 / 362_880.0)))))
}

pub fn cos(x: f32) -> f32 {
    sin(x + PI / 2.0)
}

/// atan2 to about 1e-5 rad.
pub fn atan2(y: f32, x: f32) -> f32 {
    let (ax, ay) = (abs(x), abs(y));
    if ax == 0.0 && ay == 0.0 {
        return 0.0;
    }
    let (z, swap) = if ay > ax { (ax / ay, true) } else { (ay / ax, false) };
    let z2 = z * z;
    let mut r = z * (0.999_866 + z2 * (-0.330_299_5 + z2 * (0.180_141 + z2 * (-0.085_133 + z2 * 0.020_835_1))));
    if swap {
        r = PI / 2.0 - r;
    }
    if x < 0.0 {
        r = PI - r;
    }
    if y < 0.0 {
        -r
    } else {
        r
    }
}

pub fn sqrt(x: f32) -> f32 {
    if x <= 0.0 || x.is_nan() {
        return 0.0;
    }
    let mut y = f32::from_bits(0x1fbd_1df5 + (x.to_bits() >> 1));
    y = 0.5 * (y + x / y);
    y = 0.5 * (y + x / y);
    0.5 * (y + x / y)
}

pub fn abs(x: f32) -> f32 {
    if x < 0.0 {
        -x
    } else {
        x
    }
}

fn round(x: f32) -> i32 {
    if x >= 0.0 {
        (x + 0.5) as i32
    } else {
        (x - 0.5) as i32
    }
}

// ---- geometry ---------------------------------------------------------------

pub const M_PER_DEG: f32 = 111_195.0;
pub const M_PER_NM: f32 = 1852.0;
const MPS_PER_KT: f32 = 0.514_444;

/// The watch's position, 1e-6 deg.
#[derive(Clone, Copy)]
pub struct Own {
    pub lat: i32,
    pub lon: i32,
    pub coslat: f32,
}

impl Own {
    pub fn new(lat: i32, lon: i32) -> Own {
        Own { lat, lon, coslat: cos(lat as f32 * 1e-6 * DEG) }
    }
    /// From the firmware's semicircles (2^31 = 180 deg).
    pub fn from_semi(lat: i32, lon: i32) -> Own {
        let f = |s: i32| ((s as i64 * 180_000_000) >> 31) as i32;
        Own::new(f(lat), f(lon))
    }
}

/// Position of `a` relative to `own` in metres (east, north), `dt` seconds
/// after the reply was received (negative: before), moved along its track.
/// The aircraft's own report age is added; the move is capped at 90 s.
pub fn enu(own: &Own, a: &Ac, dt: f32) -> (f32, f32) {
    let mut dlon = a.lon as i64 - own.lon as i64;
    if dlon > 180_000_000 {
        dlon -= 360_000_000;
    } else if dlon < -180_000_000 {
        dlon += 360_000_000;
    }
    let mut n = (a.lat - own.lat) as f32 * 1e-6 * M_PER_DEG;
    let mut e = dlon as i32 as f32 * 1e-6 * M_PER_DEG * own.coslat;
    if a.track != NO_TRACK && a.gs > 0 {
        let t = (a.age as f32 * 0.1 + dt).clamp(-90.0, 90.0);
        let v = a.gs as f32 * 0.1 * MPS_PER_KT * t;
        let trk = a.track as f32 * 0.1 * DEG;
        e += v * sin(trk);
        n += v * cos(trk);
    }
    (e, n)
}

/// How far away (m) an aircraft at this altitude can still be picked out by
/// eye in fair weather: 3 nm plus 0.9 nm per 1000 ft, up to 30 nm; airliners
/// a bit further, light aircraft and helicopters a bit less.
pub fn visible_m(a: &Ac) -> f32 {
    let alt = if a.alt == ALT_NONE { 5000.0 } else { a.alt.max(0) as f32 };
    let mut nm = (3.0 + alt * 0.0009).min(30.0);
    match a.cat {
        0xA3..=0xA5 => nm *= 1.2,
        0xA1 | 0xA7 | 0xB1..=0xB4 => nm *= 0.8,
        _ => {}
    }
    nm * M_PER_NM
}

// ---- matching -----------------------------------------------------------------

/// One candidate for the aimed direction.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct Cand {
    pub idx: u8,     // into the aircraft list
    pub cone: u8,    // 1: inside the tolerance cone
    pub _pad: u16,
    pub err: f32,    // |bearing - heading|, rad
    pub score: f32,
}

impl Cand {
    pub const ZERO: Cand = Cand { idx: 0, cone: 0, _pad: 0, err: 0.0, score: 0.0 };
}

/// Base half-width of the aim cone and its growth with compass jitter.
pub const TOL_BASE: f32 = 12.0 * DEG;
pub const TOL_MAX: f32 = 35.0 * DEG;
/// Candidates farther than this from the aim line are not offered at all.
pub const ERR_MAX: f32 = 60.0 * DEG;
/// Position uncertainty of a report: GPS, the watch's own fix, rounding.
const POS_UNC_M: f32 = 150.0;
/// Uncertainty in when the snapshot was taken versus the aim, s.
const TIME_UNC_S: f32 = 1.5;

pub fn tolerance(spread: f32) -> f32 {
    (TOL_BASE + 1.5 * spread).min(TOL_MAX)
}

/// Rank the visible airborne aircraft for an aim along `heading` (rad,
/// clockwise from true north; None = no compass) with circular spread
/// `spread` (rad), `dt` seconds after the reply. In-cone candidates come
/// first, best first: angular error against the cone plus straight-line
/// distance (0.35 per 10 nm), so of two planes along the same line the nearer
/// wins. Aircraft farther than they can be seen (visible_m) are left out. The
/// rest follow by angular error. Returns (count,
/// in-cone count).
pub fn rank(acs: &[Ac], own: &Own, dt: f32, heading: Option<f32>, spread: f32, out: &mut [Cand]) -> (usize, usize) {
    let tol = tolerance(spread);
    let mut n = 0;
    for (i, a) in acs.iter().enumerate() {
        if !a.airborne() || n >= out.len() {
            continue;
        }
        let (e, nn) = enu(own, a, dt);
        let dist = sqrt(e * e + nn * nn);
        let vis = visible_m(a);
        if dist > vis {
            continue;
        }
        // straight-line distance, so "nearest" means nearest in the sky
        let up = if a.alt == ALT_NONE { 0.0 } else { a.alt.max(0) as f32 * 0.3048 };
        let slant = sqrt(dist * dist + up * up);
        let (err, cone, score) = match heading {
            Some(h) => {
                // the aim is direction only: high up, a bearing error is a
                // smaller miss in the sky (cos elevation)
                let err = abs(wrap(atan2(e, nn) - h)) * dist / slant.max(1.0);
                if err > ERR_MAX {
                    continue;
                }
                // where it really is: GPS and track error plus timing error
                // at its speed, seen from this distance
                let unc = POS_UNC_M + a.gs as f32 * (0.1 * 0.5144) * TIME_UNC_S;
                let tol = tol + atan2(unc, dist.max(1.0)).min(TOL_MAX);
                let r = err / tol;
                let cone = err <= tol;
                let s = if cone { r * r + 0.35 * slant / (10.0 * M_PER_NM) } else { 10.0 + err };
                (err, cone, s)
            }
            None => (0.0, false, slant),
        };
        out[n] = Cand { idx: i as u8, cone: cone as u8, _pad: 0, err, score };
        n += 1;
    }
    // insertion sort, n <= MAX_AC
    for i in 1..n {
        let mut j = i;
        while j > 0 && out[j].score < out[j - 1].score {
            out.swap(j, j - 1);
            j -= 1;
        }
    }
    let cone = out[..n].iter().filter(|c| c.cone != 0).count();
    (n, cone)
}

/// Circular mean of compass samples given as sums of sin and cos: (heading,
/// spread), both rad. Spread is the circular standard deviation.
pub fn circ_mean(ss: f32, sc: f32, n: u32) -> Option<(f32, f32)> {
    if n == 0 {
        return None;
    }
    let (ms, mc) = (ss / n as f32, sc / n as f32);
    let r = sqrt(ms * ms + mc * mc).min(1.0);
    if r < 0.2 {
        return None; // samples all over the place
    }
    Some((atan2(ms, mc), sqrt(2.0 * (1.0 - r))))
}

// ---- text -------------------------------------------------------------------

pub struct Buf {
    pub b: [u8; 32],
    pub n: usize,
}

impl Buf {
    pub fn new() -> Buf {
        Buf { b: [0; 32], n: 0 }
    }
    pub fn s(&self) -> &[u8] {
        &self.b[..self.n.min(32)]
    }
    pub fn clear(&mut self) -> &mut Buf {
        self.n = 0;
        self
    }
    pub fn c(&mut self, c: u8) -> &mut Buf {
        if self.n < 32 {
            self.b[self.n] = c;
            self.n += 1;
        }
        self
    }
    pub fn t(&mut self, s: &[u8]) -> &mut Buf {
        for &c in s {
            if c == 0 {
                break;
            }
            self.c(c);
        }
        self
    }
    /// Decimal integer, optional thousands separators.
    pub fn int(&mut self, v: i32, sep: bool) -> &mut Buf {
        let mut u = v.unsigned_abs();
        if v < 0 {
            self.c(b'-');
        }
        let mut d = [0u8; 10];
        let mut k = 0;
        loop {
            d[k] = b'0' + (u % 10) as u8;
            u /= 10;
            k += 1;
            if u == 0 || k == 10 {
                break;
            }
        }
        while k > 0 {
            k -= 1;
            self.c(d[k]);
            if sep && k > 0 && k % 3 == 0 {
                self.c(b',');
            }
        }
        self
    }
    /// Tenths as "12.3", or "123" from 100 up.
    pub fn tenths(&mut self, v: i32) -> &mut Buf {
        if v.abs() >= 1000 {
            return self.int((v + if v < 0 { -5 } else { 5 }) / 10, false);
        }
        if v < 0 {
            self.c(b'-');
        }
        let u = v.unsigned_abs() as i32;
        self.int(u / 10, false).c(b'.').c(b'0' + (u % 10) as u8)
    }
}

impl Default for Buf {
    fn default() -> Self {
        Self::new()
    }
}

/// Common ICAO type designators with the name people use for them.
const TYPES: &[(&[u8; 4], &[u8])] = &[
    (b"A19N", b"A319neo"),
    (b"A20N", b"A320neo"),
    (b"A21N", b"A321neo"),
    (b"A319", b"Airbus A319"),
    (b"A320", b"Airbus A320"),
    (b"A321", b"Airbus A321"),
    (b"A332", b"A330-200"),
    (b"A333", b"A330-300"),
    (b"A339", b"A330-900"),
    (b"A359", b"A350-900"),
    (b"A35K", b"A350-1000"),
    (b"A388", b"A380"),
    (b"BCS1", b"A220-100"),
    (b"BCS3", b"A220-300"),
    (b"B712", b"Boeing 717"),
    (b"B737", b"737-700"),
    (b"B738", b"737-800"),
    (b"B739", b"737-900"),
    (b"B37M", b"737 MAX 7"),
    (b"B38M", b"737 MAX 8"),
    (b"B39M", b"737 MAX 9"),
    (b"B3XM", b"737 MAX 10"),
    (b"B744", b"747-400"),
    (b"B748", b"747-8"),
    (b"B752", b"757-200"),
    (b"B753", b"757-300"),
    (b"B762", b"767-200"),
    (b"B763", b"767-300"),
    (b"B764", b"767-400"),
    (b"B772", b"777-200"),
    (b"B77L", b"777-200LR"),
    (b"B77W", b"777-300ER"),
    (b"B788", b"787-8"),
    (b"B789", b"787-9"),
    (b"B78X", b"787-10"),
    (b"MD11", b"MD-11"),
    (b"E170", b"Embraer 170"),
    (b"E75L", b"Embraer 175"),
    (b"E75S", b"Embraer 175"),
    (b"E190", b"Embraer 190"),
    (b"E195", b"Embraer 195"),
    (b"E290", b"E190-E2"),
    (b"CRJ2", b"CRJ-200"),
    (b"CRJ7", b"CRJ-700"),
    (b"CRJ9", b"CRJ-900"),
    (b"DH8D", b"Dash 8-400"),
    (b"AT76", b"ATR 72"),
    (b"B190", b"Beech 1900"),
    (b"BE20", b"King Air 200"),
    (b"BE9L", b"King Air 90"),
    (b"PC12", b"Pilatus PC-12"),
    (b"C208", b"Cessna Caravan"),
    (b"C172", b"Cessna 172"),
    (b"C182", b"Cessna 182"),
    (b"C152", b"Cessna 152"),
    (b"P28A", b"Piper Cherokee"),
    (b"SR22", b"Cirrus SR22"),
    (b"SF50", b"Cirrus Jet"),
    (b"C56X", b"Citation Excel"),
    (b"C68A", b"Citation Latitude"),
    (b"E55P", b"Phenom 300"),
    (b"CL35", b"Challenger 350"),
    (b"GLF4", b"Gulfstream IV"),
    (b"GLF5", b"Gulfstream V"),
    (b"GL5T", b"Global 5000"),
    (b"B06\0", b"Bell 206"),
    (b"B407", b"Bell 407"),
    (b"EC35", b"Airbus H135"),
    (b"EC45", b"Airbus H145"),
    (b"AS50", b"AStar"),
    (b"R44\0", b"Robinson R44"),
    (b"H60\0", b"Black Hawk"),
    (b"C130", b"Hercules"),
    (b"C17\0", b"C-17"),
    (b"K35R", b"KC-135"),
];

pub fn type_name(t: &[u8; 4]) -> Option<&'static [u8]> {
    TYPES.iter().find(|(k, _)| {
        let n = flen(&k[..]);
        flen(t) == n && k[..n] == t[..n]
    }).map(|(_, v)| *v)
}

fn compass8(rad: f32) -> &'static [u8] {
    const P: [&[u8]; 8] = [b"N", b"NE", b"E", b"SE", b"S", b"SW", b"W", b"NW"];
    let d = wrap(rad) / DEG;
    let d = if d < 0.0 { d + 360.0 } else { d };
    P[((round(d / 45.0) as usize) & 7).min(7)]
}

// ---- screens ------------------------------------------------------------------

/// The round inset window, top right.
pub const INSET: (i32, i32) = (105, 22);
const CX: i32 = 64;

fn dots(d: &mut dyn Draw, cx: i32, cy: i32, r: f32, step_deg: i32, size: i32) {
    let mut a = 0;
    while a < 360 {
        let t = a as f32 * DEG;
        let x = cx + round(r * sin(t));
        let y = cy - round(r * cos(t));
        d.rect(true, x - size / 2, y - size / 2, size, size);
        a += step_deg;
    }
}

/// Polygon `pts` (icon frame: x right, y down, nose toward -y) turned by
/// `ang` (rad, clockwise from screen up) and moved to (cx, cy).
#[inline(never)] // keeps the paint frame small (UI task stack)
fn shape(d: &mut dyn Draw, ink: bool, cx: i32, cy: i32, ang: f32, pts: &[(f32, f32)]) {
    let (s, c) = (sin(ang), cos(ang));
    let mut buf = [0i16; 16];
    let n = pts.len().min(8);
    for (k, &(px, py)) in pts.iter().take(n).enumerate() {
        buf[2 * k] = (cx + round(px * c - py * s)) as i16;
        buf[2 * k + 1] = (cy + round(px * s + py * c)) as i16;
    }
    d.poly(ink, &buf[..2 * n]);
}

/// Aircraft glyph: a dart, nose along `ang`.
pub fn plane_icon(d: &mut dyn Draw, cx: i32, cy: i32, ang: f32, len: f32) {
    let l = len;
    shape(d, true, cx, cy, ang, &[(0.0, -0.6 * l), (0.42 * l, 0.42 * l), (0.0, 0.18 * l), (-0.42 * l, 0.42 * l)]);
}

/// Direction arrow from the centre outward.
fn pointer(d: &mut dyn Draw, cx: i32, cy: i32, ang: f32, len: f32) {
    shape(d, true, cx, cy, ang, &[(0.0, -len), (0.4 * len, -0.15 * len), (0.13 * len, -0.25 * len), (0.13 * len, 0.45 * len), (-0.13 * len, 0.45 * len), (-0.13 * len, -0.25 * len), (-0.4 * len, -0.15 * len)]);
}

fn text_c(d: &mut dyn Draw, f: u8, y: i32, s: &[u8]) {
    d.text(f, CX, y, 1, s);
}

/// Visible window as measured on the watch: the circle r 70..74 round the
/// centre, laid out to r 68; and the inset ring, whose black rim reaches
/// about r 31 from the porthole centre (106, 21.5).
pub const SAFE_R: i32 = 68;
pub const RING: (i32, i32, i32) = (212, 43, 64); // doubled centre and radius

/// Is the box [x0, x1] x [y0, y1] inside the visible circle and clear of the
/// inset ring? Exact: the circle holds a box when it holds its four corners,
/// and the ring is clear when the box's nearest point to its centre is.
pub fn box_ok(x0: i32, y0: i32, x1: i32, y1: i32) -> bool {
    for (x, y) in [(x0, y0), (x1, y0), (x0, y1), (x1, y1)] {
        let (dx, dy) = (2 * x - 128, 2 * y - 128);
        if dx * dx + dy * dy > 4 * SAFE_R * SAFE_R {
            return false;
        }
    }
    let nx = RING.0.clamp(2 * x0, 2 * x1) - RING.0;
    let ny = RING.1.clamp(2 * y0, 2 * y1) - RING.1;
    nx * nx + ny * ny >= RING.2 * RING.2
}

/// Text centred on x 64 at top y, cut a character at a time until it fits
/// the visible window. Returns the drawn width (0: nothing fitted).
fn fit_c(d: &mut dyn Draw, f: u8, y: i32, s: &[u8]) -> i32 {
    let (h, _) = d.metrics(f);
    let mut n = s.len();
    while n > 0 {
        let w = d.width(f, &s[..n]);
        if box_ok(CX - w / 2, y, CX + (w + 1) / 2, y + h) {
            d.text(f, CX, y, 1, &s[..n]);
            return w;
        }
        n -= 1;
    }
    0
}

/// Text left-aligned at x, top y, cut until it fits. Returns the width.
fn fit_l(d: &mut dyn Draw, f: u8, x: i32, y: i32, s: &[u8]) -> i32 {
    let (h, _) = d.metrics(f);
    let mut n = s.len();
    while n > 0 {
        let w = d.width(f, &s[..n]);
        if box_ok(x, y, x + w, y + h) {
            d.text(f, x, y, 0, &s[..n]);
            return w;
        }
        n -= 1;
    }
    0
}

/// What the round inset window shows.
#[derive(Clone, Copy, PartialEq)]
pub enum Inset {
    /// Aim lock filling up: 0..=LOCK_STEPS dots lit.
    Lock(u8),
    /// Busy without a lock to show.
    Spin(u8),
    /// Just the plane glyph.
    Idle,
    /// Waiting on the network: a radar scope whose sweep moves a step each
    /// repaint (the screen already repaints once a second for its counter).
    Radar(u8),
}

pub const LOCK_STEPS: u8 = 12;

/// Plane glyph in the inset, with a ring of LOCK_STEPS dots around it: lit
/// dots count the aim lock, or one lit dot circles while busy.
#[inline(never)] // keeps the paint frame small (UI task stack)
fn paint_inset(d: &mut dyn Draw, m: Inset) {
    if let Inset::Radar(p) = m {
        paint_radar(d, p);
        return;
    }
    plane_icon(d, INSET.0, INSET.1, 0.0, 15.0);
    if m == Inset::Idle {
        return;
    }
    for k in 0..LOCK_STEPS as i32 {
        let t = k as f32 * (360.0 / LOCK_STEPS as f32) * DEG;
        let x = INSET.0 + round(19.0 * sin(t));
        let y = INSET.1 - round(19.0 * cos(t));
        let lit = match m {
            Inset::Lock(n) => k < n as i32,
            Inset::Spin(p) => k == (p % LOCK_STEPS) as i32,
            Inset::Idle | Inset::Radar(_) => false,
        };
        let s = if lit { 5 } else { 2 };
        d.rect(true, x - s / 2, y - s / 2, s, s);
    }
}

/// Radar scope in the inset: rim, range ring, centre dot, and a sweep wedge
/// that moves 45 deg a step with two thin trailing wedges.
fn paint_radar(d: &mut dyn Draw, p: u8) {
    let (cx, cy) = (INSET.0, INSET.1);
    let at = |r: f32, a: f32| (cx + round(r * sin(a)), cy - round(r * cos(a)));
    // rim: a near-solid ring; range ring: sparse dots
    for k in 0..48 {
        let (x, y) = at(19.0, k as f32 * 7.5 * DEG);
        d.rect(true, x, y, 2, 2);
    }
    for k in 0..8 {
        let (x, y) = at(10.0, k as f32 * 45.0 * DEG);
        d.rect(true, x, y, 2, 2);
    }
    d.rect(true, cx - 1, cy - 1, 3, 3);
    let a = (p % 8) as f32 * 45.0 * DEG;
    // the beam, a filled wedge, then a fading trail of thinner wedges
    for (back, half, r) in [(0.0f32, 10.0f32, 18.0f32), (24.0, 4.0, 17.0), (40.0, 2.0, 15.0)] {
        let m = a - back * DEG;
        let (x1, y1) = at(r, m - half * DEG);
        let (x2, y2) = at(r, m + half * DEG);
        d.poly(true, &[cx as i16, cy as i16, x1 as i16, y1 as i16, x2 as i16, y2 as i16]);
    }
}

/// The sight: a notch at 12 o'clock, and a fine dotted spine up to it from
/// the middle with three chevrons growing toward the edge, so it is clear
/// which edge of the watch to point. `phase` picks the bold chevron
/// (bottom first) so a highlight climbs toward 12 on each repaint the screen
/// already makes; the others are drawn thin.
fn paint_sight(d: &mut dyn Draw, phase: u8) {
    d.poly(true, &[(CX - 7) as i16, 1, (CX + 7) as i16, 1, CX as i16, 10]);
    let mut y = 13;
    while y <= 51 {
        d.rect(true, CX, y, 1, 1);
        y += 3;
    }
    // (tip y, half width), bottom to top
    for (k, &(ty, h)) in [(45, 3), (31, 5), (17, 7)].iter().enumerate() {
        let s = if k as u8 == phase % 3 { 3 } else { 1 };
        for i in 0..=h {
            d.rect(true, CX - i, ty + i, 1, s);
            d.rect(true, CX + i, ty + i, 1, s);
        }
    }
}

/// Status screen: a title, up to two lines, the inset, and the sight when
/// `sight` is Some(phase).
pub fn paint_status(d: &mut dyn Draw, title: &[u8], l1: &[u8], l2: &[u8], inset: Inset, sight: Option<u8>) {
    let (hm, _) = d.metrics(MED);
    let (hs, _) = d.metrics(SMALL);
    paint_inset(d, inset);
    if let Some(p) = sight {
        paint_sight(d, p);
    }
    // the block ends by y 114 whatever the font sizes (the bottom line is the narrowest)
    let top = (114 - (hm + 4 + 2 * hs + 1)).clamp(54, 58); // 54: below the inset ring
    fit_c(d, MED, top, title);
    fit_c(d, SMALL, top + hm + 4, l1);
    fit_c(d, SMALL, top + hm + 4 + hs + 1, l2);
}

/// Radius of the direction marker's track around the main window.
const RIM: f32 = 58.0;

/// Where the aircraft is from the watch's 12 o'clock: a fixed notch at 12, the
/// aircraft glyph on the rim at its relative bearing, and a dotted path along
/// the rim from it back to 12 (turn that way). On target the glyph sits under
/// the notch and the path is gone.
#[inline(never)] // keeps the paint frame small (UI task stack)
fn paint_rim(d: &mut dyn Draw, rel: f32) {
    let rel = wrap(rel);
    let on = abs(rel) < 6.0 * DEG;
    let w = if on { 7 } else { 5 };
    d.poly(true, &[(CX - w) as i16, 1, (CX + w) as i16, 1, CX as i16, (2 + w) as i16]);
    let pos = |a: f32| (CX + round(RIM * sin(a)), 64 - round(RIM * cos(a)));
    if !on {
        let step = if rel > 0.0 { -9.0 * DEG } else { 9.0 * DEG };
        let mut a = rel + 2.0 * step;
        while abs(a) > 8.0 * DEG && (a > 0.0) == (rel > 0.0) {
            let (x, y) = pos(a);
            let (dx, dy) = (x - INSET.0, y - INSET.1);
            if dx * dx + dy * dy > 29 * 29 {
                d.rect(true, x - 1, y - 1, 2, 2);
            }
            a += step;
        }
    }
    let (x, y) = pos(if on { 0.0 } else { rel });
    let y = if on { y + 8 } else { y };
    plane_icon(d, x, y, if on { 0.0 } else { rel }, 12.0);
}

/// What the card shows for the selected candidate, recomputed every tick.
pub struct CardInfo<'a> {
    pub ac: &'a Ac,
    pub dist_m: f32,
    pub brg: f32,            // true bearing to it, rad
    pub heading: Option<f32>, // live compass heading, rad
    pub index: usize,        // 0-based in the candidate list
    pub count: usize,
    pub in_cone: bool,
    pub err: f32, // angular error at the aim, rad
}

#[inline(never)] // keeps the paint frame small (UI task stack)
pub fn paint_card(d: &mut dyn Draw, c: &CardInfo) {
    let a = c.ac;
    let (hm, _) = d.metrics(MED);
    let (hs, _) = d.metrics(SMALL);

    paint_kind(d, kind(a), INSET.0, INSET.1, 18.0);
    match c.heading {
        Some(h) => paint_rim(d, c.brg - h),
        None => d.text(MED, CX, 2, 1, compass8(c.brg)),
    }

    // Layout for the measured window (on the watch: medium about 22 px a
    // line, small 16): distance and speed top left, clear of the inset ring
    // and the 12 o'clock marker; callsign, type and altitude in the middle;
    // list position and match quality on the short bottom row. Every line is
    // fitted (cut, never drawn outside) against the window.
    let mut b = Buf::new();
    let t0 = 20;
    b.tenths(round(c.dist_m / M_PER_NM * 10.0)).t(b" nm");
    fit_l(d, SMALL, 18, t0, b.s());
    let sp = b.clear();
    sp.int((a.gs as i32 + 5) / 10, false).t(b" kt");
    fit_l(d, SMALL, 12, t0 + hs, sp.s());

    // callsign, else registration, else hex
    let id = b.clear();
    if a.flight[0] != 0 {
        id.t(&a.flight);
    } else if a.reg[0] != 0 {
        id.t(&a.reg);
    } else {
        for sh in [20, 16, 12, 8, 4, 0] {
            id.c(b"0123456789ABCDEF"[((a.hex >> sh) & 0xf) as usize]);
        }
    }
    let y0 = (t0 + 2 * hs + 1).max(54); // 54: clear of the inset ring below the porthole
    fit_c(d, MED, y0, id.s());

    // what it is
    let y1 = y0 + hm;
    let rt = b.clear();
    rt.t(type_name(&a.typ).unwrap_or(&a.typ[..flen(&a.typ)]));
    if a.flags & F_MIL != 0 {
        rt.t(b" mil");
    }
    fit_c(d, SMALL, y1, rt.s());

    // altitude with climb/descent mark
    let y2 = y1 + hs + 1;
    let al = b.clear();
    if a.alt == ALT_NONE {
        al.t(b"alt --");
    } else {
        al.int(a.alt, true).t(b" ft");
    }
    let w = fit_c(d, SMALL, y2, al.s());
    if w > 0 && a.flags & F_RATE != 0 && a.rate.unsigned_abs() >= 300 {
        let x = CX + w / 2 + 6;
        let yc = y2 + hs / 2;
        if box_ok(x - 4, yc - 4, x + 4, yc + 4) {
            let up = a.rate > 0;
            let pts: [i16; 6] = if up {
                [x as i16 - 4, yc as i16 + 3, x as i16 + 4, yc as i16 + 3, x as i16, yc as i16 - 4]
            } else {
                [x as i16 - 4, yc as i16 - 3, x as i16 + 4, yc as i16 - 3, x as i16, yc as i16 + 4]
            };
            d.poly(true, &pts);
        }
    }

    // position in the list and how well it matched; the window is narrow
    // down here, so the longest form that fits wins
    let y3 = y2 + hs + 1;
    let m = b.clear();
    m.int(c.index as i32 + 1, false).c(b'/').int(c.count as i32, false).c(b' ');
    let k = m.s().len();
    if a.flags & F_EMERG != 0 {
        m.t(b"EMERG");
    } else if c.heading.is_none() || c.err < 0.0 {
        m.t(b"nearest");
    } else if c.in_cone {
        m.t(b"match");
    } else {
        m.int(round(c.err / DEG), false).t("\u{b0} off".as_bytes());
    }
    let full = d.width(SMALL, m.s());
    if box_ok(CX - full / 2, y3, CX + (full + 1) / 2, y3 + hs) {
        d.text(SMALL, CX, y3, 1, m.s());
    } else {
        // the match word alone ("23 deg off" may lose " off"), else the
        // position alone
        let tail = &m.s()[k..];
        let short = if tail.ends_with(b" off") { &tail[..tail.len() - 4] } else { tail };
        let mut done = false;
        for t in [tail, short] {
            let wt = d.width(SMALL, t);
            if box_ok(CX - wt / 2, y3, CX + (wt + 1) / 2, y3 + hs) {
                d.text(SMALL, CX, y3, 1, t);
                done = true;
                break;
            }
        }
        if !done {
            fit_c(d, SMALL, y3, &m.s()[..k - 1]);
        }
    }
}

// ---- aircraft kinds -------------------------------------------------------------

/// Silhouette classes for the inset.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind {
    Light,    // single piston or single turboprop
    Twinprop, // twin turboprop or piston twin
    Bizjet,   // engines on the tail: business jets, CRJ, ERJ-145
    Jet,      // narrowbody airliner
    Heavy,    // widebody twin (and tri-jets)
    Quad,     // four engines
    Heli,
    Glider,
    Balloon,
    Drone,
    Unknown,
}

const HELI: &[&[u8]] = &[
    b"B06", b"B06T", b"B407", b"B412", b"B429", b"B505", b"EC20", b"EC30", b"EC35", b"EC45", b"EC55", b"EC75",
    b"AS32", b"AS50", b"AS55", b"AS65", b"R22", b"R44", b"R66", b"H60", b"H47", b"H64", b"H500", b"S76", b"S92",
    b"A109", b"A119", b"A139", b"A169", b"BK17", b"MD52", b"MD60", b"UH1", b"V22",
];
const QUAD: &[&[u8]] = &[
    b"B741", b"B742", b"B743", b"B744", b"B748", b"B74S", b"A388", b"A342", b"A343", b"A345", b"A346", b"C17",
    b"K35R", b"C135", b"E3TF", b"C5M", b"B52", b"A400", b"C130", b"P3",
];
const HEAVY: &[&[u8]] = &[
    b"A306", b"A310", b"A332", b"A333", b"A337", b"A338", b"A339", b"A359", b"A35K", b"B762", b"B763", b"B764",
    b"B772", b"B77L", b"B77W", b"B778", b"B779", b"B788", b"B789", b"B78X", b"MD11", b"DC10", b"KC10", b"K46",
];
/// Prefixes for jets with engines on the tail.
const BIZ: &[&[u8]] = &[
    b"CRJ", b"E135", b"E145", b"E35L", b"E45X", b"C25", b"C50", b"C51", b"C52", b"C55", b"C56", b"C65", b"C68",
    b"C700", b"C750", b"CL30", b"CL35", b"CL60", b"GLF", b"GL5", b"GL6", b"GL7", b"GLEX", b"LJ", b"E50P", b"E55P",
    b"FA", b"F2TH", b"F900", b"H25", b"PC24", b"SF50", b"HDJT", b"BE40", b"PRM1", b"MD8", b"MD9", b"B712", b"DC9",
];
const JET: &[&[u8]] = &[
    b"A31", b"A32", b"A19N", b"A20N", b"A21N", b"B73", b"B3", b"B75", b"E17", b"E19", b"E7", b"E29", b"BCS", b"A22",
];
const TWIN: &[&[u8]] = &[
    b"DH8", b"AT4", b"AT7", b"B190", b"BE20", b"BE9", b"BE30", b"BE58", b"BE55", b"BE60", b"PA31", b"PA34",
    b"PA44", b"PA23", b"C310", b"C340", b"C402", b"C414", b"C421", b"SW", b"SF34", b"DHC6", b"D228", b"L410",
    b"P180", b"DA42", b"DA62", b"P68", b"JS3", b"E120",
];

fn has_prefix(t: &[u8], list: &[&[u8]], exact: bool) -> bool {
    list.iter().any(|p| t.len() >= p.len() && &t[..p.len()] == *p && (!exact || t.len() == p.len()))
}

/// Which silhouette: the type code when it is one we know, else the ADS-B
/// emitter category.
pub fn kind(a: &Ac) -> Kind {
    let t = &a.typ[..flen(&a.typ)];
    if a.cat == 0xA7 || has_prefix(t, HELI, true) {
        return Kind::Heli;
    }
    if !t.is_empty() {
        if has_prefix(t, QUAD, true) {
            return Kind::Quad;
        }
        if has_prefix(t, HEAVY, true) {
            return Kind::Heavy;
        }
        if has_prefix(t, BIZ, false) {
            return Kind::Bizjet;
        }
        if has_prefix(t, JET, false) {
            return Kind::Jet;
        }
        if has_prefix(t, TWIN, false) {
            return Kind::Twinprop;
        }
    }
    match a.cat {
        0xA1 => Kind::Light,
        0xA2 | 0xA6 => Kind::Bizjet,
        0xA3 | 0xA4 => Kind::Jet,
        0xA5 => Kind::Heavy,
        0xB1 | 0xB4 => Kind::Glider,
        0xB2 => Kind::Balloon,
        0xB6 => Kind::Drone,
        // an unknown type code with no category: most of those are small
        // piston aircraft (homebuilts, older singles)
        0 if !t.is_empty() => Kind::Light,
        _ => Kind::Unknown,
    }
}

/// Top-down silhouette, nose up, centred on (cx, cy), `s` px from the centre
/// to the nose. Shapes are in units of `s`; parts marked mirrored are drawn
/// on both sides.
#[inline(never)] // keeps the paint frame small (UI task stack)
pub fn paint_kind(d: &mut dyn Draw, k: Kind, cx: i32, cy: i32, s: f32) {
    if k == Kind::Unknown {
        plane_icon(d, cx, cy, 0.0, 1.6 * s);
        return;
    }
    let mut part = |pts: &[(f32, f32)], m: bool| {
        let mut buf = [0i16; 20];
        let n = pts.len().min(10);
        for side in 0..(1 + m as i32) {
            let f = if side == 0 { 1.0 } else { -1.0 };
            for (i, &(x, y)) in pts.iter().take(n).enumerate() {
                buf[2 * i] = (cx + round(f * x * s)) as i16;
                buf[2 * i + 1] = (cy + round(y * s)) as i16;
            }
            d.poly(true, &buf[..2 * n]);
        }
    };
    // a thin bar from (x0, y0) to (x1, y1), half width w (rotor blades)
    fn bar(x0: f32, y0: f32, x1: f32, y1: f32, w: f32) -> [(f32, f32); 4] {
        let (dx, dy) = (x1 - x0, y1 - y0);
        let l = sqrt(dx * dx + dy * dy).max(1e-3);
        let (nx, ny) = (-dy / l * w, dx / l * w);
        [(x0 + nx, y0 + ny), (x1 + nx, y1 + ny), (x1 - nx, y1 - ny), (x0 - nx, y0 - ny)]
    }
    match k {
        Kind::Jet | Kind::Heavy | Kind::Quad => {
            let fw = if k == Kind::Jet { 0.09 } else { 0.12 };
            let span = if k == Kind::Jet { 0.88 } else { 0.98 };
            part(&[(0.0, -1.0), (fw * 0.9, -0.86), (fw, 0.7), (0.035, 0.98), (-0.035, 0.98), (-fw, 0.7), (-fw * 0.9, -0.86)], false);
            part(&[(fw, -0.22), (span, 0.26), (span, 0.36), (fw, 0.12)], true);
            part(&[(0.06, 0.66), (0.38, 0.86), (0.38, 0.94), (0.05, 0.87)], true);
            let ew = if k == Kind::Jet { 0.06 } else { 0.075 };
            let engines: &[f32] = if k == Kind::Quad { &[0.33, 0.64] } else { &[0.36] };
            for &ex in engines {
                let ey = -0.22 + (ex - fw) / (span - fw) * 0.48;
                part(&[(ex - ew, ey - 0.2), (ex + ew, ey - 0.2), (ex + ew, ey + 0.04), (ex - ew, ey + 0.04)], true);
            }
        }
        Kind::Bizjet => {
            part(&[(0.0, -1.0), (0.08, -0.84), (0.09, 0.62), (0.03, 0.98), (-0.03, 0.98), (-0.09, 0.62), (-0.08, -0.84)], false);
            part(&[(0.08, -0.1), (0.78, 0.18), (0.78, 0.27), (0.08, 0.16)], true);
            part(&[(0.11, 0.38), (0.25, 0.38), (0.25, 0.66), (0.11, 0.66)], true);
            part(&[(0.03, 0.82), (0.4, 0.9), (0.4, 0.98), (0.03, 0.96)], true);
        }
        Kind::Twinprop => {
            part(&[(0.0, -0.95), (0.08, -0.8), (0.08, 0.7), (0.03, 0.98), (-0.03, 0.98), (-0.08, 0.7), (-0.08, -0.8)], false);
            part(&[(0.08, -0.2), (0.98, -0.16), (0.98, -0.03), (0.08, 0.02)], true);
            part(&[(0.27, -0.5), (0.39, -0.5), (0.39, 0.12), (0.27, 0.12)], true);
            part(&[(0.1, -0.57), (0.56, -0.57), (0.56, -0.51), (0.1, -0.51)], true);
            part(&[(0.05, 0.74), (0.38, 0.77), (0.38, 0.9), (0.04, 0.93)], true);
        }
        Kind::Light => {
            part(&[(0.0, -0.9), (0.1, -0.82), (0.1, -0.1), (0.04, 0.9), (-0.04, 0.9), (-0.1, -0.1), (-0.1, -0.82)], false);
            part(&[(0.1, -0.42), (1.0, -0.42), (1.0, -0.2), (0.1, -0.2)], true);
            part(&[(0.04, 0.72), (0.36, 0.72), (0.36, 0.88), (0.04, 0.9)], true);
            part(&[(0.0, -1.0), (0.34, -1.0), (0.34, -0.94), (0.0, -0.94)], true);
        }
        Kind::Glider => {
            part(&[(0.0, -0.95), (0.06, -0.8), (0.04, 0.95), (-0.04, 0.95), (-0.06, -0.8)], false);
            part(&[(0.05, -0.3), (1.0, -0.24), (1.0, -0.17), (0.05, -0.17)], true);
            part(&[(0.03, 0.82), (0.3, 0.84), (0.3, 0.91), (0.03, 0.92)], true);
        }
        Kind::Heli => {
            part(&[(0.0, -0.62), (0.2, -0.52), (0.26, -0.15), (0.14, 0.18), (0.0, 0.22), (-0.14, 0.18), (-0.26, -0.15), (-0.2, -0.52)], false);
            part(&[(0.045, 0.1), (0.045, 0.92), (-0.045, 0.92), (-0.045, 0.1)], false);
            part(&[(0.045, 0.7), (0.11, 0.7), (0.11, 1.0), (0.045, 1.0)], false);
            part(&bar(-0.72, -0.9, 0.72, 0.5, 0.045), false);
            part(&bar(0.72, -0.9, -0.72, 0.5, 0.045), false);
        }
        Kind::Balloon => {
            // envelope: circle r 0.62 around (0, -0.3), every 36 degrees
            const RING: [(f32, f32); 10] = [
                (0.0, -0.92), (0.364, -0.802), (0.59, -0.492), (0.59, -0.108), (0.364, 0.202),
                (0.0, 0.32), (-0.364, 0.202), (-0.59, -0.108), (-0.59, -0.492), (-0.364, -0.802),
            ];
            part(&RING, false);
            part(&[(0.0, 0.3), (0.3, 0.22), (0.12, 0.62), (0.0, 0.62)], true);
            part(&[(0.16, 0.7), (0.16, 0.95), (-0.16, 0.95), (-0.16, 0.7)], false);
        }
        Kind::Drone => {
            part(&bar(-0.6, -0.6, 0.6, 0.6, 0.07), false);
            part(&bar(0.6, -0.6, -0.6, 0.6, 0.07), false);
            for &(x, y) in &[(0.6f32, -0.6f32), (0.6, 0.6)] {
                part(&[(x - 0.28, y - 0.28), (x + 0.28, y - 0.28), (x + 0.28, y + 0.28), (x - 0.28, y + 0.28)], true);
            }
            part(&[(0.2, -0.2), (0.2, 0.2), (-0.2, 0.2), (-0.2, -0.2)], false);
        }
        Kind::Unknown => {}
    }
}

pub const ZOOMS_NM: [u8; 3] = [5, 10, 25];

/// One aircraft as the map draws it.
#[derive(Clone, Copy)]
pub struct Blip {
    pub e: f32,
    pub n: f32,
    pub track: f32, // rad, or NAN-free: only used when has_track
    pub has_track: bool,
    pub alt: i32,
    pub sel: bool,
    pub idx: u8,
}

/// Map, centred on the watch. Heading up when there is a heading, else north
/// up. Range ring at `zoom_nm`, aircraft as darts along their track with
/// altitude in hundreds of feet; the selected one larger with its callsign.
/// Pass `blips` nearest first: where labels would collide the nearer keeps it.
#[inline(never)] // keeps the paint frame small (UI task stack)
pub fn paint_map(d: &mut dyn Draw, blips: &[Blip], acs: &[Ac], heading: Option<f32>, zoom_nm: u8) {
    let (hs, _) = d.metrics(SMALL);
    let (cx, cy) = (64, 66);
    let r = 50.0;
    let scale = r / (zoom_nm as f32 * M_PER_NM);
    let h = heading.unwrap_or(0.0);
    let (sh, ch) = (sin(h), cos(h));

    dots(d, cx, cy, r, 10, 2);
    dots(d, cx, cy, r * 0.5, 30, 1);
    // north mark on the ring
    let nx = cx + round(r * sin(-h));
    let ny = cy - round(r * cos(-h));
    d.rect(false, nx - 5, ny - hs / 2 - 1, 11, hs + 1);
    d.text(SMALL, nx, ny - hs / 2, 1, b"N");
    // you: a small dart facing 12 o'clock
    plane_icon(d, cx, cy, 0.0, 7.0);
    d.rect(true, cx - 1, cy - 1, 3, 3);

    // labels already placed (x, y, w, h), so later ones can step aside
    // (i16 and u8 here keep this frame small: it runs on the UI task stack)
    // the first entry is you, so no label sits on your marker
    let mut taken = [(0i16, 0i16, 0i16, 0i16); 16];
    taken[0] = ((cx - 5) as i16, (cy - 6) as i16, 11, 12);
    let mut nt = 1usize;
    let free = |t: &[(i16, i16, i16, i16)], x: i32, y: i32, w: i32, h: i32| {
        t.iter().all(|&(a, b, c, e)| {
            let (a, b, c, e) = (a as i32, b as i32, c as i32, e as i32);
            x + w <= a || a + c <= x || y + h <= b || b + e <= y
        })
    };
    // the selected one last so it stays on top; its label first so it wins
    let mut order = [0u8; MAX_AC];
    let mut no = 0;
    for pass in 0..2 {
        for (i, b) in blips.iter().enumerate() {
            if b.sel == (pass == 1) && no < order.len() {
                order[no] = i as u8;
                no += 1;
            }
        }
    }
    let mut sel_label: Option<(i32, i32, Buf)> = None;
    let mut drawn = 0;
    for &i in &order[..no] {
        let Some(b) = blips.get(i as usize) else { continue };
        let x = b.e * ch - b.n * sh;
        let y = b.e * sh + b.n * ch;
        let (px, py) = (cx + round(x * scale), cy - round(y * scale));
        let rr = ((px - cx) * (px - cx) + (py - cy) * (py - cy)) as f32;
        if rr > (r + 1.0) * (r + 1.0) {
            continue;
        }
        drawn += 1;
        let ang = if b.has_track { b.track - h } else { 0.0 };
        if b.sel {
            plane_icon(d, px, py, ang, 14.0);
            if let Some(a) = acs.get(b.idx as usize) {
                let mut l = Buf::new();
                if a.flight[0] != 0 {
                    l.t(&a.flight);
                } else {
                    l.t(&a.reg);
                }
                // right of the glyph, else left, below, above; none if
                // nothing fits the window (the card shows the callsign)
                let w = d.width(SMALL, l.s());
                let spots = [(px + 9, py - hs / 2), (px - 9 - w, py - hs / 2), (px - w / 2, py + 8), (px - w / 2, py - 8 - hs)];
                let spot = spots.into_iter().find(|&(x, y)| box_ok(x - 1, y, x + w + 1, y + hs) && free(&taken[..nt], x - 1, y, w + 2, hs));
                if let Some((lx, ly)) = spot {
                    sel_label = Some((lx, ly, l));
                    if nt < taken.len() {
                        taken[nt] = ((lx - 1) as i16, ly as i16, (w + 2) as i16, hs as i16);
                        nt += 1;
                    }
                }
            }
        } else {
            plane_icon(d, px, py, ang, 9.0);
        }
    }
    // altitude labels, nearest first wins a spot; skipped where they would
    // overlap another label
    for &i in &order[..no] {
        let Some(b) = blips.get(i as usize) else { continue };
        if b.sel || b.alt == ALT_NONE {
            continue;
        }
        let x = b.e * ch - b.n * sh;
        let y = b.e * sh + b.n * ch;
        let (px, py) = (cx + round(x * scale), cy - round(y * scale));
        let rr = ((px - cx) * (px - cx) + (py - cy) * (py - cy)) as f32;
        if rr > (r + 1.0) * (r + 1.0) {
            continue;
        }
        let mut l = Buf::new();
        l.int((b.alt.max(0) + 50) / 100, false);
        let w = d.width(SMALL, l.s());
        let (lx, ly) = (px + 5, py + 1);
        // keep the label inside the ring and clear of the range label
        let (fx, fy) = ((lx + w - cx) as f32, (ly + hs - 2 - cy) as f32);
        if fx * fx + fy * fy > (r + 2.0) * (r + 2.0) || ly + hs > cy + round(r) - hs / 2 || !free(&taken[..nt], lx, ly, w, hs - 2) {
            continue;
        }
        d.text(SMALL, lx, ly, 0, l.s());
        if nt < taken.len() {
            taken[nt] = (lx as i16, ly as i16, w as i16, (hs - 2) as i16);
            nt += 1;
        }
    }
    if drawn == 0 {
        text_c(d, SMALL, cy + 12, b"no traffic");
    }
    if let Some((lx, ly, l)) = sel_label {
        let w = d.width(SMALL, l.s());
        d.rect(false, lx - 1, ly, w + 2, hs);
        d.text(SMALL, lx, ly, 0, l.s());
    }

    // range label under the ring
    let mut z = Buf::new();
    z.int(zoom_nm as i32, false).t(b" nm");
    let y = cy + round(r) - hs / 2;
    let w = d.width(SMALL, z.s());
    d.rect(false, cx - w / 2 - 2, y, w + 4, hs);
    d.text(SMALL, cx, y, 1, z.s());
}
