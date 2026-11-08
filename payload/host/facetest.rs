// PC harness for payload/src/face.rs. Usage: facetest FACE VALS OUT
// VALS: one entry per line, "src int" or "src s text". OUT gets the 2048-byte
// canvas, one status byte (0 invalid, 1 no inset, 2 inset; +4 when the face owns the inset) and the inset text.
#[path = "../src/face.rs"]
#[allow(dead_code)]
mod face;
use std::fs;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let file = fs::read(&a[1]).unwrap();
    let txt = fs::read(&a[2]).unwrap();
    let lines: Vec<&[u8]> = txt.split(|&c| c == b'\n').filter(|l| !l.is_empty()).collect();
    let mut v = face::Vals::new();
    for l in &lines {
        let sp = l.iter().position(|&c| c == b' ').unwrap();
        let src: usize = std::str::from_utf8(&l[..sp]).unwrap().parse().unwrap();
        let rest = &l[sp + 1..];
        if rest.starts_with(b"s ") {
            v.set_str(src, &rest[2..]);
        } else {
            v.set(src, std::str::from_utf8(rest).unwrap().trim().parse().unwrap());
        }
    }
    face::derive(&mut v);
    let mut px = [0u8; face::CANVAS];
    let mut ins = face::Scratch::new();
    let r = {
        let mut cv = face::Canvas::new(&mut px);
        let r = face::render(&file, &v, &mut cv, &mut ins);
        println!("work {}", cv.work);
        r
    };
    let mut out = px.to_vec();
    out.push(match r { None => 0, Some(false) => 1, Some(true) => 2 } | if ins.own { 4 } else { 0 });
    out.extend_from_slice(ins.inset.bytes());
    fs::write(&a[3], out).unwrap();
}
