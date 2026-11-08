// Mutation fuzz for face.rs: flips, overwrites and truncates real face files,
// fixes the length field half the time, and checks render never panics or
// overflows. Usage: fuzz N FACE...
#[path = "../src/face.rs"]
#[allow(dead_code)]
mod face;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let n: u64 = a[1].parse().unwrap();
    let files: Vec<Vec<u8>> = a[2..].iter().map(|p| std::fs::read(p).unwrap()).collect();
    let mut seed = 0x1234_5678_9abc_def0u64;
    let mut rnd = move || { seed ^= seed << 13; seed ^= seed >> 7; seed ^= seed << 17; seed };
    let mut vals = face::Vals::new();
    for (s, x) in [(2, 22), (4, 8), (5, 36), (7, 7), (8, 11), (9, 2026), (10, 6), (21, 8432), (22, 10000), (24, 73)] {
        vals.set(s, x);
    }
    vals.set_str(80, b"BUS 14 IN 6 MIN");
    face::derive(&mut vals);
    let mut ok = 0u64;
    let mut invalid = 0u64;
    for i in 0..n {
        let mut f = files[(rnd() % files.len() as u64) as usize].clone();
        let k = 1 + rnd() % 8;
        for _ in 0..k {
            let m = rnd();
            let p = (m >> 8) as usize % f.len();
            match m % 5 {
                0 => f[p] ^= 1 << ((m >> 40) % 8),
                1 => f[p] = (m >> 32) as u8,
                2 => f[p] = [0, 0xff, 0x7f, 0x80, 1, 2][(m >> 32) as usize % 6],
                3 => { let q = (p / 16) * 16; if q + 1 < f.len() { f[q + 1] = (m >> 24) as u8 } }
                _ => { let l = f.len(); f.truncate(16.max(l - (m >> 20) as usize % l.min(64))) }
            }
        }
        if rnd() % 2 == 0 && f.len() >= 16 {
            let l = f.len() as u32;
            f[12..16].copy_from_slice(&l.to_le_bytes());
        }
        let r = std::panic::catch_unwind(|| {
            let mut px = [0u8; face::CANVAS];
            let mut ins = face::Scratch::new();
            let mut cv = face::Canvas::new(&mut px);
            face::render(&f, &vals, &mut cv, &mut ins)
        });
        match r {
            Ok(Some(_)) => ok += 1,
            Ok(None) => invalid += 1,
            Err(_) => { std::fs::write(format!("crash_{}.face", i), &f).unwrap(); eprintln!("panic at case {}", i); std::process::exit(1) }
        }
    }
    println!("cases {}: rendered {}, rejected {}, panics 0", n, ok, invalid);
}
