//! Each stage of the pipeline on its own, against what it must never do
//! whatever it decides: tint a grey, reverse the order of two greys, push a
//! colour off its hue, or send a channel out of range.

mod common;
use common::*;
use kit::{Image, Report};
use lumiraw_core::analyze::HIGHLIGHT_CEILING;
use lumiraw_core::grade::{self, Decisions, Preset, Settings};

// The stages, each run as the pipeline runs it, with default settings and the
// thumbnail it measures on.

fn white_balance(img: &mut Image) {
    let mut thumb = kit::thumbnail(img, 1024);
    grade::apply_white_balance(img, &mut thumb, &Settings::default(), &Preset::default(), None,
                               &mut Decisions::default(), false, &mut Report::new());
}

fn exposure(img: &mut Image) -> f32 {
    let thumb = kit::thumbnail(img, 1024);
    grade::apply_exposure(img, &thumb, &Settings::default(), &Preset::default(), None,
                          &mut Decisions::default(), false, &mut Report::new())
}

fn tone_map(img: &mut Image) {
    let (thumb, long) = (kit::thumbnail(img, 1024), img.w.max(img.h));
    grade::apply_local_tone_map(img, &thumb, &Settings::default(), &Preset::default(), long,
                                &mut Decisions::default(), false, &mut Report::new());
}

fn perceptual(img: &Image) -> Image {
    let (noise, sharpen_sigma, focus_sigma, render_scale) = (0.1, 1.0, 0.0, 1.0);
    grade::apply_perceptual(img, &Settings::default(), &Preset::default(), noise, sharpen_sigma,
                            focus_sigma, render_scale, img.w.max(img.h),
                            &mut Decisions::default(), false, &mut Report::new(), None)
}

/// Scene light brought to display levels, where middle grey sits at the
/// target, as the exposure stage leaves it.
fn at_display_levels(mut img: Image) -> Image {
    img.scale(Preset::default().target_key / grey());
    img
}

/// Greys from deep shadow to just under the sensor's top, smooth enough that
/// no local stage has an edge to work on: about 10 stops across.
fn grey_ramp() -> Image {
    scene(440, 60, |x, _, _| [1e-3 * 2f32.powf(9.8 * x); 3])
}

/// A chart: the colour patches on the top half, greys on the bottom.
fn chart() -> Image {
    scene(500, 200, |x, y, i| {
        let k = ((x * 5.0) as usize).min(4);
        let level = 2.0 * grey() * grain(i, 0.15);
        if y < 0.5 { PATCHES[k].map(|v| v * level) } else { [level * (0.5 + k as f32 * 0.4); 3] }
    })
}

/// The chroma of the chart's greys.
fn chart_grey_chroma(img: &Image) -> f32 {
    lch(mean(img, 0.0, 0.55, 1.0, 0.95)).1
}

/// The largest Oklab chroma anywhere in the image.
fn worst_chroma(img: &Image) -> f32 {
    img.px().iter().map(|p| lch(*p).1).fold(0.0, f32::max)
}

/// How far each colour patch's mean hue turned between two charts.
fn hue_shifts(before: &Image, after: &Image) -> Vec<f32> {
    (0..5).map(|k| {
        let x = (k as f32 + 0.5) / 5.0;
        let (_, _, a) = lch(mean(before, x - 0.05, 0.1, x + 0.05, 0.4));
        let (_, _, b) = lch(mean(after, x - 0.05, 0.1, x + 0.05, 0.4));
        hue_gap(b, a)
    }).collect()
}

// White balance

#[test]
fn white_balance_leaves_a_neutral_scene_alone() {
    let mut img = grey_ramp();
    let before = img.clone();
    white_balance(&mut img);
    for (a, b) in img.d.iter().zip(&before.d) {
        assert!((a - b).abs() <= 1e-4 * b.max(1e-3), "a grey scene was rebalanced");
    }
}

#[test]
fn white_balance_keeps_the_greys_beside_colours_grey() {
    let mut img = chart();
    white_balance(&mut img);
    let chroma = chart_grey_chroma(&img);
    assert!(chroma < 0.003, "the chart's greys were tinted: chroma {chroma}");
}

// Exposure

#[test]
fn exposure_is_one_gain_on_every_channel_and_keeps_the_highlights() {
    let mut img = grey_ramp();
    let before = img.clone();
    let gain = exposure(&mut img);
    for (a, b) in img.d.iter().zip(&before.d) {
        assert!((a - b * gain).abs() <= 1e-5 * a.abs().max(1e-3), "not one gain");
    }
    let top = kit::percentile(&img.d, 99.5);
    assert!(top <= HIGHLIGHT_CEILING * 1.01, "the brightest greys were taken to {top}");
}

// Local tone map

#[test]
fn the_tone_map_keeps_greys_grey_and_in_order() {
    let mut img = grey_ramp();
    tone_map(&mut img);
    assert!(worst_chroma(&img) < 1e-3, "the tone map tinted a grey");
    keeps_order(middle_row(&img), 0.01).unwrap();
}

#[test]
fn the_tone_map_keeps_hue() {
    let mut img = chart();
    let before = img.clone();
    tone_map(&mut img);
    for (k, shift) in hue_shifts(&before, &img).iter().enumerate() {
        assert!(shift.abs() < 1.0, "patch {k} turned {shift} degrees");
    }
}

