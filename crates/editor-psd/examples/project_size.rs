//! Project size breakdown for a PSD: `project_size FILE`.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let bytes = std::fs::read(&a[1]).unwrap();
    let doc = editor_psd::import(&bytes, &Default::default()).unwrap().doc;
    let t = std::time::Instant::now();
    let p = editor_project::save(&doc).unwrap();
    println!("psd {} project {} in {:?}", bytes.len(), p.len(), t.elapsed());
    let z = editor_project::ZipReader::open(&p).unwrap();
    let mut v: Vec<(usize, String)> = z.names().map(|n| (z.read(n, u64::MAX).unwrap().unwrap().len(), n.to_string())).collect();
    v.sort();
    for (s, n) in v.iter().rev().take(8) {
        let d = z.read(n, u64::MAX).unwrap().unwrap();
        if d.starts_with(b"OPPLANE1") {
            let u = |at: usize| u32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]]);
            println!("{s:>10} {n} {}x{} ch{} fill{:?} tiles {}", u(8), u(12), d[16], &d[17..21], u(21));
        } else {
            println!("{s:>10} {n}");
        }
    }
}
