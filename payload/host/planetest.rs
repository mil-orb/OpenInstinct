// Host tests for planecore (scanner, math, matching, layout bounds).
// Build: rustc +stable-x86_64-pc-windows-gnu -O -C debug-assertions=on -C overflow-checks=on --edition 2021 planetest.rs
// Run: planetest [reply.json ...]   (saved adsb.lol replies; optional)
#[path = "../src/planecore.rs"]
#[allow(dead_code)]
mod planecore;
use planecore::*;

struct Null {
    calls: usize,
    oob: usize,
}
impl Draw for Null {
    fn rect(&mut self, _: bool, x: i32, y: i32, w: i32, h: i32) {
        self.calls += 1;
        if x < -20 || y < -20 || x + w > 148 || y + h > 148 {
            self.oob += 1;
        }
    }
    fn poly(&mut self, _: bool, p: &[i16]) {
        self.calls += 1;
        assert!(p.len() % 2 == 0 && p.len() <= 16);
    }
    fn text(&mut self, _: u8, x: i32, top: i32, _: u32, s: &[u8]) {
        self.calls += 1;
        assert!(s.len() <= 32);
        if !(-10..=138).contains(&x) || !(-10..=128).contains(&top) {
            self.oob += 1;
        }
    }
    fn width(&mut self, _: u8, s: &[u8]) -> i32 {
        s.len() as i32 * 7
    }
    fn metrics(&mut self, f: u8) -> (i32, i32) {
        if f == MED {
            (16, 13)
        } else {
            (13, 10)
        }
    }
}

fn ac(lat: f64, lon: f64, alt: i32, trk: u16, gs: u16, name: &str) -> Ac {
    let mut a = Ac::ZERO;
    a.lat = (lat * 1e6) as i32;
    a.lon = (lon * 1e6) as i32;
    a.alt = alt;
    a.track = trk;
    a.gs = gs;
    a.flags = F_POS;
    for (i, c) in name.bytes().take(8).enumerate() {
        a.flight[i] = c;
    }
    a
}

fn lcg(s: &mut u64) -> u64 {
    *s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    *s >> 33
}

