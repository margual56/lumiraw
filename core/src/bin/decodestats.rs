//! Decode-only statistics, for comparing against the Python (LibRaw) decoder.
use autoraw_core::{decode, ops, raw};

fn main() {
    for path in std::env::args().skip(1) {
        let bytes = std::fs::read(&path).expect("read");

        // Straight from the decoder, before any of our own processing, so
        // this measures the file rather than the pipeline.
        let raw = raw::load(&bytes).expect("raw");
        let mut reach = [0f32; 3];
        let mut saturated = [0usize; 3];
        let (x0, y0, w, h) = raw.area;
        for y in y0..y0 + h {
            for x in x0..x0 + w {
                for k in 0..raw.cpp {
                    let c = match &raw.layout {
                        raw::Layout::Mosaic(cfa) => cfa.color_at(y, x).min(2),
                        raw::Layout::Linear => k,
                    };
                    let level = raw.normalised(x, y, k);
                    reach[c] = reach[c].max(level);
                    if level >= 1.0 {
                        saturated[c] += 1;
                    }
                }
            }
        }

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
        println!("  brightest photosite per channel {:.3} {:.3} {:.3}, saturated {:?}",
                 reach[0], reach[1], reach[2], saturated);
    }
}
