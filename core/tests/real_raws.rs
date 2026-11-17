//! Guard rails on real photographs: every raw in a folder, developed fully
//! automatically, must stay within what a good development does. The raws
//! are yours and stay out of the repository; point the test at them:
//!
//! ```text
//! LUMIRAW_RAWS=~/Pictures/some-raws cargo test --release --test real_raws -- --ignored
//! ```
//!
//! Per frame, and on average over the folder: how much is crushed to black,
//! how much is near white, how much has a channel clipped, and how far the
//! automatic white balance tints what the camera's own balance left neutral.

mod common;
use common::*;
use kit::Image;
use lumiraw_core::develop::Development;
use lumiraw_core::grade::Settings;

/// What any single frame may come to.
const FRAME_CRUSHED: f32 = 0.05;
const FRAME_CLIPPED: f32 = 0.02;
const FRAME_TINT: f32 = 0.025;

/// What the folder may come to on average.
const MEAN_CRUSHED: f32 = 0.01;
const MEAN_NEAR_WHITE: f32 = 0.002;
const MEAN_CLIPPED: f32 = 0.003;
const MEAN_TINT: f32 = 0.008;

/// The share of pixels coding at 250 or above.
fn near_white(img: &Image) -> f32 {
    img.px().iter().filter(|p| code(kit::luminance_px(*p)) >= 250.0).count() as f32
        / img.px().len() as f32
}

/// Oklab chroma of the mean of what the camera's own balance left neutral,
/// once the automatic balance has been at it.
fn tint(dev: &mut Development, developed: &Image) -> f32 {
    let mut camera = Settings::default();
    camera.enabled.insert("white_balance".into(), false);
    let (as_shot, _) = dev.render(&camera, Some(1200), true, None);
    let (mut sum, mut n) = ([0f64; 3], 0usize);
    for (p, q) in as_shot.px().iter().zip(developed.px()) {
        let (l, c, _) = lch(*p);
        if l > 0.3 && l < 0.9 && c < 0.02 {
            let (_, a, b) = kit::rgb_to_oklab_px(q[0], q[1], q[2]);
            sum[0] += a as f64;
            sum[1] += b as f64;
            n += 1;
        }
    }
    if n == 0 { 0.0 } else { ((sum[0] / n as f64) as f32).hypot((sum[1] / n as f64) as f32) }
}

#[test]
#[ignore = "needs LUMIRAW_RAWS pointing at a folder of raw files"]
fn real_raws_stay_within_bounds() {
    let Ok(dir) = std::env::var("LUMIRAW_RAWS") else {
        panic!("set LUMIRAW_RAWS to a folder of raw files");
    };
    let mut files: Vec<_> = std::fs::read_dir(&dir).expect("a readable folder")
        .filter_map(|e| e.ok().map(|e| e.path())).collect();
    files.sort();
    let (mut totals, mut count, mut failures) = ([0f32; 4], 0f32, Vec::new());
    for path in files {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let Ok(bytes) = std::fs::read(&path) else { continue };
        let Ok(mut dev) = Development::open(&name, &bytes, None) else { continue };
        let (out, _) = dev.render(&Settings::default(), Some(1200), true, None);
        let row = [crushed(&out), near_white(&out), clipped(&out), tint(&mut dev, &out)];
        println!("{name:<20} crushed {:5.2} %  near white {:5.2} %  clipped {:5.2} %  \
                  tint {:5.1}  median {:3.0}",
                 row[0] * 100.0, row[1] * 100.0, row[2] * 100.0, row[3] * 1000.0, median_code(&out));
        for (what, value, limit) in [("crushed", row[0], FRAME_CRUSHED),
                                     ("clipped", row[2], FRAME_CLIPPED),
                                     ("tinted", row[3], FRAME_TINT)] {
            if value > limit {
                failures.push(format!("{name}: {what} {value:.4}, over {limit}"));
            }
        }
        for i in 0..4 {
            totals[i] += row[i];
        }
        count += 1.0;
    }
    assert!(count > 0.0, "no raw files could be opened in {dir}");
    let mean = totals.map(|t| t / count);
    println!("mean                 crushed {:5.2} %  near white {:5.2} %  clipped {:5.2} %  tint {:5.1}",
             mean[0] * 100.0, mean[1] * 100.0, mean[2] * 100.0, mean[3] * 1000.0);
    for (what, value, limit) in [("crushed", mean[0], MEAN_CRUSHED),
                                 ("near white", mean[1], MEAN_NEAR_WHITE),
                                 ("clipped", mean[2], MEAN_CLIPPED),
                                 ("tinted", mean[3], MEAN_TINT)] {
        if value > limit {
            failures.push(format!("on average {what} {value:.4}, over {limit}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
