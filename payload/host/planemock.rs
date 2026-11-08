// Host run of the plane screens for mock-ups: prints drawing calls like
// navmock (same output format), for faces/plane_mock.py to render.
// Build: rustc +stable-x86_64-pc-windows-gnu -O --edition 2021 planemock.rs
// Run: planemock <metrics file> <adsb reply json>
#[path = "../src/planecore.rs"]
#[allow(dead_code)]
mod planecore;
use planecore::*;
use std::collections::HashMap;

struct Rec {
    adv: [HashMap<u32, i32>; 2],
    met: [(i32, i32); 2],
}

impl Draw for Rec {
    fn rect(&mut self, ink: bool, x: i32, y: i32, w: i32, h: i32) {
        println!("R {} {x} {y} {w} {h}", ink as u8);
    }
    fn poly(&mut self, ink: bool, pts: &[i16]) {
        let p: Vec<String> = pts.iter().map(|v| v.to_string()).collect();
        println!("P {} {}", ink as u8, p.join(" "));
    }
    fn text(&mut self, font: u8, x: i32, top: i32, align: u32, s: &[u8]) {
        println!("T {font} {x} {top} {align} {}", String::from_utf8_lossy(s));
    }
    fn width(&mut self, font: u8, s: &[u8]) -> i32 {
        let t = String::from_utf8_lossy(s);
        let a = &self.adv[font as usize & 1];
        let sum: i32 = t.chars().map(|c| *a.get(&(c as u32)).unwrap_or(&(6 * 64))).sum();
        (sum + 32) / 64
    }
    fn metrics(&mut self, font: u8) -> (i32, i32) {
        self.met[font as usize & 1]
    }
}

/// Made-up routes in adsbdb's reply format, run through the real parser and
/// the plausibility check, as the watch does.
fn route_for(a: &Ac) -> Option<Route> {
    let (o, d) = match &a.flight[..flen(&a.flight)] {
        b"TAP1301" => (("LIS", "LPPT", 38.7742, -9.1342), ("OPO", "LPPR", 41.2481, -8.6814)),
        b"RYR8HK" => (("FAO", "LPFR", 37.0144, -7.9659), ("DUB", "EIDW", 53.4213, -6.2701)),
        b"EZY62QL" => (("LIS", "LPPT", 38.7742, -9.1342), ("LGW", "EGKK", 51.1481, -0.1903)),
        b"AFR1025" => (("CDG", "LFPG", 49.0097, 2.5479), ("LIS", "LPPT", 38.7742, -9.1342)),
        _ => return None,
    };
    let ap = |x: (&str, &str, f64, f64)| format!(
        "{{\"country_iso_name\":\"XX\",\"elevation\":100,\"iata_code\":\"{}\",\"icao_code\":\"{}\",\"latitude\":{},\"longitude\":{},\"name\":\"Airport\"}}",
        x.0, x.1, x.2, x.3);
    let body = format!("{{\"response\":{{\"flightroute\":{{\"callsign\":\"X\",\"origin\":{},\"destination\":{}}}}}}}", ap(o), ap(d));
    let r = parse_route(body.as_bytes())?;
    route_fits(&r, a.lat, a.lon).then_some(r)
}

