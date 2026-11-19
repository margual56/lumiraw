//! Guard rails on real photographs: every raw in a folder, developed fully
//! automatically, must stay within what a good development does. The raws
//! are yours and stay out of the repository; point the test at them:
//!
//! ```text
//! LUMIRAW_RAWS=~/Pictures/some-raws cargo test --release --test real_raws -- --ignored --nocapture
//! ```
//!
//! Per frame, and on average over the folder: how much is crushed to black,
//! how much is near white, how much has a channel clipped, and how far the
//! automatic white balance tints what the camera's own balance left neutral.
//! The per-frame bounds hold for any photograph; the averages assume a varied
//! folder, not twenty night shots or twenty of snow.

mod common;
use common::*;
use kit::Image;
use lumiraw_core::analyze::NEUTRAL_CHROMA;
use lumiraw_core::develop::Development;
use lumiraw_core::grade::Settings;

/// The size the frames are developed at.
const EDGE: usize = 1200;

/// What is measured, and what it may come to in one frame (if anything is
/// asked of a single frame) and on average.
const METRICS: [(&str, Option<f32>, f32); 4] = [
    ("crushed", Some(0.05), 0.01),
    ("near white", None, 0.002),
    ("clipped", Some(0.02), 0.003),
    ("tinted", Some(0.025), 0.008),
];

/// Oklab chroma of the mean of what the camera's own balance left neutral,
/// once the automatic balance has been at it.
fn tint(dev: &mut Development, developed: &Image) -> f32 {
    let mut camera = Settings::default();
    camera.enabled.insert("white_balance".into(), false);
    let (as_shot, _) = dev.render(&camera, Some(EDGE), true, None);
    let shifts: Vec<(f32, f32)> = as_shot.px().iter().zip(developed.px())
        .filter(|(p, _)| { let (l, c, _) = lch(**p); l > 0.3 && l < 0.9 && c < NEUTRAL_CHROMA })
        .map(|(_, q)| { let (_, a, b) = kit::rgb_to_oklab_px(q[0], q[1], q[2]); (a, b) })
        .collect();
    let n = shifts.len().max(1) as f32;
    let (a, b) = shifts.iter().fold((0.0, 0.0), |(sa, sb), (a, b)| (sa + a, sb + b));
    (a / n).hypot(b / n)
}

/// The metrics as one line, percentages and the tint ×1000.
fn line(v: [f32; 4]) -> String {
    format!("crushed {:5.2} %  near white {:5.2} %  clipped {:5.2} %  tint {:5.1}",
            v[0] * 100.0, v[1] * 100.0, v[2] * 100.0, v[3] * 1000.0)
}

#[test]
#[ignore = "needs LUMIRAW_RAWS pointing at a folder of raw files"]
fn real_raws_stay_within_bounds() {
    let dir = std::env::var("LUMIRAW_RAWS").expect("set LUMIRAW_RAWS to a folder of raw files");
    let mut files: Vec<_> = std::fs::read_dir(&dir).expect("a readable folder")
        .filter_map(|e| e.ok().map(|e| e.path())).collect();
    files.sort();
    let (mut rows, mut skipped, mut failures) = (Vec::new(), Vec::new(), Vec::new());
    for path in files {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let opened = std::fs::read(&path).ok()
            .and_then(|bytes| Development::open(&name, &bytes, None).ok());
        let Some(mut dev) = opened else {
            skipped.push(name);
            continue;
        };
        let (out, _) = dev.render(&Settings::default(), Some(EDGE), true, None);
        let row = [crushed(&out), near_white(&out), clipped(&out), tint(&mut dev, &out)];
        println!("{name:<20} {}  median {:3.0}", line(row), median_code(&out));
        for ((what, limit, _), value) in METRICS.iter().zip(row) {
            if limit.is_some_and(|l| value > l) {
                failures.push(format!("{name}: {what} {value:.4}, over {}", limit.unwrap()));
            }
        }
        rows.push(row);
    }
    if !skipped.is_empty() {
        println!("not opened ({}): {}", skipped.len(), skipped.join(", "));
    }
    assert!(!rows.is_empty(), "no raw files could be opened in {dir}");
    let mean: [f32; 4] = std::array::from_fn(|i| {
        rows.iter().map(|r| r[i]).sum::<f32>() / rows.len() as f32
    });
    println!("{:<20} {}", "mean", line(mean));
    for ((what, _, limit), value) in METRICS.iter().zip(mean) {
        if value > *limit {
            failures.push(format!("on average {what} {value:.4}, over {limit}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