fn main() {
    let mut fails = 0;
    macro_rules! check {
        ($c:expr, $($m:tt)*) => {
            if !$c {
                fails += 1;
                println!("FAIL: {}", format!($($m)*));
            }
        };
    }

    // ---- parse_fixed
    check!(parse_fixed(b"38.667755", 6) == Some(38_667_755), "lat");
    check!(parse_fixed(b"-9.359211", 6) == Some(-9_359_211), "lon");
    check!(parse_fixed(b"82.25", 1) == Some(823), "round half up");
    check!(parse_fixed(b"-82.25", 1) == Some(-823), "round half away");
    check!(parse_fixed(b"37975", 0) == Some(37975), "int");
    check!(parse_fixed(b"0.288", 1) == Some(3), "seen_pos");
    check!(parse_fixed(b"null", 0).is_none(), "null");
    check!(parse_fixed(b"1e5", 0).is_none(), "exp");
    check!(parse_fixed(b"-", 0).is_none(), "lone minus");
    check!(parse_fixed(b"99999999999999999999", 0) == Some(i32::MAX), "clamp");

    // ---- scanner on a hand-made reply
    let j = br#"{"ac":[{"hex":"a2b55d","type":"adsr_icao","flight":"N274AE  ","r":"N274AE","t":"B06",
      "alt_baro":1625,"alt_geom":1750,"gs":82.2,"track":80.19,"baro_rate":128,"category":"A7",
      "lat":38.667755,"lon":-9.359211,"seen_pos":0.982,"mlat":[],"tisb":["a",{"x":[1,2]}],"dst":10.254},
      {"hex":"~abc123","alt_baro":3000,"lat":38.6,"lon":-9.09,"emergency":"general","dbFlags":1},
      {"hex":"a00003","alt_baro":"ground","lat":38.6,"lon":-9.09},
      {"hex":"a00001","alt_geom":5000,"lat":null,"lon":-9.09},
      {"hex":"a00002","lastPosition":{"lat":1,"lon":2},"flight":"X\"Y"}],
      "msg":"No error","now":1762560000000,"total":4}"#;
    let mut out = [Ac::ZERO; MAX_AC];
    let n = scan(j, &mut out);
    check!(n == Some(2), "scan count {:?}", n);
    let a = &out[0];
    check!(a.hex == 0xa2b55d, "hex {:x}", a.hex);
    check!(&a.flight[..6] == b"N274AE" && a.flight[6] == 0, "flight trim");
    check!(&a.typ == b"B06\0", "type");
    check!(a.alt == 1625 && a.gs == 822 && a.track == 802 && a.rate == 128, "numbers");
    check!(a.cat == 0xA7 && a.age == 10 && a.dst == 1025, "cat/age/dst {:x} {} {}", a.cat, a.age, a.dst);
    let b = &out[1];
    check!(b.hex == 0xabc123 | 1 << 24 && b.alt == 3000, "non-icao hex {:x}", b.hex);
    check!(!out[..2].iter().any(|a| a.hex == 0xa00003), "ground traffic dropped in scan");
    check!(b.flags & F_EMERG != 0 && b.flags & F_MIL != 0, "flags");
    check!(scan(br#"{"msg":"bad"}"#, &mut out).is_none(), "no ac");
    check!(scan(b"", &mut out).is_none(), "empty");
    check!(scan(b"<html>", &mut out).is_none(), "html");

    // every prefix of the reply: never panics, never more than the full count
    for k in 0..j.len() {
        if let Some(m) = scan(&j[..k], &mut out) {
            check!(m <= 2, "prefix {k} gave {m}");
        }
    }
    // random mutations
    let mut seed = 1u64;
    let mut buf = j.to_vec();
    for _ in 0..200_000 {
        buf.copy_from_slice(j);
        for _ in 0..(lcg(&mut seed) % 6 + 1) {
            let p = (lcg(&mut seed) as usize) % buf.len();
            buf[p] = b"{}[]\",:0123456789.-~ ae\\\0\xff"[(lcg(&mut seed) % 26) as usize];
        }
        let _ = scan(&buf, &mut out);
    }
    // keep-nearest when there are more than MAX_AC
    let mut big = String::from("{\"ac\":[");
    for i in 0..100 {
        big += &format!("{{\"hex\":\"{:06x}\",\"lat\":38.{:03},\"lon\":-9.09,\"dst\":{}.5}},", i, i, 100 - i);
    }
    big.pop();
    big += "]}";
    let n = scan(big.as_bytes(), &mut out).unwrap_or(0);
    let maxd = out[..n].iter().map(|a| a.dst).max().unwrap_or(0);
    check!(n == MAX_AC && maxd <= 4050, "keep nearest n={n} max dst={maxd}");

    // ---- saved replies
    for path in std::env::args().skip(1) {
        let body = std::fs::read(&path).expect("read reply");
        let n = scan(&body, &mut out);
        println!("{path}: {} bytes -> {:?} aircraft", body.len(), n);
        for a in &out[..n.unwrap_or(0)] {
            println!(
                "  {:06x} {:8} {:4} {:7} alt {:6} gs {:4} trk {:5} lat {} lon {} age {} dst {}",
                a.hex,
                String::from_utf8_lossy(&a.flight[..flen(&a.flight)]),
                String::from_utf8_lossy(&a.typ[..flen(&a.typ)]),
                String::from_utf8_lossy(&a.reg[..flen(&a.reg)]),
                a.alt,
                a.gs,
                a.track,
                a.lat,
                a.lon,
                a.age,
                a.dst
            );
        }
    }

    // ---- math
    let mut worst = (0f32, 0f32, 0f32, 0f32);
    let mut x = -20.0f32;
    while x < 20.0 {
        worst.0 = worst.0.max((sin(x) - x.sin()).abs());
        worst.1 = worst.1.max((cos(x) - x.cos()).abs());
        x += 0.001;
    }
    for i in 0..2000 {
        let t = i as f32 * 0.00314;
        for r in [0.001f32, 1.0, 1000.0] {
            let (y, xx) = (r * t.sin(), r * t.cos());
            worst.2 = worst.2.max((atan2(y, xx) - y.atan2(xx)).abs());
            worst.2 = worst.2.max((atan2(-y, xx) - (-y).atan2(xx)).abs());
        }
    }
    for i in 1..10000 {
        let v = i as f32 * 37.3;
        worst.3 = worst.3.max((sqrt(v) - v.sqrt()).abs() / v.sqrt());
    }
    println!("max err sin {:.1e} cos {:.1e} atan2 {:.1e} sqrt(rel) {:.1e}", worst.0, worst.1, worst.2, worst.3);
    check!(worst.0 < 1e-4 && worst.1 < 1e-4 && worst.2 < 1e-4 && worst.3 < 1e-5, "math accuracy");
    check!(wrap(f32::NAN) == 0.0 && sqrt(f32::NAN) == 0.0 && sqrt(-1.0) == 0.0, "nan safety");
    check!(atan2(0.0, 0.0) == 0.0, "atan2 0");

    // ---- geometry: 1 nm north, 1 nm east of Lisbon
    let own = Own::new(38_670_000, -9_140_000);
    let n1 = ac(38.67 + 1.0 / 60.0, -9.14, 3000, NO_TRACK, 0, "N");
    let (e, nn) = enu(&own, &n1, 0.0);
    check!((nn - 1853.0).abs() < 5.0 && e.abs() < 1.0, "north {e} {nn}");
    let e1 = ac(38.67, -9.14 + 1.0 / 60.0 / 0.7815, 3000, NO_TRACK, 0, "E");
    let (e, nn) = enu(&own, &e1, 0.0);
    check!((e - 1853.0).abs() < 10.0 && nn.abs() < 1.0, "east {e} {nn}");
    // moving east at 360 kt for 10 s: +1852 m
    let mv = ac(38.67, -9.14, 3000, 900, 3600, "MV");
    let (e, _) = enu(&own, &mv, 10.0);
    check!((e - 1852.0).abs() < 2.0, "dead reckoning {e}");

    // ---- matching
    let deg = |d: f32| d * DEG;
    let list = [
        ac(38.70, -9.14, 30000, 900, 4500, "NORTH"), // ~1.8 nm N
        ac(38.67, -9.09, 8000, 0, 2500, "EAST"),     // ~2.3 nm E
        ac(38.67, -9.07, 35000, 0, 4500, "EAST2"),   // ~3.3 nm E, same line
        ac(38.60, -9.14, 0, 0, 0, "LOW"),            // S, alt 0
        ac(38.67, -9.12, ALT_GROUND, 0, 0, "GND"),   // E but on the ground
        ac(39.40, -9.14, 2000, 0, 1000, "FAR"),      // 44 nm N, too far to see
    ];
    let mut cands = [Cand::ZERO; MAX_AC];
    let (n, cone) = rank(&list, &own, 0.0, Some(deg(92.0)), deg(3.0), &mut cands);
    let name = |c: &Cand| String::from_utf8_lossy(&list[c.idx as usize].flight[..flen(&list[c.idx as usize].flight)]).to_string();
    println!("aim 92: {:?}", cands[..n].iter().map(|c| (name(c), c.cone, (c.err / DEG) as i32)).collect::<Vec<_>>());
    check!(cone == 2 && name(&cands[0]) == "EAST" && name(&cands[1]) == "EAST2", "east pair, nearer first");
    check!(!cands[..n].iter().any(|c| name(c) == "GND" || name(c) == "FAR"), "ground and far dropped");
    let (_, cone) = rank(&list, &own, 0.0, Some(deg(-5.0)), 0.0, &mut cands);
    check!(cone == 1 && name(&cands[0]) == "NORTH", "north");
    let (n, cone) = rank(&list, &own, 0.0, Some(deg(135.0)), 0.0, &mut cands);
    check!(cone == 0 && n >= 1, "nothing in the cone at 135, {n} near misses");
    // heading wrap: aim 359 finds the plane at 001
    let wrapl = [ac(38.70, -9.139, 30000, 0, 0, "W")];
    let (_, cone) = rank(&wrapl, &own, 0.0, Some(deg(359.0)), 0.0, &mut cands);
    check!(cone == 1, "wrap");
    // no compass: by distance over visibility
    let (n, cone) = rank(&list, &own, 0.0, None, 0.0, &mut cands);
    check!(n >= 3 && cone == 0, "no compass ranking");

    // ---- circular mean across north
    let samples = [deg(355.0), deg(5.0), deg(358.0), deg(2.0)];
    let (ss, sc) = samples.iter().fold((0.0, 0.0), |(s, c), &a| (s + sin(a), c + cos(a)));
    let (h, sp) = circ_mean(ss, sc, 4).unwrap();
    check!(wrap(h).abs() < deg(0.5) && sp < deg(5.0), "circ mean {} {}", h / DEG, sp / DEG);
    check!(circ_mean(0.0, 0.0, 4).is_none(), "circ mean spread out");

    // ---- text
    let mut b = Buf::new();
    b.int(35000, true).c(b' ').int(-1234567, true).c(b' ').tenths(123).c(b' ').tenths(5).c(b' ').tenths(1234);
    check!(b.s() == b"35,000 -1,234,567 12.3 0.5 123", "buf {:?}", String::from_utf8_lossy(b.s()));
    let mut b = Buf::new();
    b.int(i32::MIN, true);
    check!(b.s() == b"-2,147,483,648", "int min");
    check!(type_name(b"B38M") == Some(&b"737 MAX 8"[..]) && type_name(b"B06\0") == Some(&b"Bell 206"[..]), "type names");
    check!(type_name(b"ZZZZ").is_none(), "unknown type");

    // ---- layouts stay on screen for awkward inputs
    let mut d = Null { calls: 0, oob: 0 };
    paint_status(&mut d, b"Hold steady", b"Aim 12 o'clock", b"at the plane", Inset::Spin(3), Some(1));
    let mut weird = ac(38.7, -9.09, -1200, 3599, 65535, "WWWWWWWW");
    weird.reg = *b"MMMMMMMM";
    weird.typ = *b"WWWW";
    weird.rate = -32000;
    weird.flags |= F_RATE | F_EMERG | F_MIL;
    for h in [None, Some(0.0), Some(3.0)] {
        paint_card(&mut d, &CardInfo { ac: &weird, dist_m: 1e9, brg: 1.0, heading: h, index: 39, count: 40, in_cone: false, err: 1.0 });
    }
    let blips: Vec<Blip> = (0..40)
        .map(|i| Blip { e: (i as f32 - 20.0) * 2000.0, n: (i % 7) as f32 * 3000.0 - 9000.0, track: i as f32, has_track: i % 2 == 0, alt: i * 1000, sel: i == 3, idx: (i % 6) as u8 })
        .collect();
    for z in ZOOMS_NM {
        paint_map(&mut d, &blips, &list, Some(1.0), z);
        paint_map(&mut d, &blips, &list, None, z);
    }
    check!(d.oob == 0, "{} draw calls off screen", d.oob);

    if fails == 0 {
        println!("all plane tests passed");
    } else {
        println!("{fails} FAILED");
        std::process::exit(1);
    }
}
