// Unit tests for navcore.rs (the nav screen's text and number logic).
// Build: rustc +stable-x86_64-pc-windows-gnu -O -C debug-assertions=on -C overflow-checks=on --edition 2021 navtest.rs
// Run: navtest   (prints each group, exits 1 on the first failure)
#[path = "../src/navcore.rs"]
#[allow(dead_code)]
mod navcore;
use navcore::*;

static mut FAILS: u32 = 0;

fn check<T: PartialEq + std::fmt::Debug>(what: &str, got: T, want: T) {
    if got != want {
        println!("FAIL {what}: got {got:?}, want {want:?}");
        unsafe { FAILS += 1 };
    }
}

fn s(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

/// Records the inked bounding box of what is drawn.
struct BBox(i32, i32, i32, i32);
impl Draw for BBox {
    fn rect(&mut self, ink: bool, x: i32, y: i32, w: i32, h: i32) {
        if ink && w > 0 && h > 0 {
            self.add(x, y);
            self.add(x + w - 1, y + h - 1);
        }
    }
    fn poly(&mut self, ink: bool, p: &[i16]) {
        if ink {
            for c in p.chunks(2) {
                self.add(c[0] as i32, c[1] as i32);
            }
        }
    }
    fn text(&mut self, _: u8, _: i32, _: i32, _: u32, _: &[u8]) {}
    fn width(&mut self, _: u8, _: &[u8]) -> i32 {
        0
    }
    fn metrics(&mut self, _: u8) -> (i32, i32) {
        (0, 0)
    }
}
impl BBox {
    fn add(&mut self, x: i32, y: i32) {
        self.0 = self.0.min(x);
        self.1 = self.1.min(y);
        self.2 = self.2.max(x);
        self.3 = self.3.max(y);
    }
}

fn main() {
    // ---- distances
    let arrow = "\u{2191} "; // Gadgetbridge's optional leading arrow
    for (t, want) in [
        ("350 m", Some(350)),
        ("350m", Some(350)),
        ("0 m", Some(0)),
        ("1.2 km", Some(1200)),
        ("1,2 km", Some(1200)),
        ("1.25km", Some(1250)),
        ("2 KM", Some(2000)),
        (".5 km", Some(500)),
        ("1.9999 km", Some(1999)), // fraction keeps 3 digits
        ("0.4 mi", Some(643)),
        ("0,4 mi", Some(643)),
        ("1 mi", Some(1609)),
        ("500 ft", Some(152)),
        ("1,200 ft", Some(365)),
        ("1,000 ft", Some(304)),
        ("80 yd", Some(73)),
        ("1,25 km", Some(1250)), // decimal comma, two digits
        ("1,2345 km", Some(1234)), // four digits: decimal, not thousands
        ("99999999 yd", Some(91439)), // whole part capped at 99999
        ("4294967296 km", Some(99_999_000)),
        ("99999,999 ft", Some(30479)),
        ("350", None),
        ("m", None),
        ("", None),
        ("abc", None),
        ("12 parsecs", None),
        ("- 5 m", None),
    ] {
        check(&format!("metres({t:?})"), metres(t.as_bytes()), want);
        let a = format!("{arrow}{t}");
        check(&format!("metres(arrow {t:?})"), metres(a.as_bytes()), want);
    }
    println!("metres done");

    // ---- bands
    check("band None", band(None), 0);
    check("band 0", band(Some(0)), 2);
    check("band 80", band(Some(NOW)), 2);
    check("band 81", band(Some(NOW + 1)), 1);
    check("band 300", band(Some(NEAR)), 1);
    check("band 301", band(Some(NEAR + 1)), 0);
    check("band max", band(Some(u32::MAX)), 0);
    // band crossing as decide() sees it: first show buzzes, same manoeuvre and
    // band is quiet, entering NEAR then NOW buzzes, moving away is quiet and
    // re-arms the band.
    let h = hash(2, b"Turn left onto Main St");
    let (v, f) = decide(0, 0, h, band(Some(900)));
    check("first show buzzes", v, true);
    check("first show flags", f, F_SHOWN);
    let (v, f) = decide(f, h, h, band(Some(600)));
    check("same band quiet", v, false);
    let (v, f) = decide(f, h, h, band(Some(290)));
    check("enter NEAR buzzes", v, true);
    check("NEAR stored", f & F_BAND, 1);
    let (v, f) = decide(f, h, h, band(Some(250)));
    check("inside NEAR quiet", v, false);
    let (v, f) = decide(f, h, h, band(Some(60)));
    check("enter NOW buzzes", v, true);
    let (v, f2) = decide(f, h, h, band(Some(500)));
    check("moving away quiet", v, false);
    check("moving away re-arms", f2 & F_BAND, 0);
    let h2 = hash(5, b"Turn right onto Elm St");
    let (v, _) = decide(f, h, h2, band(Some(900)));
    check("new manoeuvre buzzes", v, true);
    check("flags keep held/vibe bits", decide(F_HELD | F_VIBE | F_SHOWN, h, h, 0).1, F_HELD | F_VIBE | F_SHOWN);
    check("band bits masked", decide(F_SHOWN, h, h, 7).1, F_SHOWN | 3);
    println!("band done");

    // ---- manoeuvre hash
    check("hash stable", hash(2, b"Turn left"), hash(2, b"Turn left"));
    let mut same = 0;
    for a in 0..19u32 {
        for b in 0..19u32 {
            if a != b && hash(a, b"Turn left onto Main St") == hash(b, b"Turn left onto Main St") {
                same += 1;
            }
        }
    }
    // 8-bit hash: some clashes are expected; one only costs a missed buzz
    // when two manoeuvres in a row have identical text
    println!("hash: {same} of 342 action pairs clash on the same text");
    let words = ["Turn left onto Main St", "Turn left onto Elm St", "Keep right", "Keep left", "Continue on A1", ""];
    let mut clash = 0;
    for x in words {
        for y in words {
            if x != y && hash(2, x.as_bytes()) == hash(2, y.as_bytes()) {
                clash += 1;
            }
        }
    }
    println!("hash: {clash} of 30 instruction pairs clash");
    println!("hash done");

    // ---- app id
    check("action 2", parse_action(b"gadgetbridge.nav.v1.2"), Some(2));
    check("action 18", parse_action(b"gadgetbridge.nav.v1.18"), Some(18));
    check("prefix only", parse_action(b"gadgetbridge.nav.v1."), None);
    check("letters", parse_action(b"gadgetbridge.nav.v1.2a"), None);
    check("other app", parse_action(b"com.google.android.apps.maps"), None);
    check("short", parse_action(b"gadget"), None);
    check("empty", parse_action(b""), None);
    println!("parse_action done");

    // ---- lines, ETA, remaining
    let msg = "Turn left onto Main St\nETA 14:32\n3.4 km".as_bytes();
    check("line 0", s(line(msg, 0)), "Turn left onto Main St".into());
    check("line 2", s(line(msg, 2)), "3.4 km".into());
    check("line 3", s(line(msg, 3)), "".into());
    check("line of empty", s(line(b"", 0)), "".into());
    check("trailing newline", s(line(b"a\n", 1)), "".into());
    check("eta", s(eta(msg)), "14:32".into());
    check("left", s(left(msg)), "3.4 km".into());
    check("eta colon", s(eta(b"x\nETA: 2:05 PM\n")), "2:05 PM".into());
    check("eta lower", s(eta(b"x\neta14:32")), "14:32".into());
    check("eta plain", s(eta(b"x\n14:32")), "14:32".into());
    check("eta long cut", s(eta(b"x\nETA 14:32 tomorrow")), "14:32 to".into());
    check("eta missing", s(eta(b"only one line")), "".into());
    check("left long cut", s(left(b"a\nb\n1234.5 km left")), "1234.5 k".into());
    check("left utf8 cut", s(left("a\nb\n123456 \u{00e9}x".as_bytes())), "123456 ".into());
    // Gadgetbridge NavigationNotificationSender bodies (instruction, then
    // "ETA: %s" if known, then "%s left" if known)
    let gb = b"Turn left onto Main Street\nETA: 11:55\n2.5km left";
    check("gb eta", s(eta(gb)), "11:55".into());
    check("gb left", s(left(gb)), "2.5km".into());
    let gb = b"High St towards Blah\nETA: 11:55";
    check("gb maps eta", s(eta(gb)), "11:55".into());
    check("gb maps left", s(left(gb)), "".into());
    let gb = b"Keep right\n800 m left";
    check("gb no eta: eta", s(eta(gb)), "".into());
    check("gb no eta: left", s(left(gb)), "800 m".into());
    check("gb only instr", s(eta(b"Keep right")), "".into());
    check("dist_prefix nbsp", s(dist_prefix("4.6\u{a0}mi left".as_bytes())), "4.6\u{a0}mi".into());
    check("metres nbsp", metres("100\u{a0}yd".as_bytes()), Some(91));
    check("metres narrow nbsp", metres("1,2\u{202f}km".as_bytes()), Some(1200));
    check("metres no space", metres(b"100m"), Some(100));
    println!("lines done");

    // ---- pixel wrap (6 px per character, as a stand-in for the watch font)
    let cw = |t: &[u8]| -> i32 { 6 * std::str::from_utf8(t).map(|x| x.chars().count()).unwrap_or(t.len()) as i32 };
    let wl = |t: &str, ws: &[i32]| -> Vec<String> {
        let r = wrap_lines(t.as_bytes(), ws, 12, &mut |t: &[u8]| cw(t));
        let b = t.as_bytes();
        let mut v: Vec<String> = (0..r.n).map(|i| s(&b[r.start[i]..r.end[i]])).collect();
        if r.dots {
            if let Some(l) = v.last_mut() {
                l.push_str("..");
            }
        }
        v
    };
    let w3 = [96, 108, 90];
    check("wl short", wl("Keep right", &w3), vec!["Keep right".to_string()]);
    check("wl exact", wl("1234567890123456", &w3), vec!["1234567890123456".to_string()]);
    check("wl street whole", wl("Turn right onto Main St", &w3), vec!["Turn right onto".to_string(), "Main St".to_string()]);
    // greedy would give "Turn right onto Main" / "St"
    check("wl street kept", wl("Turn right onto Main St", &[120, 108, 90]), vec!["Turn right onto".to_string(), "Main St".to_string()]);
    check("wl fits one", wl("Turn right onto Main St", &[140]), vec!["Turn right onto Main St".to_string()]);
    check("wl I-5", wl("Keep left at the fork to continue on I-5 N", &w3),
        vec!["Keep left at the".to_string(), "fork to continue".to_string(), "on I-5 N".to_string()]);
    check("wl arrive", wl("Arrive at destination", &w3), vec!["Arrive at".to_string(), "destination".to_string()]);
    check("wl long name not moved", wl("Turn left onto Avenue of the Americas South", &w3),
        vec!["Turn left onto".to_string(), "Avenue of the".to_string(), "Americas South".to_string()]);
    check("wl dots at end only", wl("Turn left onto the Main Street towards the city centre and beyond", &w3),
        vec!["Turn left onto".to_string(), "the Main Street".to_string(), "towards the c..".to_string()]);
    check("wl two lines dots", wl("Turn left onto the Main Street towards the city centre", &[108, 90]),
        vec!["Turn left onto the".to_string(), "Main Street t..".to_string()]);
    check("wl no space", wl("Abcdefghijklmnopqrstuvwxyz", &[60, 60]), vec!["Abcdefghij".to_string(), "klmnopqr..".to_string()]);
    check("wl umlaut", wl("Continue onto the \u{00d6}lweg", &[108, 90]),
        vec!["Continue onto".to_string(), "the \u{00d6}lweg".to_string()]);
    let t = "\u{00e9}".repeat(40);
    let v = wl(&t, &[60, 60]);
    check("wl utf8 l1", v[0].clone(), "\u{00e9}".repeat(10));
    check("wl utf8 l2", v[1].clone(), format!("{}..", "\u{00e9}".repeat(8)));
    check("wl tiny width", wl("Abc def", &[1, 1]), vec!["A".to_string(), "b..".to_string()]);
    check("wl empty", wl("", &w3), Vec::<String>::new());
    check("wl no lines", wl("Abc", &[]), Vec::<String>::new());
    let long = "Turn left ".repeat(20);
    let v = wl(long.trim_end(), &w3);
    check("wl long n", v.len(), 3);
    check("wl long dots", v[2].ends_with(".."), true);
    let r = wrap_lines(&[0x80u8; 80], &[60, 60, 60], 12, &mut |t: &[u8]| cw(t));
    check("wl invalid bounds", (0..r.n).all(|i| r.start[i] <= r.end[i] && r.end[i] <= 80), true);
    // spans keep clear of the inset and the circle
    check("span low row", span(100, 110), (64 - (halfw(110) - 3), 64 + halfw(110) - 3));
    let (_, x1) = span(20, 30);
    check("span left of inset", x1 <= inset_left(20, 30) - 1 && x1 < 79, true);
    check("inset_left none", inset_left(60, 70), 128);
    check("inset_left at centre row", inset_left(22, 22), 105 - INSET_R - 1);
    // event time in the record and the delete rules
    for t in [1u16, 0x7ff, 0x100, 0x2ab, 0x7fe] {
        let (f, lo) = with_time(F_SHOWN | F_HELD | 2, t);
        check("time round trip", time_of(f, lo), t);
        check("time keeps flags", f & !F_TIME, F_SHOWN | F_HELD | 2);
    }
    check("time 0 stored as 1", { let (f, lo) = with_time(0, 0x800); time_of(f, lo) }, 1);
    check("age wraps", age(3, 0x7fe), 5);
    let (f, lo) = with_time(F_SHOWN, 100);
    check("hold recent", hold_delete(f, lo, 110), true);
    check("pass after gap", hold_delete(f, lo, 100 + GAP_FRESH), false);
    check("pass after gap across wrap", hold_delete(with_time(F_SHOWN, 0x7f0).0, with_time(F_SHOWN, 0x7f0).1, 0x7f0u16.wrapping_add(GAP_FRESH) & TIME_MASK), false);
    check("hold never stamped", hold_delete(F_SHOWN, 0, 500), true);
    check("pass dangling held", hold_delete(f | F_HELD, lo, 101), false);
    check("held not done", held_done(f | F_HELD, lo, 101), false);
    check("held done", held_done(f | F_HELD, lo, 100 + HELD_DONE), true);
    check("not held never done", held_done(f, lo, 900), false);
    let (vib, f2) = decide(f, 0, 0, 0);
    check("decide keeps time", (vib, time_of(f2, lo)), (false, 100));
    // unknown actions read from the instruction
    check("resolve known", resolve_action(A_TURN_LEFT, b"Continue"), A_TURN_LEFT);
    check("resolve turn right", resolve_action(0, b"Turn right onto Main St"), A_TURN_RIGHT);
    check("resolve slight", resolve_action(255, b"Slight left toward I-64"), A_LEFT_SLIGHTLY);
    check("resolve sharp", resolve_action(0, b"SHARP RIGHT onto Elm"), A_RIGHT_SHARPLY);
    check("resolve continue", resolve_action(0, b"Continue onto A12 E"), A_CONTINUE);
    check("resolve roundabout", resolve_action(0, b"At the roundabout, take the 2nd exit"), A_RB_STRAIGHT);
    check("resolve street only", resolve_action(0, b"A12 E / Mill Rd"), A_JUNCTION);
    check("resolve inside word", resolve_action(0, b"Leftwich Turnpike Rightmore"), A_JUNCTION);
    check("resolve empty", resolve_action(0, b""), A_JUNCTION);
    check("resolve head", resolve_action(0, b"Head north on Elm"), A_CONTINUE);
    check("resolve headley", resolve_action(0, b"Headley Rd"), A_JUNCTION);
    // icon boxes: what icon() draws stays inside ICON_BOX at every scale used
    for a in 1..=19u32 {
        let bx = ICON_BOX[a as usize - 1];
        for sc in 4..=16 {
            let mut b = BBox(i32::MAX, i32::MAX, i32::MIN, i32::MIN);
            icon(&mut b, a, 0, 0, sc);
            let lim = bx.map(|v| (v as i32 * sc) >> 4);
            check(&format!("icon {a} sc {sc} inside its box"),
                b.0 >= lim[0] && b.1 >= lim[1] && b.2 <= lim[2] && b.3 <= lim[3] && b.2 >= b.0, true);
        }
    }
    println!("wrap done");

    // ---- bezel geometry and digits
    check("halfw centre", halfw(63), 68);
    check("halfw top edge", halfw(0), 26);
    check("halfw outside", halfw(-10), 0);
    check("halfw symmetric", halfw(10), halfw(117));
    check("row_width bottom row", row_width(110, 122), 2 * halfw(122) - 6);
    check("isqrt", (isqrt(0), isqrt(1), isqrt(15), isqrt(16), isqrt(i32::MAX)), (0, 1, 3, 4, 46340));
    check("seg 8", seg_mask(b'8'), 0x7f);
    check("seg x", seg_mask(b'x'), 0);
    check("split 1,200 ft", split_dist(b"1,200 ft").map(|(a, b)| (s(a), s(b))), Some(("1,200".into(), "ft".into())));
    check("split 0.3 mi", split_dist("\u{2190} 0.3\u{a0}mi".as_bytes()).map(|(a, b)| (s(a), s(b))), Some(("0.3".into(), "mi".into())));
    check("split 100m", split_dist(b"100m").map(|(a, b)| (s(a), s(b))), Some(("100".into(), "m".into())));
    check("split none", split_dist(b"Navigation").is_none(), true);
    check("split too long", split_dist(b"12345678 m").is_none(), true);
    check("digits width", digits_width(b"1.2", 15, 4, 3), 4 + 3 + 4 + 3 + 15); // "1" is one bar
    check("digits width 20", digits_width(b"20", 15, 4, 3), 15 + 3 + 15);
    check("digits width empty", digits_width(b"", 15, 4, 3), 0);
    println!("layout done");

    // ---- utf8_cut
    check("cut short", utf8_cut(b"abc", 8), 3);
    check("cut exact", utf8_cut(b"abcdefgh", 8), 8);
    check("cut mid 2-byte", utf8_cut("abcdefg\u{00e9}".as_bytes(), 8), 7);
    check("cut zero", utf8_cut(b"abc", 0), 0);
    println!("utf8_cut done");

    let f = unsafe { FAILS };
    if f != 0 {
        println!("{f} failures");
        std::process::exit(1);
    }
    println!("all nav tests passed");
}
