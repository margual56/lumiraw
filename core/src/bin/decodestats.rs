//! Decode-only statistics, for comparing against the Python (LibRaw) decoder.
use autoraw_core::{decode, ops};

fn main() {
    for path in std::env::args().skip(1) {
        let bytes = std::fs::read(&path).expect("read");
        let d = decode::decode(&bytes).expect("decode");
        let n = (d.img.w * d.img.h) as f64;
        let mut ch = [0f64; 3];
        for px in d.img.d.chunks_exact(3) {
            for c in 0..3 {
                ch[c] += px[c] as f64;
            }
        }
        let y = ops::luminance(&d.img);
        let p = ops::percentiles(&y.d, &[1.0, 50.0, 99.0, 99.9]);
        println!("{}  {}x{}  means {:.5} {:.5} {:.5}  luma {:?}",
                 path.rsplit('/').next().unwrap(), d.img.w, d.img.h,
                 ch[0] / n, ch[1] / n, ch[2] / n,
                 p.iter().map(|v| (v * 100000.0).round() / 100000.0).collect::<Vec<_>>());
    }
}
