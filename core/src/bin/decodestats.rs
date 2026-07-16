//! Decode-only statistics, for comparing against the Python (LibRaw) decoder.
use autoraw_core::{decode, ops};

fn main() {
    for path in std::env::args().skip(1) {
        let bytes = std::fs::read(&path).expect("read");

        // Straight from rawloader, before any of our own processing, so this
        // measures the file rather than the pipeline.
        let raw = rawloader::decode(&mut std::io::Cursor::new(&bytes)).expect("raw");
        let values: Vec<f32> = match &raw.data {
            rawloader::RawImageData::Integer(v) => v.iter().map(|x| *x as f32).collect(),
            rawloader::RawImageData::Float(v) => v.clone(),
        };
        let mut reach = [0f32; 3];
        let mut saturated = [0usize; 3];
        for y in 0..raw.height {
            for x in 0..raw.width {
                let c = raw.cfa.color_at(y, x).min(2);
                let black = raw.blacklevels[c] as f32;
                let white = raw.whitelevels[c] as f32;
                let level = (values[y * raw.width + x] - black) / (white - black).max(1.0);
                reach[c] = reach[c].max(level);
                if level >= 1.0 {
                    saturated[c] += 1;
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
