use lumiraw_core::{develop::Development, grade::Settings, lensdb::Database, merge, output};
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let out_dir = &args[0];
    let db_bytes = std::fs::read("../web/src/lib/wasm/lensfun.json").ok();
    let db = db_bytes.as_ref().and_then(|b| Database::parse(b).ok());
    let frames: Vec<merge::Frame> = args[1..].iter().map(|p| merge::Frame::open(p, &std::fs::read(p).unwrap()).unwrap()).collect();
    for f in &frames { println!("{} exp {:e} ev_comp {} {}x{}", f.name, f.exposure(), f.meta.exposure_comp, f.linear.w, f.linear.h); }
    let (img, notes) = merge::merge(&frames, merge::Options::default()).unwrap();
    println!("{:?}", notes);
    let r = &frames[notes.reference];
    let mut dev = Development::from_frame("merged", img, r.meta.clone(), r.exif.clone(), db.as_ref());
    let (out, report) = dev.render(&Settings::default(), Some(1600), true, None);
    println!("{}", report.to_json());
    std::fs::write(format!("{out_dir}/merged.png"), output::save(&out, "png8", 92, &dev.exif, None).unwrap()).unwrap();
    for (i, f) in frames.iter().enumerate() {
        let mut dev = Development::from_frame("f", f.linear.clone(), f.meta.clone(), f.exif.clone(), db.as_ref());
        let (out, report) = dev.render(&Settings::default(), Some(1600), true, None);
        println!("frame {i}: {}", report.to_json());
        std::fs::write(format!("{out_dir}/frame{i}.png"), output::save(&out, "png8", 92, &dev.exif, None).unwrap()).unwrap();
    }
}
