// Fuzz for navcore.rs: random and hostile phone text (long, no NUL, NUL inside,
// invalid UTF-8, huge numbers, separators and units in odd places) through
// every function, with overflow checks on. Checks the output invariants and,
// for well-formed distances, the value against a float reference.
// Build: rustc +stable-x86_64-pc-windows-gnu -O -C debug-assertions=on -C overflow-checks=on --edition 2021 navfuzz.rs
// Usage: navfuzz N [SEED]
#[path = "../src/navcore.rs"]
#[allow(dead_code)]
mod navcore;
use navcore::*;

const PIECES: &[&[u8]] = &[
    b"0", b"1", b"9", b"350", b"99999999999999999999", b"4294967296", b".", b",", b",000", b",5",
    b" ", b"  ", b"m", b"km", b"KM", b"mi", b"ft", b"yd", b"Yd", b"\n", b"\n\n", b"ETA", b"eta:",
    b":", b"%s", b"%n", b"\0", b"\xe2\x86\x91", b"\xe2\x86", b"\x86", b"\xc3\xa9", b"\xc3",
    b"\xf0\x9f\x9a\x97", b"\xff", b"\xfe", b"\x80\x80\x80", b"Turn left onto ", b"Main Street",
    b"gadgetbridge.nav.v1.", b"gadgetbridge.nav.v1.18", b"Keep right", b"towards", b"-", b"+",
];

/// Draw that checks every inked shape stays inside the visible circle and
/// clear of the inset, with fake fonts of a given cell height and char width.
struct Check {
    h: [i32; 2],
    cw: [i32; 2],
    errs: Vec<String>,
}

fn inside(x: f64, y: f64) -> bool {
    let (dx, dy) = (x - 64.0, y - 64.0);
    dx * dx + dy * dy <= 70.5 * 70.5
}

fn in_inset(x: f64, y: f64) -> bool {
    let (dx, dy) = (x - INSET_X as f64 - 0.5, y - INSET_Y as f64 - 0.5);
    dx * dx + dy * dy < (INSET_R as f64 - 0.5) * (INSET_R as f64 - 0.5)
}

impl Check {
    fn bx(&mut self, what: &str, x: i32, y: i32, w: i32, h: i32) {
        if w <= 0 || h <= 0 {
            return;
        }
        let (x0, y0, x1, y1) = (x as f64 + 0.5, y as f64 + 0.5, (x + w) as f64 - 0.5, (y + h) as f64 - 0.5);
        let corners_in = [(x0, y0), (x1, y0), (x0, y1), (x1, y1)].iter().all(|&(a, b)| inside(a, b));
        // closest point of the box to the inset centre
        let cx = (INSET_X as f64 + 0.5).clamp(x0, x1);
        let cy = (INSET_Y as f64 + 0.5).clamp(y0, y1);
        if !corners_in || in_inset(cx, cy) {
            self.errs.push(format!("{what} {x},{y} {w}x{h}"));
        }
    }
}