fn main() {
    let mut args = std::env::args().skip(1);
    let mpath = args.next().expect("metrics file");
    let jpath = args.next().expect("reply json");
    let mut r = Rec { adv: [HashMap::new(), HashMap::new()], met: [(12, 10), (16, 13)] };
    for l in std::fs::read_to_string(mpath).expect("read").lines() {
        let f: Vec<&str> = l.split_whitespace().collect();
        match f.as_slice() {
            ["M", font, h, a] => r.met[font.parse::<usize>().unwrap() & 1] = (h.parse().unwrap(), a.parse().unwrap()),
            ["W", font, c, w] => {
                r.adv[font.parse::<usize>().unwrap() & 1].insert(c.parse().unwrap(), w.parse().unwrap());
            }
            _ => {}
        }
    }
    let body = std::fs::read(jpath).expect("reply");
    let mut acs = [Ac::ZERO; MAX_AC];
    let n = scan(&body, &mut acs).unwrap_or(0);
    let acs = &acs[..n];
    // the query point of the saved reply
    let own = Own::new(38_670_000, -9_140_000);

    println!("CASE status_aim");
    paint_status(&mut r, b"Hold steady", b"let go of BACK", b"to look up", Inset::Lock(7), Some(1));
    println!("CASE status_wait");
    paint_status(&mut r, b"Searching", b"phone position", b"4 s", Inset::Radar(1), Some(0));
    for (i, (t1, l1, l2)) in [
        (&b"No position"[..], &b"phone location"[..], &b"or a GPS fix"[..]),
        (b"No phone", b"connect phone,", b"hold BACK"),
        (b"No internet", b"Gadgetbridge:", b"turn it on"),
        (b"Server error", b"HTTP 503", b"hold BACK"),
        (b"Clear skies", b"no aircraft", b"within 25 nm"),
        (b"Nothing there", b"none that way;", b"GPS for map"),
        (b"Lost it", b"hold BACK to", b"look again"),
        (b"Hold BACK", b"aim 12 o'clock", b"then let go"),
    ].into_iter().enumerate() {
        println!("CASE err_{i}");
        paint_status(&mut r, t1, l1, l2, Inset::Idle, if i == 7 { Some(2) } else { None });
    }
    println!("CASE radar_4");
    paint_status(&mut r, b"Searching", b"phone position", b"9 s", Inset::Radar(4), Some(2));
    println!("CASE status_nofirewall");
    paint_status(&mut r, b"No reply", b"Gadgetbridge:", b"api.adsb.lol", Inset::Idle, None);

    // aim at TAP1301 (north, high) in the sample reply
    let mut cands = [Cand::ZERO; MAX_AC];
    for (name, aim, live, sel_off) in [("card_match", 2.0f32, Some(10.0f32), 0usize), ("card_next", 2.0, Some(40.0), 1), ("card_nocompass", 0.0, None, 0)] {
        let heading = live.map(|_| aim * DEG);
        let (cn, cone) = rank(acs, &own, 0.0, heading, 2.0 * DEG, &mut cands);
        let Some(c) = cands[..cn].get(sel_off) else { continue };
        let a = &acs[c.idx as usize];
        let (e, nn) = enu(&own, a, 0.0);
        println!("CASE {name}");
        paint_card(
            &mut r,
            &CardInfo {
                ac: a,
                dist_m: sqrt(e * e + nn * nn),
                brg: atan2(e, nn),
                heading: live.map(|d| d * DEG),
                route: route_for(a).as_ref(),
                note: b"",
                flip: false,
            },
        );
        eprintln!("{name}: {} candidates, {} in cone", cn, cone);
    }

    // every silhouette, on a 3x4 grid, and each one at inset size
    let kinds = [Kind::Light, Kind::Twinprop, Kind::Bizjet, Kind::Jet, Kind::Heavy, Kind::Quad, Kind::Heli, Kind::Glider, Kind::Balloon, Kind::Drone, Kind::Unknown];
    println!("CASE kinds");
    for (i, k) in kinds.iter().enumerate() {
        let (x, y) = (24 + (i % 3) as i32 * 40, 18 + (i / 3) as i32 * 31);
        paint_kind(&mut r, *k, x, y, 13.0);
    }
    for k in kinds {
        println!("CASE inset_{:?}", k);
        paint_kind(&mut r, k, INSET.0, INSET.1, 20.0);
    }
    let sel = acs.iter().position(|a| &a.flight[..7] == b"TAP1301").unwrap_or(0);
    let blips: Vec<Blip> = acs
        .iter()
        .enumerate()
        .filter(|(_, a)| a.airborne())
        .map(|(i, a)| {
            let (e, n) = enu(&own, a, 0.0);
            Blip { e, n, track: a.track as f32 * 0.1 * DEG, has_track: a.track != NO_TRACK, alt: a.alt, sel: i == sel, idx: i as u8 }
        })
        .collect();
    let mut blips = blips;
    blips.sort_by(|a, b| (a.e * a.e + a.n * a.n).partial_cmp(&(b.e * b.e + b.n * b.n)).unwrap());
    for (z, h) in [(5u8, Some(30.0f32)), (10, Some(30.0)), (25, Some(30.0)), (10, None)] {
        println!("CASE map_{z}{}", if h.is_none() { "_north" } else { "" });
        paint_map(&mut r, &blips, acs, h.map(|d| d * DEG), z);
    }

    // the whole flow as GIF frames: seq_<n>_<what>
    let mut f = 0;
    let mut frame = |r: &mut Rec, what: &str| {
        println!("CASE seq_{:02}_{}", f, what);
        f += 1;
        let _ = r;
    };
    frame(&mut r, "ready");
    paint_status(&mut r, b"Hold BACK", b"aim 12 o'clock", b"then let go", Inset::Idle, Some(2));
    for k in 0..=4u8 {
        frame(&mut r, "hold");
        paint_status(&mut r, b"Hold steady", b"let go of BACK", b"to look up", Inset::Lock(k * 3), Some(k));
    }
    for p in 0..8u8 {
        frame(&mut r, "search");
        let secs = format!("{} s", p / 2 + 1);
        paint_status(&mut r, b"Searching", b"phone position", secs.as_bytes(), Inset::Radar(p), Some(p));
    }
    for (what, sel_off) in [("card", 0usize), ("next", 1)] {
        let (cn, _) = rank(acs, &own, 0.0, Some(2.0 * DEG), 2.0 * DEG, &mut cands);
        let Some(c) = cands[..cn].get(sel_off) else { continue };
        let a = &acs[c.idx as usize];
        let (e, nn) = enu(&own, a, 0.0);
        frame(&mut r, what);
        paint_card(&mut r, &CardInfo { ac: a, dist_m: sqrt(e * e + nn * nn), brg: atan2(e, nn), heading: Some(4.0 * DEG),
            route: route_for(a).as_ref(), note: b"", flip: false });
    }
    frame(&mut r, "map");
    paint_map(&mut r, &blips, acs, Some(4.0 * DEG), 25);
}