// Filmic

#[test]
fn filmic_keeps_greys_grey_in_order_and_in_range() {
    let mut img = scene(800, 4, |x, _, _| [1e-4 * 2f32.powf(16.0 * x); 3]);   // to 6.5
    grade::filmic(&mut img);
    assert!(img.d.iter().all(|v| (0.0..=1.0).contains(v)), "out of range");
    assert!(worst_chroma(&img) < 1e-4, "filmic tinted a grey");
    keeps_order(middle_row(&img), 0.0).unwrap();
}

#[test]
fn filmic_keeps_saturated_colours_in_range_and_on_hue() {
    for colour in [ORANGE, SKY, FOLIAGE, RED] {
        let bright = colour.map(|v| v / colour.iter().cloned().fold(0.0, f32::max));
        let mut img = scene(400, 4, |x, _, _| bright.map(|v| v * 0.02 * 2f32.powf(8.0 * x)));
        let before = img.clone();
        grade::filmic(&mut img);
        let worst = img.d.iter().cloned().fold(f32::MIN, f32::max);
        assert!(worst <= 1.0, "{colour:?} went to {worst}, past white");
        // Hue holds until the shoulder bleaches the brightest toward white.
        for x in (0..img.w / 2).step_by(20) {
            let (_, _, a) = lch(before.px()[x]);
            let (_, _, b) = lch(img.px()[x]);
            assert!(hue_gap(b, a).abs() < 3.0, "{colour:?} turned at {x}: {a} to {b}");
        }
    }
}

// Perceptual: levels, contrast, vibrance, sharpening

#[test]
fn the_perceptual_stage_keeps_greys_grey_in_order_and_in_gamut() {
    let mut img = grey_ramp();
    grade::filmic(&mut img);
    let out = perceptual(&img);
    assert!(out.d.iter().all(|v| (-1e-6..=1.0 + 1e-6).contains(v)), "out of gamut");
    assert!(worst_chroma(&out) < 1e-3, "the perceptual stage tinted a grey");
    keeps_order(middle_row(&out), 0.01).unwrap();
}

#[test]
fn the_perceptual_stage_keeps_hue_and_greys() {
    let mut img = at_display_levels(chart());
    grade::filmic(&mut img);
    let out = perceptual(&img);
    for (k, shift) in hue_shifts(&img, &out).iter().enumerate() {
        assert!(shift.abs() < 4.0, "patch {k} turned {shift} degrees");
    }
    let chroma = chart_grey_chroma(&out);
    assert!(chroma < 0.002, "the perceptual stage tinted the chart's greys: {chroma}");
}

/// Darkening may lose detail, never invent colour: what levels and contrast
/// take from a shadow's lightness they take from its chroma too.
#[test]
fn darkened_shadows_get_no_more_saturated() {
    let mut img = at_display_levels(scene(400, 100, |x, _, i| {
        [0.6, 0.45, 0.4].map(|v| v * 0.01 * (1.0 + 4.0 * x) * grain(i, 0.2))
    }));
    grade::filmic(&mut img);
    let out = perceptual(&img);
    let ratio = |im: &Image| { let (l, c, _) = lch(mean(im, 0.0, 0.0, 0.2, 1.0)); c / l.max(1e-6) };
    let (before, after) = (ratio(&img), ratio(&out));
    assert!(after <= before * 1.05, "saturation of the shadows rose from {before} to {after}");
}

// Resampling

/// Straightening a picture by a degree keeps its fine detail: the sampler
/// does not average neighbouring pixels together wherever it lands between
/// them, which a lens correction or a rotation does all over the frame.
#[test]
fn straightening_keeps_the_fine_detail() {
    use lumiraw_core::geometry::{self, Framing};
    // Fine stripes, five pixels a cycle, across the frame.
    let img = scene(300, 200, |x, _, _| {
        [0.2 + 0.15 * (x * 300.0 * std::f32::consts::TAU / 5.0).sin(); 3]
    });
    let swing = |im: &Image| {
        let row = middle_row(im);
        let (from, to) = (row.len() / 4, row.len() * 3 / 4);
        row[from..to].iter().map(|p| p[1]).fold(0.0, f32::max)
            - row[from..to].iter().map(|p| p[1]).fold(1.0, f32::min)
    };
    let turned = geometry::apply(&img, &Framing { angle: 1.0, ..Framing::default() }, true);
    let (before, after) = (swing(&img), swing(&turned));
    assert!(after > 0.85 * before, "the stripes went from {before} to {after} peak to peak");
}

// White balance as a change of light

#[test]
fn adaptation_keeps_blown_white_and_takes_grey_where_the_gains_say() {
    let gains = [0.8f32, 1.0, 1.25];
    let mut img = Image::new(2, 1);
    img.d.copy_from_slice(&[0.3 / 0.8, 0.3, 0.3 / 1.25, 1.0, 1.0, 1.0]);
    grade::adapt(&mut img, gains);
    let (_, grey, _) = lch(img.px()[0]);
    let (_, white, _) = lch(img.px()[1]);
    assert!(grey < 1e-3, "the grey did not land on grey: {:?}", img.px()[0]);
    assert!(white < 1e-4, "blown white was tinted: {:?}", img.px()[1]);
}
