//! Known answers: a synthetic colour chart whose right appearance is
//! published, developed fully automatically and compared patch by patch with
//! what it should be.
//!
//! The chart is the classic 24-patch ColorChecker, with its widely published
//! sRGB values (D65). Photographed under neutral light and metered normally,
//! a faithful development shows it as those values up to the tone curve: the
//! same hues, the same chroma give or take the look's saturation, greys that
//! are grey and in order. Under a colour cast, the automatic white balance
//! must bring it back to the neutral-light result. Under- or over-exposed by
//! a stop, the automatic exposure corrects half of the difference, by design.
//!
//! `cargo test --test known -- --nocapture` prints the patch-by-patch
//! comparison; with `LUMIRAW_DUMP=<folder>` it also writes the expected and
//! developed charts as PNGs there, to look at side by side.

mod common;
use common::*;
use kit::Image;
use lumiraw_core::decode::Meta;
use lumiraw_core::grade::Settings;
use std::sync::OnceLock;

/// The 24 patches, row by row, as 8-bit sRGB.
const CHART: [(&str, [u8; 3]); 24] = [
    ("dark skin", [115, 82, 68]), ("light skin", [194, 150, 130]),
    ("blue sky", [98, 122, 157]), ("foliage", [87, 108, 67]),
    ("blue flower", [133, 128, 177]), ("bluish green", [103, 189, 170]),
    ("orange", [214, 126, 44]), ("purplish blue", [80, 91, 166]),
    ("moderate red", [193, 90, 99]), ("purple", [94, 60, 108]),
    ("yellow green", [157, 188, 64]), ("orange yellow", [224, 163, 46]),
    ("blue", [56, 61, 150]), ("green", [70, 148, 73]),
    ("red", [175, 54, 60]), ("yellow", [231, 199, 31]),
    ("magenta", [187, 86, 149]), ("cyan", [8, 133, 161]),
    ("white", [243, 243, 242]), ("neutral 8", [200, 200, 200]),
    ("neutral 6.5", [160, 160, 160]), ("neutral 5", [122, 122, 121]),
    ("neutral 3.5", [85, 85, 85]), ("black", [52, 52, 52]),
];

const PATCH: usize = 40;

/// The patch at a position on the chart (0..1 across and down).
fn patch_at(x: f32, y: f32) -> usize {
    ((y * 4.0) as usize * 6 + (x * 6.0) as usize).min(23)
}

/// A patch's reference colour, display-linear.
fn reference(k: usize) -> [f32; 3] {
    CHART[k].1.map(|v| kit::srgb_decode_scalar(v as f32 / 255.0))
}

/// The chart as a raw records it: the patches' reflectances under a light of
/// `light` (per channel), exposed `stops` from where a camera meters it.
fn photographed(light: [f32; 3], stops: f32) -> Image {
    let scale = 2f32.powf(-HEADROOM_STOPS + stops);
    scene(6 * PATCH, 4 * PATCH, |x, y, i| {
        let r = reference(patch_at(x, y));
        std::array::from_fn(|c| r[c] * light[c] * scale * grain(i, 0.02))
    })
}

/// The chart under neutral light, normally metered, as developed with its
/// exposure gain (EV): shared by the tests, which all compare against it.
fn neutral() -> &'static (Image, f32) {
    static NEUTRAL: OnceLock<(Image, f32)> = OnceLock::new();
    NEUTRAL.get_or_init(|| shot(0.0))
}

/// The chart under neutral light `stops` from normal metering, developed,
/// with the exposure gain the pipeline chose.
fn shot(stops: f32) -> (Image, f32) {
    let mut dev = development(photographed([1.0; 3], stops), Meta::default());
    let (out, report) = dev.render(&Settings::default(), Some(600), true, None);
    (out, report.to_json()["exposure"]["gain_ev"].as_f64().unwrap() as f32)
}

/// Each patch's mean, display-linear, from its middle so edges do not count.
fn patches(img: &Image) -> Vec<[f32; 3]> {
    (0..24).map(|k| {
        let (col, row) = ((k % 6) as f32, (k / 6) as f32);
        mean(img, (col + 0.25) / 6.0, (row + 0.25) / 4.0, (col + 0.75) / 6.0, (row + 0.75) / 4.0)
    }).collect()
}

/// The chart drawn from 24 colours, for looking at.
fn drawn(colours: &[[f32; 3]]) -> Image {
    scene(6 * PATCH, 4 * PATCH, |x, y, _| colours[patch_at(x, y)])
}

fn dump(name: &str, img: &Image) {
    if let Ok(dir) = std::env::var("LUMIRAW_DUMP") {
        let bytes = lumiraw_core::output::save(img, "png8", 90, &[], None).expect("png");
        std::fs::write(format!("{dir}/{name}.png"), bytes).expect("written");
    }
}

/// Oklab ΔE×100 between two display-linear colours: about 2 is a just
/// noticeable difference.
fn delta_e(p: [f32; 3], q: [f32; 3]) -> f32 {
    let (l0, a0, b0) = kit::rgb_to_oklab_px(p[0], p[1], p[2]);
    let (l1, a1, b1) = kit::rgb_to_oklab_px(q[0], q[1], q[2]);
    ((l1 - l0).powi(2) + (a1 - a0).powi(2) + (b1 - b0).powi(2)).sqrt() * 100.0
}

