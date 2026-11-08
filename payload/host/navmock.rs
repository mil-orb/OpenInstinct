// Host run of navcore::paint for mock-ups: prints the drawing calls the watch
// would make, one per line, for faces/nav_mock.py to render.
// Build: rustc +stable-x86_64-pc-windows-gnu -O -C debug-assertions=on -C overflow-checks=on --edition 2021 navmock.rs
// Run: navmock <metrics file>   (written by nav_mock.py: "M font height ascent"
// lines, then "W font codepoint advance*64" lines)
//
// Output per case: "CASE name", then "R ink x y w h", "P ink x0 y0 x1 y1 ...",
// "T font x top align text".
#[path = "../src/navcore.rs"]
#[allow(dead_code)]
mod navcore;
use navcore::*;
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

fn main() {
    let path = std::env::args().nth(1).expect("metrics file");
    let mut r = Rec { adv: [HashMap::new(), HashMap::new()], met: [(12, 10), (16, 13)] };
    for l in std::fs::read_to_string(path).expect("read").lines() {
        let f: Vec<&str> = l.split_whitespace().collect();
        match f.as_slice() {
            ["M", font, h, a] => r.met[font.parse::<usize>().unwrap() & 1] = (h.parse().unwrap(), a.parse().unwrap()),
            ["W", font, c, w] => {
                r.adv[font.parse::<usize>().unwrap() & 1].insert(c.parse().unwrap(), w.parse().unwrap());
            }
            _ => {}
        }
    }
    let cases: [(&str, u32, &str, &str); 9] = [
        ("turn_right", A_TURN_RIGHT, "300 m", "Turn right onto Main St\nETA: 14:32\n3.4 km left"),
        ("keep_left", A_KEEP_LEFT, "1.2 km", "Keep left at the fork to continue on I-5 N\nETA: 15:05\n48 km left"),
        ("arrive", A_FINISH, "50 m", "Arrive at destination\nETA: 14:36\n50 m left"),
        ("roundabout", A_RB_RIGHT, "800 ft", "At the roundabout, take the 2nd exit onto Bahnhofstra\u{00df}e towards the city centre\nETA: 9:41 AM\n2.1 mi left"),
        ("uturn_no_eta", A_UTURN_LEFT, "120 m", "Make a U-turn"),
        ("long_number", A_CONTINUE, "12,400 ft", "Continue on US-101 S\nETA: 16:20\n23 mi left"),
        ("photo_left", A_TURN_LEFT, "0.2 mi", "A12 E / Mill Rd\nETA: 19:16"),
        ("unknown_street", 0, "0.5 mi", "A12 E / Mill Rd\nETA: 19:16"),
        ("unknown_continue", 255, "1.4 mi", "Continue onto I-270 N\nETA: 19:22\n6.1 mi left"),
    ];
    for (name, a, t, m) in cases {
        println!("CASE {name}");
        paint(&mut r, a, t.as_bytes(), m.as_bytes());
    }
}