impl Draw for Check {
    fn rect(&mut self, ink: bool, x: i32, y: i32, w: i32, h: i32) {
        if ink {
            self.bx("rect", x, y, w, h);
        }
    }
    fn poly(&mut self, ink: bool, pts: &[i16]) {
        if !ink {
            return;
        }
        let n = pts.len() / 2;
        for i in 0..n {
            let (ax, ay) = (pts[2 * i] as f64, pts[2 * i + 1] as f64);
            let j = (i + 1) % n;
            let (bx, by) = (pts[2 * j] as f64, pts[2 * j + 1] as f64);
            for k in 0..=8 {
                let t = k as f64 / 8.0;
                let (x, y) = (ax + (bx - ax) * t, ay + (by - ay) * t);
                if !inside(x + 0.5, y + 0.5) || in_inset(x + 0.5, y + 0.5) {
                    self.errs.push(format!("poly point {x:.0},{y:.0}"));
                    return;
                }
            }
        }
    }
    fn text(&mut self, font: u8, x: i32, top: i32, align: u32, s: &[u8]) {
        let w = self.width(font, s);
        let x0 = if align == 1 { x - w / 2 } else { x };
        let h = self.h[font as usize & 1];
        self.bx("text", x0, top, w, h);
    }
    fn width(&mut self, font: u8, s: &[u8]) -> i32 {
        self.cw[font as usize & 1] * s.len() as i32
    }
    fn metrics(&mut self, font: u8) -> (i32, i32) {
        let h = self.h[font as usize & 1];
        (h, h * 3 / 4)
    }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let n: u64 = a.get(1).and_then(|x| x.parse().ok()).unwrap_or(1_000_000);
    let mut seed: u64 = a.get(2).and_then(|x| x.parse().ok()).unwrap_or(0x9e37_79b9_7f4a_7c15);
    let mut rnd = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let mut bad = 0u64;
    let mut checked = 0u64;
    let mut longest = 0usize;
    for i in 0..n {
        // build a string: random bytes, pieces, or a mix; sometimes very long
        let mut s: Vec<u8> = Vec::new();
        let kind = rnd() % 4;
        let len = match rnd() % 10 {
            0 => (rnd() % 5000) as usize,
            1..=3 => (rnd() % 200) as usize,
            _ => (rnd() % 40) as usize,
        };
        while s.len() < len {
            match kind {
                0 => s.push(rnd() as u8),
                1 => s.extend_from_slice(PIECES[(rnd() % PIECES.len() as u64) as usize]),
                _ => {
                    if rnd() % 3 == 0 {
                        s.push(rnd() as u8)
                    } else {
                        s.extend_from_slice(PIECES[(rnd() % PIECES.len() as u64) as usize])
                    }
                }
            }
        }
        longest = longest.max(s.len());
        let valid = std::str::from_utf8(&s).is_ok();

        let _ = parse_action(&s);
        let _ = band(metres(&s));
        let h = hash(rnd() as u32, &s);
        let (_, f) = decide(rnd() as u8, rnd() as u8, h, rnd() as u8);
        if f & F_SHOWN == 0 {
            println!("#{i}: decide lost F_SHOWN");
            bad += 1;
        }
        let y = (rnd() % 400) as i32 - 200;
        let hw = halfw(y);
        if !(0..=VIS_R).contains(&hw) || row_width(y, y + (rnd() % 40) as i32) < 0 {
            println!("#{i}: halfw({y}) = {hw}");
            bad += 1;
        }
        if let Some((num, unit)) = split_dist(&s) {
            if num.is_empty() || num.len() > 7 || unit.len() > 4 || digits_width(num, 15, 4, 3) > 7 * 18 {
                println!("#{i}: split_dist out of bounds");
                bad += 1;
            }
        }
        for k in [0usize, 1, 2, 7, 1000] {
            let l = line(&s, k);
            if l.contains(&b'\n') || l.len() > s.len() {
                println!("#{i}: line {k} bad");
                bad += 1;
            }
        }
        let max_cut = (rnd() % 64) as usize;
        let c = utf8_cut(&s, max_cut);
        let cut_ok = if s.len() <= max_cut { c == s.len() } else { c <= max_cut };
        if !cut_ok {
            println!("#{i}: utf8_cut {c} for max {max_cut} len {}", s.len());
            bad += 1;
        }
        if valid && std::str::from_utf8(&s[..c]).is_err() {
            println!("#{i}: utf8_cut split a character");
            bad += 1;
        }
        let e = eta(&s);
        let l = left(&s);
        if e.len() > 8 || l.len() > 8 {
            println!("#{i}: eta/left too long");
            bad += 1;
        }
        if valid && (std::str::from_utf8(e).is_err() || std::str::from_utf8(l).is_err()) {
            println!("#{i}: eta/left split a character");
            bad += 1;
        }
        let d = dist_prefix(&s);
        if !d.is_empty() && (metres(d) != metres(&s) || !trim_lead(&s).starts_with(d)) {
            println!("#{i}: dist_prefix {d:?} disagrees with metres");
            bad += 1;
        }
        if valid && std::str::from_utf8(d).is_err() {
            println!("#{i}: dist_prefix split a character");
            bad += 1;
        }
        let ins = line(&s, 0);
        let cw = (rnd() % 9) as i32; // 0..8 px per byte, 0 = everything fits
        let mut calls = 0usize;
        let mut longest = 0usize;
        let nl = (rnd() % 4) as usize;
        let ws = [(rnd() % 140) as i32, (rnd() % 140) as i32, (rnd() % 140) as i32];
        let r = wrap_lines(ins, &ws[..nl], 2 * cw, &mut |t: &[u8]| {
            calls += 1;
            longest = longest.max(t.len());
            cw * t.len() as i32
        });
        let mut ok = r.n <= nl;
        let mut prev = 0;
        for k in 0..r.n {
            ok &= r.start[k] >= prev && r.start[k] <= r.end[k] && r.end[k] <= ins.len();
            ok &= k == 0 || r.start[k] > r.start[k - 1]; // every line makes progress
            prev = r.end[k];
        }
        if !ok {
            println!("#{i}: wrap_lines out of bounds {:?} {:?} n {} of {}", r.start, r.end, r.n, ins.len());
            bad += 1;
        }
        if longest > MEASURE_MAX || calls > 12 * MEASURE_MAX + 12 {
            println!("#{i}: wrap_lines measured {longest} bytes, {calls} calls");
            bad += 1;
        }
        if valid && (0..r.n).any(|k| std::str::from_utf8(&ins[r.start[k]..r.end[k]]).is_err()) {
            println!("#{i}: wrap_lines split a character");
            bad += 1;
        }
        if r.dots && r.n != nl {
            println!("#{i}: wrap_lines dots before the last line");
            bad += 1;
        }

        // record time and delete rules
        let (fl, t, now) = (rnd() as u8, rnd() as u16, rnd() as u16 & TIME_MASK);
        let (f2, lo) = with_time(fl, t);
        let tt = time_of(f2, lo);
        if f2 & !F_TIME != fl & !F_TIME || (tt != t & TIME_MASK && !(t & TIME_MASK == 0 && tt == 1)) || tt == 0 {
            println!("#{i}: with_time {fl:#x} {t:#x} -> {f2:#x} {lo:#x}");
            bad += 1;
        }
        if (fl & F_HELD != 0 && hold_delete(fl, lo, now)) || (held_done(fl, lo, now) && fl & F_HELD == 0) {
            println!("#{i}: delete rules {fl:#x} {lo:#x} {now:#x}");
            bad += 1;
        }
        let (_, f3) = decide(f2, rnd() as u8, rnd() as u8, rnd() as u8);
        if time_of(f3, lo) != tt || f3 & F_HELD != f2 & F_HELD {
            println!("#{i}: decide changed time or held");
            bad += 1;
        }

        // the whole screen: shapes inside the circle, clear of the inset
        let hs = 8 + (rnd() % 12) as i32;
        let hm = hs + (rnd() % 10) as i32;
        let cs = 3 + (rnd() % 6) as i32;
        let mut ck = Check { h: [hs, hm], cw: [cs, cs + (rnd() % 4) as i32], errs: Vec::new() };
        let title: Vec<u8> = match rnd() % 3 {
            0 => format!("{}{}{}", rnd() % 10u64.pow((rnd() % 9) as u32), [" m", " km", "ft", " mi", ",5 km", ".25 mi"][(rnd() % 6) as usize], "").into_bytes(),
            1 => s.iter().take(30).copied().collect(),
            _ => Vec::new(),
        };
        let action = (rnd() % 20) as u32;
        paint(&mut ck, action, &title, &s);
        if !ck.errs.is_empty() {
            println!("#{i}: paint a{action} fonts {hs}/{hm} cw {:?} title {:?}: {}", ck.cw, String::from_utf8_lossy(&title), ck.errs.join("; "));
            bad += 1;
        }

        // well-formed distance against a float reference
        let whole = rnd() % 100_000;
        let fd = rnd() % 4; // fraction digits
        let frac = rnd() % 10u64.pow(fd as u32);
        let units: [(&str, f64); 6] = [("m", 1.0), ("km", 1000.0), ("mi", 1609.0), ("ft", 0.3048), ("yd", 0.9144), ("KM", 1000.0)];
        let (u, per) = units[(rnd() % 6) as usize];
        let sep = if rnd() % 2 == 0 { "." } else { "," };
        let sp = if rnd() % 2 == 0 { " " } else { "" };
        // a comma with exactly 3 digits is a thousands group, skip that case
        let txt = if fd == 0 {
            format!("{whole}{sp}{u}")
        } else if fd == 3 && sep == "," {
            format!("{whole}.{frac:03}{sp}{u}")
        } else {
            format!("{whole}{sep}{frac:0w$}{sp}{u}", w = fd as usize)
        };
        let want = (whole as f64 + frac as f64 / 10f64.powi(fd as i32)) * per;
        match metres(txt.as_bytes()) {
            Some(got) if (got as f64 - want).abs() <= 1.0 + want * 1e-6 => {}
            got => {
                println!("#{i}: metres({txt:?}) = {got:?}, want about {want:.1}");
                bad += 1;
            }
        }
        checked += 1;
        if bad > 20 {
            break;
        }
    }
    println!("{checked} cases, longest input {longest} bytes, {bad} failures, 0 panics (a panic aborts the run)");
    if bad != 0 {
        std::process::exit(1);
    }
}