/// Neutral light, normal metering: the published chart is what comes out,
/// up to the tone curve.
#[test]
fn the_chart_comes_out_as_published() {
    let got = patches(&neutral().0);
    let want: Vec<[f32; 3]> = (0..24).map(reference).collect();
    dump("chart_expected", &drawn(&want));
    dump("chart_developed", &drawn(&got));

    println!("{:<14} {:>14} {:>14} {:>7} {:>7} {:>8}", "patch", "expected sRGB", "developed",
             "dL", "hue°", "chroma");
    let (mut hue_errors, mut chroma_ratios) = (Vec::new(), Vec::new());
    for k in 0..24 {
        let ((le, ce, he), (lg, cg, hg)) = (lch(want[k]), lch(got[k]));
        let hue = hue_gap(hg, he);
        let code8 = |px: [f32; 3]| px.map(|v| code(v).round() as u8);
        let (shown_hue, shown_chroma) = if ce > 0.03 { (hue, cg / ce) } else { (0.0, 0.0) };
        println!("{:<14} {:>14} {:>14} {:>+7.3} {:>7.1} {:>8.2}", CHART[k].0,
                 format!("{:?}", code8(want[k])), format!("{:?}", code8(got[k])), lg - le,
                 shown_hue, shown_chroma);
        if k < 18 {
            hue_errors.push(hue.abs());
            chroma_ratios.push(cg / ce);
        } else {
            assert!(cg < 0.006, "{} came out tinted: chroma {cg}", CHART[k].0);
        }
    }
    // The six greys keep their order, white to black.
    let greys: Vec<f32> = (18..24).map(|k| lch(got[k]).0).collect();
    assert!(greys.windows(2).all(|w| w[0] > w[1] + 0.03), "greys out of order: {greys:?}");
    // Every colour keeps its hue, and its chroma within the look's range.
    let worst_hue = hue_errors.iter().cloned().fold(0.0, f32::max);
    let mean_hue = kit::mean(&hue_errors);
    println!("hue error: mean {mean_hue:.1}°, worst {worst_hue:.1}°; chroma ratio {:.2}..{:.2}",
             chroma_ratios.iter().cloned().fold(f32::MAX, f32::min),
             chroma_ratios.iter().cloned().fold(0.0, f32::max));
    assert!(mean_hue < 4.0 && worst_hue < 10.0, "hues moved: mean {mean_hue}, worst {worst_hue}");
    assert!(chroma_ratios.iter().all(|r| (0.75..=1.5).contains(r)),
            "chroma off the chart's: {chroma_ratios:?}");
}

/// The same chart under a warm and a cool light, as a camera set to a fixed
/// balance records it: the automatic white balance takes a cast within its
/// reach out, and the chart comes out as it does under neutral light.
#[test]
fn a_cast_chart_comes_out_as_the_neutral_one() {
    let neutral = patches(&neutral().0);
    for (name, light) in [("warm", [1.15f32, 1.0, 0.85]), ("cool", [0.87, 1.0, 1.15])] {
        let out = develop(photographed(light, 0.0));
        dump(&format!("chart_{name}_light_developed"), &out);
        let got = patches(&out);
        let errors: Vec<f32> = (0..24).map(|k| delta_e(neutral[k], got[k])).collect();
        let mean = kit::mean(&errors);
        let worst = errors.iter().cloned().fold(0.0, f32::max);
        println!("{name} light: Oklab ΔE×100 against the neutral-light chart mean {mean:.2}, \
                  worst {worst:.2}");
        assert!(mean < 2.0 && worst < 4.0, "{name}: mean {mean}, worst {worst}");
    }
    // Further than the balance reaches (a tungsten preset in daylight, say),
    // the cast is reported, not asserted: refining stops at half a stop off.
    let far_light = [1.25f32, 1.0, 0.75];
    let far = patches(&develop(photographed(far_light, 0.0)));
    let greys_chroma = |grey: &dyn Fn(usize) -> [f32; 3]| {
        (18..24).map(|k| lch(grey(k)).1).sum::<f32>() / 6.0
    };
    let before = greys_chroma(&|k| { let r = reference(k); std::array::from_fn(|c| r[c] * far_light[c]) });
    println!("a cast beyond reach: greys' chroma {:.3} after, {before:.3} before",
             greys_chroma(&|k| far[k]));
}

/// A stop under or over, the automatic exposure corrects half of it, by
/// design: the frame keeps half a stop of the photographer's departure.
#[test]
fn a_stop_off_is_corrected_by_half() {
    // The gain, and neutral 5 on screen.
    let read = |(out, gain): &(Image, f32)| (*gain, kit::luminance_px(&patches(out)[21]));
    let ((g_under, y_under), (g_normal, y_normal), (g_over, y_over)) =
        (read(&shot(-1.0)), read(neutral()), read(&shot(1.0)));
    let (lift, cut) = (g_under - g_normal, g_normal - g_over);
    println!("exposure gain a stop under / over, against normal: {lift:+.2} / {cut:+.2} stops; \
              neutral 5 on screen {:+.2} / {:+.2} stops (the tone curve's midtone slope)",
             (y_under / y_normal).log2(), (y_over / y_normal).log2());
    assert!((lift - 0.5).abs() < 0.05 && (cut - 0.5).abs() < 0.05,
            "a stop off was corrected {lift:+.2} / {cut:+.2}, not by half");
}

/// Developing again gives the same picture: what is measured is the
/// picture, not the run.
#[test]
fn the_chart_is_developed_the_same_every_time() {
    assert_eq!(develop(photographed([1.0; 3], 0.0)).d, neutral().0.d);
}
