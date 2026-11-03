//! Develop a raw natively, the same way the wasm build does, and print what
//! every stage decided.
//!
//!     probe <raw> [out.png] [long_edge] [settings.json]
use lumiraw_core::{develop::Development, grade::Settings, lensdb::Database, output};
use std::time::Instant;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("usage: probe <raw> [out.png] [long_edge] [settings.json]");
    let out_path = args.next().unwrap_or_else(|| "out.png".into());
    let long_edge: Option<usize> = args.next().and_then(|s| s.parse().ok());
    let settings = match args.next() {
        Some(p) => {
            let text = std::fs::read_to_string(&p).expect("read settings");
            Settings::from_json(&serde_json::from_str(&text).expect("settings json"))
        }
        None => Settings::default(),
    };

    let db_bytes = ["web/src/lib/wasm/lensfun.json", "../web/src/lib/wasm/lensfun.json",
                    "../../web/src/lib/wasm/lensfun.json"]
        .iter()
        .find_map(|p| std::fs::read(p).ok());
    let db = db_bytes.as_ref().and_then(|b| Database::parse(b).ok());
    match db.as_ref() {
        Some(d) => println!("database: {} calibrated lenses", d.lenses.len()),
        None => println!("database: MISSING (no lens correction)"),
    }

    let bytes = std::fs::read(&path).expect("read");
    let t0 = Instant::now();
    let mut dev = Development::open(&path, &bytes, db.as_ref()).expect("open");
    println!("decode {:.2}s -> {}x{}", t0.elapsed().as_secs_f32(), dev.width(), dev.height());
    println!("describe {}", dev.describe());
    println!("lens match: {:?}", dev.lens_match.as_ref().map(|m| m.lens.clone()));

    let t1 = Instant::now();
    let (img, report) = dev.render(&settings, long_edge, true, None);
    println!("render {:.2}s -> {}x{}", t1.elapsed().as_secs_f32(), img.w, img.h);
    println!("RESULT {}", serde_json::json!({
        "describe": dev.describe(),
        "report": report.to_json(),
        "decode_s": (t0.elapsed().as_secs_f32() - t1.elapsed().as_secs_f32()),
        "render_s": t1.elapsed().as_secs_f32(),
        "width": img.w, "height": img.h,
    }));

    let t2 = Instant::now();
    let png = output::save(&img, "png8", 92, &dev.exif, None).expect("encode");
    std::fs::write(&out_path, &png).expect("write");
    println!("png {:.2}s, {} bytes -> {}", t2.elapsed().as_secs_f32(), png.len(), out_path);
    println!("total {:.2}s", t0.elapsed().as_secs_f32());
}
