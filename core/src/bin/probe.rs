//! Native harness: develop a raw the same way the wasm build will, and print
//! what every stage decided.  Not part of the wasm build.
use autoraw_core::{develop::Development, grade::Settings, lensdb::Database, output};

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("usage: probe <raw> [out.png] [long_edge]");
    let out_path = args.next().unwrap_or_else(|| "out.png".into());
    let long_edge: Option<usize> = args.next().and_then(|s| s.parse().ok());

    // The baked database lives with the web assets; find it whether we were
    // started from the crate or from the repository root.
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
    let t0 = std::time::Instant::now();
    let mut dev = Development::open(&path, &bytes, db.as_ref()).expect("open");
    println!("decode {:.2}s -> {}x{}", t0.elapsed().as_secs_f32(), dev.width(), dev.height());
    println!("describe {}", dev.describe());
    println!("lens match: {:?}", dev.lens_match.as_ref().map(|m| m.lens.clone()));

    let settings = Settings::default();
    let t1 = std::time::Instant::now();
    let mut last = String::new();
    let mut cb = |f: f32, code: &str| {
        last = format!("{:.0}% {}", f * 100.0, code);
    };
    let (img, report) = dev.render(&settings, "original", long_edge, true, Some(&mut cb));
    println!("render {:.2}s -> {}x{}", t1.elapsed().as_secs_f32(), img.w, img.h);
    // One line of JSON, so tools/parity.py can diff it against the Python
    // pipeline's own report field by field.
    println!("RESULT {}", serde_json::json!({
        "describe": dev.describe(),
        "report": report.to_json(),
        "decode_s": (t0.elapsed().as_secs_f32() - t1.elapsed().as_secs_f32()),
        "render_s": t1.elapsed().as_secs_f32(),
        "width": img.w, "height": img.h,
    }));

    let t2 = std::time::Instant::now();
    let png = output::save(&img, "png8", 92, &dev.exif, None).expect("encode");
    std::fs::write(&out_path, &png).expect("write");
    println!("png {:.2}s, {} bytes -> {}", t2.elapsed().as_secs_f32(), png.len(), out_path);
    println!("total {:.2}s", t0.elapsed().as_secs_f32());
}
