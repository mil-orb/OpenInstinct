// Host test for fold() in payload/facehook/src/lib.rs (copy kept in sync by hand).
// Build: rustc +stable-x86_64-pc-windows-gnu -O -C overflow-checks=on foldtest.rs
/// UTF-8 to the ASCII the face fonts have: U+00C0..U+00FF lose their accents
/// (e acute becomes e), other non-ASCII characters and control bytes are
/// dropped. A non-empty input with nothing left becomes "?", so a playing
/// track never looks like "nothing playing". Output is never longer than the
/// input. No panics: every index goes through get().
pub fn fold(src: &[u8], dst: &mut [u8]) -> usize {
    // U+00C0..U+00FF; '*' = no ASCII form (dropped)
    const L1: &[u8; 64] = b"AAAAAAACEEEEIIIIDNOOOOO*OUUUUYTsaaaaaaaceeeeiiiidnooooo*ouuuuyty";
    let mut n = 0usize;
    let mut i = 0usize;
    while let Some(&c) = src.get(i) {
        let mut out = 0u8;
        if c < 0x80 {
            if c >= 0x20 {
                out = c;
            }
            i += 1;
        } else if c == 0xc3 && matches!(src.get(i + 1), Some(&t) if t & 0xc0 == 0x80) {
            let t = src.get(i + 1).copied().unwrap_or(0x80);
            if let Some(&a) = L1.get((t & 0x3f) as usize) {
                if a != b'*' {
                    out = a;
                }
            }
            i += 2;
        } else {
            i += 1; // part of a character the fonts can't draw
        }
        if out != 0 {
            if let Some(d) = dst.get_mut(n) {
                *d = out;
                n += 1;
            }
        }
    }
    if n == 0 && !src.is_empty() {
        if let Some(d) = dst.get_mut(0) {
            *d = b'?';
            n = 1;
        }
    }
    n
}

fn f(s: &[u8]) -> String { let mut d = [0u8; 100]; let n = fold(s, &mut d); String::from_utf8(d[..n].to_vec()).unwrap() }
fn main() {
    assert_eq!(f("Beyonc\u{e9}".as_bytes()), "Beyonce");
    assert_eq!(f("Sigur R\u{f3}s - \u{c1}g\u{e6}tis".as_bytes()), "Sigur Ros - Agatis");
    assert_eq!(f("\u{5ba3}\u{4f1d}".as_bytes()), "?");
    assert_eq!(f("AB\u{5ba3}C".as_bytes()), "ABC");
    assert_eq!(f(b""), "");
    assert_eq!(f(b"a\x01b\tc"), "abc");
    assert_eq!(f(&[0xc3]), "?");
    assert_eq!(f(&[0xc3, 0x41]), "A");
    let mut seed = 12345u64; let mut d = [0u8; 100];
    for _ in 0..2_000_000 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let len = (seed >> 58) as usize + 40; let mut v = vec![0u8; len];
        for b in v.iter_mut() { seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1); *b = (seed >> 56) as u8; }
        let n = fold(&v[..len.min(99)], &mut d);
        assert!(n <= 99); assert!(d[..n].iter().all(|&c| c >= 0x20 && c < 0x80));
    }
    println!("fold ok");
}
