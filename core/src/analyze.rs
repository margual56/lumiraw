//! Scene analysis: every number the pipeline needs is measured here.

use kit::{round_to, Image, Plane};
use serde_json::{json, Value};

pub const EPS: f32 = 1e-6;

/// Scene light dim enough that nothing survives to the exported file.
pub const DISPLAY_FLOOR: f32 = 0.016;

/// How much of a frame may be given up to black before the exposure is treated
/// as too dark rather than deliberately dark.
pub const CRUSH_BUDGET: f32 = 0.10;

/// The most the rescue may add on top of the soft limit. Bounded because
/// lifting shadows lifts their noise with them.
pub const MAX_RESCUE_EV: f32 = 3.5;

/// Oklab lightness that still reaches the first code value of an 8-bit file.
/// Below this a pixel is black however much detail it holds.
pub const BLACK_LIGHTNESS: f32 = 0.0534;

/// The darkest a frame without a true white may have its white point put: a
/// white point is a stretch of the whole picture, and a dim scene is allowed
/// to stay dim. About a quarter of a stop.
const LOWEST_WHITE: f32 = 0.95;

/// Map an Oklab lightness through a black and white point, in linear light.
///
/// Lightness is close to the cube root of luminance, so the same offset taken
/// off L is several stops in the shadows; taken off luminance it removes a
/// pedestal and leaves the shadows' gradation alone.
#[inline]
pub fn levels(l: f32, black: f32, white: f32) -> f32 {
    let (yb, yw) = (black.powi(3), white.powi(3));
    let y = l.max(0.0).powi(3);
    ((y - yb) / (yw - yb).max(1e-6)).clamp(0.0, 1.0).cbrt()
}

const THIRD_STOP: f32 = 1.259_921_f32;

/// Smoothly saturate at ±limit instead of clipping hard.
pub fn soft_limit(x: f32, limit: f32) -> f32 {
    limit * (x / limit).tanh()
}


// White balance
/// How much of the automatic white balance's green-magenta correction is
/// kept; see `auto_white_balance`. Green-magenta is rarely the light's own
/// doing, but with an estimate taken from neutral surfaces it can be trusted
/// for more than half.
pub const TINT_TRUST: f32 = 0.6;

/// How far from the camera's own neutral (log2 of R/G and B/G) a pixel may
/// sit and still be taken for a grey surface: wide on the first pass, narrow
/// once the estimate has settled.
const GREY_REACH: [f32; 4] = [0.5, 0.35, 0.25, 0.25];

/// The share of the measured pixels that must look grey before the estimate
/// is trusted at all, and the share at which it is trusted fully.
const GREYS_NEEDED: (f32, f32) = (0.02, 0.15);

/// Corrections up to this size (or 60 % of the limit, if less) are delivered
/// in full; beyond it the soft limit takes over.
const WB_KNEE_EV: f32 = 0.5;

/// Refine the camera's white balance from the surfaces that look grey.
///
/// Only pixels near the camera's neutral take part, weighted by how near, and
/// the estimate is refined over a few passes. A blue sky, a red rock or a
/// field of flowers is not a grey surface under a coloured light, and letting
/// it vote (as a shades-of-grey estimate does) tints the whole picture the
/// opposite way. Too few greys, and the camera's balance stands.
pub fn auto_white_balance(thumb: &Image, strength: f32, limit_ev: f32) -> ([f32; 3], Value) {
    let y = kit::luminance(thumb);
    let pct = kit::percentiles(&y.d, &[3.0, 97.0]);
    let (lo, hi) = (pct[0].max(EPS), pct[1]);
    let candidates: Vec<usize> = (0..y.d.len())
        .filter(|&i| {
            let px = &thumb.px()[i];
            y.d[i] > lo && y.d[i] < hi && px[0].max(px[1]).max(px[2]) < 0.98
                && px.iter().all(|v| *v > EPS)
        })
        .collect();
    if candidates.len() < 64 {
        return ([1.0, 1.0, 1.0], json!({"applied": false}));
    }

    let mut light = [1.0f32; 3];
    let mut greys = 0.0f32;
    for reach in GREY_REACH {
        let (mut sum, mut total, mut count) = ([0f64; 3], 0f64, 0usize);
        for &i in &candidates {
            let px = &thumb.px()[i];
            let rg = (px[0] / light[0] / (px[1] / light[1])).log2();
            let bg = (px[2] / light[2] / (px[1] / light[1])).log2();
            let off = rg.hypot(bg);
            if off >= reach {
                continue;
            }
            let w = (1.0 - off / reach) as f64;
            for c in 0..3 {
                sum[c] += w * (px[c] / y.d[i]) as f64;
            }
            total += w;
            count += 1;
        }
        greys = count as f32 / candidates.len() as f32;
        if total <= 0.0 {
            break;
        }
        light = sum.map(|v| (v / total) as f32);
        let l = kit::luminance_px(&light).max(EPS);
        light = light.map(|v| v / l);
    }
    let trust = kit::smoothstep(GREYS_NEEDED.0, GREYS_NEEDED.1, greys);
    if trust <= 0.0 {
        return ([1.0, 1.0, 1.0], json!({"applied": false, "greys": round_to(greys, 3)}));
    }

    // Into warm-cool and green-magenta, which are not equally to be trusted
    // and are limited together, as one correction: limited channel by channel,
    // the unequal compression of red and blue would turn part of a warm-cool
    // correction into green or magenta.
    let log_gain = light.map(|v| -v.max(EPS).log2());
    let temp = (log_gain[0] - log_gain[2]) / 2.0;                         // along (1, 0, -1)
    let tint = (2.0 * log_gain[1] - log_gain[0] - log_gain[2]) / 3.0;     // along (-1/2, 1, -1/2)
    let (temp, tint) = (temp * strength * trust, tint * TINT_TRUST * strength * trust);
    let size = temp.hypot(tint);
    let knee = WB_KNEE_EV.min(0.6 * limit_ev);
    let limited = if size <= knee { size } else { knee + soft_limit(size - knee, limit_ev - knee) };
    let k = if size > 0.0 { limited / size } else { 0.0 };
    let (temp, tint) = (temp * k, tint * k);
    let mut log_gain = [temp - 0.5 * tint, tint, -temp - 0.5 * tint];
    let lw = [0.2126f32, 0.7152, 0.0722];
    let weighted: f32 = (0..3).map(|c| log_gain[c] * lw[c]).sum::<f32>() / lw.iter().sum::<f32>();
    for g in log_gain.iter_mut() {
        *g -= weighted;
    }
    let gains = log_gain.map(|g| g.exp2());
    (
        gains,
        json!({"applied": true,
               "gains": gains.map(|g| round_to(g, 4)),
               "shift_ev": log_gain.map(|g| round_to(g, 3)),
               "greys": round_to(greys, 3)}),
    )
}

// Exposure
/// Where a normally metered raw lands against the target key: the file is
/// scaled to the sensor's white level, which sits two and a half stops or so
/// above what the camera metered for, to keep highlight headroom. The soft
/// limit restrains departures from this, not from zero, or every ordinary
/// frame is held a fraction of a stop dark.
pub const METERED_EV: f32 = 2.5;

/// How far past white the highlight guard lets the brightest highlights go
/// (the 99.5th percentile of what it measures): the filmic shoulder rolls the
/// last fifth off without clipping it.
pub const HIGHLIGHT_CEILING: f32 = 1.20;

/// How far above the frame's key a pixel is taken for a light source or a
/// specular glint rather than a highlight to keep: six stops.
const SOURCE_ABOVE_KEY: f32 = 64.0;

/// Light sources covering more of the frame than this are not small: a sky
/// six stops over a backlit subject is a highlight to keep.
const SMALL_SOURCES: f32 = 0.03;

/// What the highlight guard keeps below the shoulder: luminance, or the
/// brightest channel at a discount, whichever is higher, so a saturated sky
/// whose blue would clip counts even though its luminance would not.
#[inline]
fn highlight_measure(px: &[f32; 3]) -> f32 {
    kit::luminance_px(px).max(0.6 * px[0].max(px[1]).max(px[2]))
}

/// Match the log-average luminance to middle grey, partially, under a
/// highlight guard; lift a buried frame only as far as the guard allows.
///
/// The guard is never waived. When the shadows and the highlights want
/// different exposures, the highlights win here and the shadows are left to
/// the tone map, which can lift them without moving the highlights.
pub fn auto_exposure(thumb: &Image, target_key: f32, strength: f32, limit_ev: f32) -> (f32, Value) {
    let y = kit::luminance(thumb);
    // The key is the body of the frame: the deepest shadows and the brightest
    // highlights (a window, the sky, a lamp, which the guard below looks
    // after) are left out of it, or a small bright window holds a correctly
    // exposed room a fraction of a stop dark.
    let ends = kit::percentiles(&y.d, &[1.0, 95.0]);
    let (floor, top) = (ends[0].max(1e-5), ends[1]);
    let sample: Vec<f32> = y.d.iter().cloned().filter(|v| *v > floor && *v <= top).collect();
    if sample.len() < 64 {
        return (1.0, json!({"applied": false}));
    }
    let log_mean: f64 = sample.iter().map(|v| v.log2() as f64).sum::<f64>() / sample.len() as f64;
    let key = (log_mean as f32).exp2();
    let wanted = (target_key / key.max(EPS)).log2() * strength;
    let delta = METERED_EV + soft_limit(wanted - METERED_EV, limit_ev);
    let key_gain = delta.exp2();

    // The highlight guard, on what the shoulder has to hold. Pixels the raw
    // already clipped say nothing about how far to go, and neither do small
    // light sources far above everything else.
    let measure: Vec<f32> = thumb.px().iter().map(highlight_measure).collect();
    let n = measure.len() as f32;
    let clipped = y.d.iter().filter(|v| **v >= 0.99).count() as f32 / n;
    let sources = key * SOURCE_ABOVE_KEY;
    let small = (measure.iter().filter(|m| **m > sources).count() as f32 / n) < SMALL_SOURCES;
    let kept: Vec<f32> = measure.iter().cloned()
        .filter(|m| *m < 0.99 && (!small || *m <= sources))
        .collect();
    let cap = if kept.len() >= 64 {
        HIGHLIGHT_CEILING / kit::percentile(&kept, 99.5).max(EPS)
    } else {
        f32::INFINITY
    };
    let mut gain = key_gain.min(cap);

    // Rescue, rather than restraint: the soft limit keeps a deliberately dark
    // frame dark, which is right for a night portrait and wrong for a frame
    // that is simply underexposed. Never past the guard.
    let buried = |g: f32| {
        y.d.iter().filter(|v| **v * g < DISPLAY_FLOOR).count() as f32 / n
    };
    let mut rescue_ev = 0.0f32;
    while rescue_ev < MAX_RESCUE_EV && buried(gain) > CRUSH_BUDGET && gain * THIRD_STOP <= cap {
        gain *= THIRD_STOP;
        rescue_ev += 1.0 / 3.0;
    }
    gain = gain.clamp(0.125, 2f32.powf(METERED_EV + limit_ev + MAX_RESCUE_EV));
    (
        gain,
        json!({"applied": true, "key": round_to(key, 5), "gain_ev": round_to(gain.log2(), 3),
               "highlight_guard_ev": (key_gain > cap)
                   .then(|| round_to((key_gain / cap).log2(), 3)),
               "rescue_ev": round_to(rescue_ev, 2),
               "buried_after": round_to(buried(gain), 4),
               "small_sources_ignored": small && measure.iter().any(|m| *m > sources),
               "clipped_fraction": round_to(clipped, 5)}),
    )
}

// Dynamic range
/// How hard to squeeze the scene's range into the display's.
pub fn auto_tone_compression(thumb: &Image, comfortable_stops: f32, floor: f32) -> (f32, Value) {
    let y = kit::luminance(thumb);
    let logs: Vec<f32> = y.d.iter().map(|v| v.max(EPS).log2()).collect();
    let p = kit::percentiles(&logs, &[2.0, 99.5]);
    let stops = p[1] - p[0];
    let factor = (comfortable_stops / stops.max(EPS)).clamp(floor, 1.0);
    (
        factor,
        json!({"scene_stops": round_to(stops, 2), "base_compression": round_to(factor, 3)}),
    )
}

// Display-referred tone
/// Robust black/white point from percentiles, each moved the same fraction of
/// the way, for `levels` to apply in linear light. A raw has had its black
/// level taken off already, so the black point only ever removes a pedestal.
///
/// The white point is read from `peak`, the lightness of each pixel's
/// brightest channel: a saturated sky can sit well below white in lightness
/// with its blue already at the top, and stretching it would clip the blue.
pub fn auto_levels(lightness: &Plane, peak: &Plane, strength: f32, clip_pct: f64)
    -> (f32, f32, Value) {
    let black = (kit::percentile(&lightness.d, clip_pct) * strength).max(0.0);
    let top = kit::percentile(&peak.d, 100.0 - clip_pct);
    let white = (1.0 - (1.0 - top) * strength).max(LOWEST_WHITE);

    if white - black < 0.25 {
        return (0.0, 1.0, json!({"applied": false}));
    }
    let stretch = 1.0 / (white.powi(3) - black.powi(3));
    (
        black,
        white,
        json!({"applied": true, "black": round_to(black, 4), "white": round_to(white, 4),
               "stretch": round_to(stretch, 3)}),
    )
}

/// How much of a look's curve every frame gets, whatever it measures. Small:
/// the filmic curve already carries a contrast look of its own.
const CURVE_FLOOR: f32 = 0.60;

/// S-curve strength and its pivot. The curve only adds midtone contrast (see
/// `grade::s_curve`), so it is asked for by how flat the frame is and nothing
/// else: a frame the tone map compressed is compressed on purpose, and a curve
/// that steepened it again would put back the crushed shadows and blown
/// highlights the tone map took out.
pub fn auto_contrast(lightness: &Plane, target_spread: f32, max_amount: f32)
    -> (f32, f32, Value) {
    let spread = kit::std_dev(&lightness.d);
    // How flat the histogram is.
    let flat = ((target_spread - spread) / target_spread).clamp(0.0, 1.0);
    let want = CURVE_FLOOR + (1.0 - CURVE_FLOOR) * flat;
    let amount = want * max_amount;
    // Pivoting far from the middle bends one end of the range hard.
    let pivot = kit::median(&lightness.d).clamp(0.3, 0.65);
    (
        amount,
        pivot,
        json!({"spread": round_to(spread, 4), "s_curve": round_to(amount, 3),
               "pivot": round_to(pivot, 3), "flat": round_to(flat, 3)}),
    )
}

// Colour
/// Chroma below which a pixel is taken as neutral: what is left there is a
/// white balance error or noise, and neither should be boosted.
pub const NEUTRAL_CHROMA: f32 = 0.02;

/// Saturation boost from how colourful the frame already is, measured on the
/// 80th percentile of chroma over well-lit coloured pixels. A frame of mostly
/// greys is not asking for colour; counting them would make it ask hardest.
pub fn auto_vibrance(chroma: &Plane, lightness: &Plane, target_chroma: f32, max_boost: f32)
    -> (f32, Value) {
    let sel: Vec<f32> = (0..chroma.d.len())
        .filter(|i| lightness.d[*i] > 0.15 && lightness.d[*i] < 0.95
                    && chroma.d[*i] > NEUTRAL_CHROMA)
        .map(|i| chroma.d[i])
        .collect();
    if sel.len() < 64 {
        return (1.0, json!({"applied": false}));
    }
    let reference = kit::percentile(&sel, 80.0);
    let boost = (target_chroma / reference.max(1e-4)).clamp(1.0, max_boost);
    (
        boost,
        json!({"applied": true, "chroma_p80": round_to(reference, 4), "boost": round_to(boost, 3)}),
    )
}

/// Luminance on a square-root scale, roughly perceptual.
pub fn sqrt_luminance(img: &Image) -> Plane {
    let mut luma = kit::luminance(img);
    for v in luma.d.iter_mut() {
        *v = v.max(0.0).sqrt();
    }
    luma
}

/// The noise floor of a frame's centre `size` pixels square, at the frame's own
/// resolution.
pub fn centre_noise_floor(img: &Image, size: usize) -> f32 {
    let (w, h) = (size.min(img.w), size.min(img.h));
    noise_floor(&sqrt_luminance(&img.crop((img.w - w) / 2, (img.h - h) / 2, w, h)))
}

/// The noise floor of a frame: the local variation left where the picture is
/// flattest, which is the one place the signal cannot be mistaken for texture.
pub fn noise_floor(lightness: &Plane) -> f32 {
    let blurred = kit::gaussian_blur(lightness, 1.0);
    let mut sq = Plane::new(lightness.w, lightness.h);
    for i in 0..sq.d.len() {
        let r = lightness.d[i] - blurred.d[i];
        sq.d[i] = r * r;
    }
    let local = kit::box_blur(&sq, 4);
    let rms: Vec<f32> = local.d.iter().map(|v| v.max(0.0).sqrt()).collect();
    kit::percentile(&rms, 8.0)
}

/// Turn a measured floor into the level the rest of the pipeline reasons
/// about, bounded by what the sensor and ISO make physically plausible.
pub fn noise_from_floor(floor: f32, iso: f32, prior: f32) -> (f32, Value) {
    let measured = (floor * 55.0).clamp(0.0, 1.0);
    let level = prior.max(measured.min(prior + 0.35)).clamp(0.0, 1.0);
    (
        level,
        json!({"iso": iso, "flat_area_noise": round_to(floor, 5), "measured": round_to(measured, 3),
               "prior": round_to(prior, 3), "noise_level": round_to(level, 3)}),
    )
}

// Focus
/// The size the frame is measured at, whatever the sensor's.
pub const FOCUS_LONG_EDGE: usize = 1800;
const FOCUS_TILE: usize = 96;
/// How far an edge is followed when measuring how far it climbs.
const FOCUS_REACH: i32 = 18;
/// How many pixels either side of the crest must climb the same way before
/// this is an edge rather than a coincidence.
const FOCUS_RUN: i32 = 3;
/// How much structure a tile needs, as a multiple of the frame's noise floor.
const FOCUS_STRUCTURE: f32 = 2.5;
/// How many tiles have to produce an answer before there is an answer.
const FOCUS_MIN_TILES: usize = 3;
/// How many edges a tile needs before its median means anything.
const FOCUS_MIN_EDGES: usize = 10;

/// How wide the edges are in the sharpest part of the frame, in pixels of the
/// working size, or `None` when the frame does not hold enough measurable edge
/// to say.
pub fn focus_width(img: &Image) -> Option<f32> {
    let scale = (FOCUS_LONG_EDGE as f32 / img.w.max(img.h) as f32).min(1.0);
    let small = kit::resize_rgb(img, ((img.w as f32 * scale) as usize).max(1),
                                ((img.h as f32 * scale) as usize).max(1));
    // The perceptual scale the noise estimator uses, or this would only ever
    // describe the sky.
    let mut luma = sqrt_luminance(&small);
    let top = kit::percentile(&luma.d, 99.5).max(1e-6);
    for v in luma.d.iter_mut() {
        *v /= top;
    }
    let floor = noise_floor(&luma);

    let mut widths: Vec<f32> = Vec::new();
    for ty in (0..luma.h.saturating_sub(FOCUS_TILE)).step_by(FOCUS_TILE) {
        for tx in (0..luma.w.saturating_sub(FOCUS_TILE)).step_by(FOCUS_TILE) {
            let t = luma.crop(tx, ty, FOCUS_TILE, FOCUS_TILE);
            if kit::std_dev(&t.d) < (FOCUS_STRUCTURE * floor).max(0.004) {
                continue;
            }
            if let Some(w) = tile_edge_width(&t, floor) {
                widths.push(w);
            }
        }
    }
    if widths.len() < FOCUS_MIN_TILES {
        return None;
    }
    widths.sort_by(|a, b| a.partial_cmp(b).unwrap());
    // The sharpest tile the frame has.
    Some(widths[0])
}

/// The median edge width of one tile, or `None` if it holds too few edges.
fn tile_edge_width(p: &Plane, floor: f32) -> Option<f32> {
    let mut widths: Vec<f32> = Vec::new();
    let sample = |x: f32, y: f32| -> f32 {
        let xi = (x.round() as isize).clamp(0, p.w as isize - 1) as usize;
        let yi = (y.round() as isize).clamp(0, p.h as isize - 1) as usize;
        p.d[yi * p.w + xi]
    };
    for y in 2..p.h - 2 {
        for x in 2..p.w - 2 {
            let i = y * p.w + x;
            // Centred over two pixels rather than one.
            let gx = (p.d[i + 1] - p.d[i - 1]) * 0.5;
            let gy = (p.d[i + p.w] - p.d[i - p.w]) * 0.5;
            let g = gx.hypot(gy);
            // Only enough to give the climb a direction.
            if g < (floor * 6.0).max(1e-5) {
                continue;
            }
            let (ux, uy) = (gx / g, gy / g);
            let along = |k: f32| sample(x as f32 + ux * k, y as f32 + uy * k);
            // Only the crest of the ridge: without this every pixel of a wide
            // ramp is an edge, and the widest ramps would be counted most.
            let slope = |k: f32| (along(k + 1.0) - along(k - 1.0)).abs() * 0.5;
            if slope(1.0) > g || slope(-1.0) > g {
                continue;
            }
            // Monotonic, not strictly climbing.
            let mut rising = 0;
            let mut falling = 0;
            for k in -FOCUS_RUN..FOCUS_RUN {
                let step = along((k + 1) as f32) - along(k as f32);
                if step >= 0.0 {
                    rising += 1;
                }
                if step <= 0.0 {
                    falling += 1;
                }
            }
            // One step of the six may go the wrong way.
            if rising.max(falling) < (2 * FOCUS_RUN) - 1 {
                continue;
            }
            let (mut lo, mut hi) = (along(0.0), along(0.0));
            for k in 1..=FOCUS_REACH {
                let (a, b) = (along(-(k as f32)), along(k as f32));
                lo = lo.min(a).min(b);
                hi = hi.max(a).max(b);
            }
            let height = hi - lo;
            if height < (floor * 8.0).max(1e-4) {
                continue;
            }
            widths.push((height / g).clamp(0.5, 24.0));
        }
    }
    if widths.len() < FOCUS_MIN_EDGES {
        return None;
    }
    widths.sort_by(|a, b| a.partial_cmp(b).unwrap());
    // The tile's median edge, not its sharpest.
    Some(kit::percentile_sorted(&widths, 50.0))
}

/// What a sharp frame measures, and the width past which one is soft enough to
/// be worth offering to rescue.
pub const FOCUS_SHARP: f32 = 3.4;
pub const FOCUS_SOFT: f32 = 5.5;

/// Turn a measured edge width into the blur that would explain it.
pub fn focus_sigma(width: f32) -> f32 {
    ((width - 2.4) / 2.3).max(0.0)
}

#[cfg(test)]
mod tests {
    /// Temperature and tint of a set of gains, log2.
    fn temp_tint(gains: [f32; 3]) -> (f32, f32) {
        let log = gains.map(|g| g.log2());
        ((log[0] - log[2]) / 2.0, (2.0 * log[1] - log[0] - log[2]) / 3.0)
    }

    /// Greys under a light that leans cool and magenta.
    #[test]
    fn automatic_balance_trusts_temperature_more_than_tint() {
        let (w, h) = (64usize, 64usize);
        let mut thumb = Image::new(w, h);
        for (i, px) in thumb.d.chunks_exact_mut(3).enumerate() {
            let v = 0.05 + 0.6 * (i % 97) as f32 / 97.0;
            // Cool (more blue than red) and magenta (less green than both).
            px.copy_from_slice(&[v, v * 0.9, v * 1.2]);
        }
        let (gains, _) = auto_white_balance(&thumb, 1.0, 2.0);
        let (temp, tint) = temp_tint(gains);
        // The cast is 0.13 warm-cool and 0.19 green-magenta, log2.
        assert!((temp - 0.13).abs() < 0.02, "the blue was not taken out: {gains:?}");
        assert!((tint - TINT_TRUST * 0.19).abs() < 0.02,
                "tint should be corrected by {TINT_TRUST} of the cast: {gains:?}");
    }

    /// Colourful surfaces under a neutral light: the greys are already grey,
    /// and the sky and the rock must not talk the balance into a tint.
    #[test]
    fn colourful_surfaces_do_not_tint_the_greys() {
        let (w, h) = (80usize, 60usize);
        let mut thumb = Image::new(w, h);
        for (i, px) in thumb.d.chunks_exact_mut(3).enumerate() {
            let v = 0.05 + 0.5 * (i % 89) as f32 / 89.0;
            let colour: [f32; 3] = match i % 5 {
                0 => [0.35, 0.55, 1.0],   // sky
                1 => [1.0, 0.55, 0.3],    // warm rock
                2 => [0.4, 0.8, 0.25],    // foliage
                _ => [1.0, 1.0, 1.0],     // grey
            };
            for c in 0..3 {
                px[c] = v * colour[c];
            }
        }
        let (gains, info) = auto_white_balance(&thumb, 1.0, 0.8);
        let (temp, tint) = temp_tint(gains);
        assert!(temp.abs() < 0.02 && tint.abs() < 0.02,
                "a neutral scene was pushed to {temp} / {tint}: {info}");
    }

    /// A correction purely along warm-cool stays purely along it, however
    /// hard the limit bites.
    #[test]
    fn limiting_a_warm_correction_adds_no_tint() {
        let (w, h) = (64usize, 64usize);
        let mut thumb = Image::new(w, h);
        for (i, px) in thumb.d.chunks_exact_mut(3).enumerate() {
            let v = 0.05 + 0.6 * (i % 97) as f32 / 97.0;
            // Warm: red up and blue down by the same amount, green as is.
            px.copy_from_slice(&[v * 2f32.powf(0.3), v, v * 2f32.powf(-0.3)]);
        }
        // A tight limit, so that it bites.
        let (gains, _) = auto_white_balance(&thumb, 1.0, 0.2);
        let (temp, tint) = temp_tint(gains);
        assert!(temp < -0.12 && temp > -0.2, "the limit did not hold the warmth: {gains:?}");
        assert!(tint.abs() < 0.01, "a warm-cool correction came out with tint {tint}");
    }

    /// Nothing grey to go by, and the camera's balance stands.
    #[test]
    fn no_greys_means_no_correction() {
        let (w, h) = (64usize, 64usize);
        let mut thumb = Image::new(w, h);
        for (i, px) in thumb.d.chunks_exact_mut(3).enumerate() {
            let v = 0.05 + 0.6 * (i % 97) as f32 / 97.0;
            px.copy_from_slice(&[v * 0.4, v * 0.9, v * 0.3]);   // a field of leaves
        }
        let (gains, _) = auto_white_balance(&thumb, 1.0, 0.8);
        assert_eq!(gains, [1.0, 1.0, 1.0]);
    }

    use super::*;

    fn frame(luminance: f32, spread: f32, w: usize, h: usize) -> Image {
        let mut img = Image::new(w, h);
        for (i, px) in img.px_mut().iter_mut().enumerate() {
            // A gentle ramp, so percentiles have something to bite on.
            *px = [luminance * (1.0 + spread * ((i % 97) as f32 / 97.0 - 0.5)); 3];
        }
        img
    }

    /// A plane of a given lightness spread, to ask `auto_contrast` about.
    fn lightness(spread: f32, w: usize, h: usize) -> Plane {
        let mut p = Plane::new(w, h);
        for i in 0..w * h {
            // Two clumps at either end, which is what a wide but flat
            // histogram looks like: a dark subject against a bright sky.
            p.d[i] = if i % 2 == 0 { 0.5 - spread } else { 0.5 + spread };
        }
        p
    }

    /// Every frame gets a curve.
    #[test]
    fn a_contrasty_frame_still_gets_a_curve() {
        let wide = lightness(0.35, 64, 48);
        let (amount, _, info) = auto_contrast(&wide, 0.20, 0.45);
        assert_eq!(info["flat"].as_f64().unwrap(), 0.0,
                   "this frame is past the target spread, so nothing is missing: {info}");
        assert!((amount - 0.45 * CURVE_FLOOR).abs() < 1e-4,
                "it should still get the floor, got {amount}");
    }

    /// A flat frame still gets more curve than a contrasty one.
    #[test]
    fn a_flat_frame_gets_more_curve_than_a_wide_one() {
        let flat = auto_contrast(&lightness(0.02, 64, 48), 0.20, 0.45).0;
        let wide = auto_contrast(&lightness(0.35, 64, 48), 0.20, 0.45).0;
        assert!(flat > wide + 0.1, "flat {flat} should ask for much more than wide {wide}");
        assert!(flat <= 0.45, "nothing may exceed the look's own maximum, got {flat}");
    }

    /// The curve adds midtone contrast and nothing else: both ends keep their
    /// slope, so it cannot press shadows into black or highlights into white.
    #[test]
    fn the_curve_leaves_black_and_white_alone() {
        use crate::grade::s_curve;
        for amount in [0.2f32, 0.45, 0.6] {
            for pivot in [0.3f32, 0.5, 0.65] {
                let slope = |x: f32| (s_curve(x + 1e-3, amount, pivot) - s_curve(x, amount, pivot))
                    / 1e-3;
                assert!(slope(0.002) > 0.8 && slope(0.997) > 0.8,
                        "amount {amount} pivot {pivot}: ends at {} and {}",
                        slope(0.002), slope(0.997));
                assert!(slope(pivot) > 1.05, "no midtone contrast left at {pivot}");
                let mut last = 0.0;
                for i in 0..=1000 {
                    let y = s_curve(i as f32 / 1000.0, amount, pivot);
                    assert!(y >= last, "folds at {}", i as f32 / 1000.0);
                    last = y;
                }
            }
        }
    }

    /// A scene with edges in it, at a size worth measuring.
    fn edged(w: usize, h: usize) -> Image {
        let mut img = Image::new(w, h);
        for y in 0..h {
            for x in 0..w {
                // Bars and blocks, so there are real steps in both directions
                // rather than one ramp the metric could read either way.
                let bar = ((x / 37) % 2 == 0) ^ ((y / 53) % 2 == 0);
                let v = if bar { 0.62 } else { 0.11 };
                for c in 0..3 {
                    img.d[(y * w + x) * 3 + c] = v;
                }
            }
        }
        img
    }

    fn blur_rgb(img: &Image, sigma: f32) -> Image {
        let mut out = Image::new(img.w, img.h);
        for c in 0..3 {
            out.set_plane(c, &kit::gaussian_blur(&img.plane(c), sigma));
        }
        out
    }

    /// The whole point: blurring a frame has to move the reading, and keep
    /// moving it.
    #[test]
    fn blur_widens_the_measured_edge() {
        let sharp = edged(600, 420);
        let a = focus_width(&sharp).expect("a frame of edges is measurable");
        let b = focus_width(&blur_rgb(&sharp, 2.0)).expect("still measurable");
        let c = focus_width(&blur_rgb(&sharp, 5.0)).expect("still measurable");
        assert!(a < b && b < c, "width should rise with blur: {a} {b} {c}");
        assert!(a <= FOCUS_SHARP, "an unblurred frame of hard edges should read sharp: {a}");
        assert!(c >= FOCUS_SOFT, "five pixels of blur should read soft: {c}");
    }

    /// Noise is the trap this measurement exists to avoid falling into.
    #[test]
    fn noise_is_not_mistaken_for_sharpness() {
        let mut noise = Image::new(600, 420);
        let mut seed = 0x9e3779b9u32;
        for px in noise.d.chunks_exact_mut(3) {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let v = 0.35 + 0.25 * ((seed >> 8) as f32 / 16777216.0 - 0.5);
            px.iter_mut().for_each(|c| *c = v);
        }
        match focus_width(&noise) {
            None => {}
            Some(w) => assert!(w > FOCUS_SHARP,
                               "noise must not come back as a sharp frame, got {w}"),
        }
    }

    /// A frame with nothing to measure says so.
    #[test]
    fn a_featureless_frame_gives_no_answer() {
        let mut flat = Image::new(400, 300);
        flat.d.iter_mut().for_each(|v| *v = 0.4);
        assert_eq!(focus_width(&flat), None);
    }

    /// An underexposed frame has to be lifted past the usual restraint, or the
    /// part of it below the display's floor is lost for good.
    #[test]
    fn a_buried_frame_is_rescued() {
        let dark = frame(0.0004, 0.6, 120, 90);
        let (gain, info) = auto_exposure(&dark, 0.13, 0.85, 3.2);
        assert!(info["rescue_ev"].as_f64().unwrap() > 0.0,
                "an underexposed frame should be rescued: {info}");
        assert!(gain.log2() > METERED_EV + 3.2,
                "the rescue should reach past the soft limit, got {} EV", gain.log2());
    }

    /// A rescue may lift the shadows only as far as the highlights allow: a
    /// frame that is mostly deep shadow with a bright window keeps the window.
    #[test]
    fn a_rescue_never_blows_the_highlights() {
        let mut img = Image::new(120, 90);
        for (i, px) in img.px_mut().iter_mut().enumerate() {
            // Half the frame in deep shadow, a grey, and 2 % window.
            let v = if i % 50 == 0 { 0.5 } else if i % 2 == 0 { 0.0008 } else { 0.03 };
            *px = [v; 3];
        }
        let (gain, info) = auto_exposure(&img, 0.13, 1.0, 3.2);
        assert!(0.5 * gain <= HIGHLIGHT_CEILING * 1.01,
                "the window was taken to {}: {info}", 0.5 * gain);
    }

    /// A few lamps in a night scene are light sources, not highlights to keep,
    /// and must not bury the rest of the frame.
    #[test]
    fn small_lamps_do_not_set_the_exposure() {
        let with_lamps = |share: usize| {
            let mut img = Image::new(200, 100);
            for (i, px) in img.px_mut().iter_mut().enumerate() {
                let v = if i % share == 0 { 0.6 } else { 0.004 * (1.0 + (i % 7) as f32 * 0.1) };
                *px = [v; 3];
            }
            auto_exposure(&img, 0.13, 1.0, 3.2).0.log2()
        };
        let (one_percent, half_percent) = (with_lamps(100), with_lamps(250));
        assert!((one_percent - half_percent).abs() < 0.5,
                "1 % of lamps gave {one_percent} EV, 0.4 % gave {half_percent} EV");
        assert!(one_percent > 3.0, "the night was buried at {one_percent} EV");
    }

    /// A blue sky whose luminance fits but whose blue channel would not is
    /// still a highlight to keep.
    #[test]
    fn a_saturated_sky_counts_as_a_highlight() {
        let mut img = Image::new(120, 90);
        for (i, px) in img.px_mut().iter_mut().enumerate() {
            *px = if i % 3 == 0 { [0.12, 0.2, 0.45] } else { [0.02; 3] };
        }
        let (gain, _) = auto_exposure(&img, 0.13, 1.0, 3.2);
        assert!(0.45 * gain * 0.6 <= HIGHLIGHT_CEILING * 1.01,
                "the sky's blue was sent to {}", 0.45 * gain);
    }

    /// An ordinary frame about two and a half stops under, which is where a
    /// raw scaled to its white level lands, is brought up to the target.
    #[test]
    fn a_normally_metered_raw_reaches_the_target() {
        let normal = frame(0.13 / 2f32.powf(METERED_EV), 0.9, 120, 90);
        let (gain, info) = auto_exposure(&normal, 0.13, 1.0, 3.2);
        let key = info["key"].as_f64().unwrap() as f32;
        assert!(((key * gain / 0.13).log2()).abs() < 0.15,
                "landed {} EV from the target: {info}", (key * gain / 0.13).log2());
    }

    /// A bright window in a correctly exposed room is the guard's business,
    /// not the key's: the room keeps its exposure.
    #[test]
    fn a_window_does_not_darken_the_room() {
        let room = |window: bool| {
            let mut img = Image::new(200, 100);
            for (i, px) in img.px_mut().iter_mut().enumerate() {
                let v = if window && i % 20 == 0 { 0.13 * 8.0 }
                        else { 0.13 * (1.0 + 0.2 * ((i % 7) as f32 / 7.0 - 0.5)) };
                *px = [v / 2f32.powf(METERED_EV); 3];
            }
            auto_exposure(&img, 0.13, 1.0, 3.2).1["key"].as_f64().unwrap()
        };
        let (plain, windowed) = (room(false), room(true));
        assert!((windowed / plain).log2().abs() < 0.03,
                "a 5 % window moved the key by {} EV", (windowed / plain).log2());
    }

    /// A properly exposed frame must come out exactly as it did before the
    /// rescue existed: the guard is for emergencies, not for everyone.
    #[test]
    fn a_well_exposed_frame_is_left_alone() {
        let ordinary = frame(0.13, 0.9, 120, 90);
        let (_, info) = auto_exposure(&ordinary, 0.13, 0.85, 3.2);
        assert_eq!(info["rescue_ev"].as_f64().unwrap(), 0.0,
                   "nothing to rescue here: {info}");
    }

    /// A dense band of shadow just above the black point is the case that used
    /// to lose half a frame to flat black.
    #[test]
    fn the_black_point_cannot_swallow_the_shadows() {
        // Two thirds of the frame piled into a narrow dark band.
        let mut dark = Plane::new(300, 100);
        for (i, v) in dark.d.iter_mut().enumerate() {
            *v = if i % 3 == 0 { 0.55 + (i % 7) as f32 * 0.01 }
                 else { 0.11 + (i % 5) as f32 * 0.002 };
        }
        let (black, white, info) = auto_levels(&dark, &dark, 0.6, 0.15);
        let survives = |l: f32| levels(l, black, white) > BLACK_LIGHTNESS;
        let lost = dark.d.iter().filter(|l| !survives(**l)).count() as f32 / dark.d.len() as f32;
        assert!(lost <= CRUSH_BUDGET + 0.02,
                "the black point pinned {:.1}% of the frame: {info}", lost * 100.0);
    }

    /// A black point removes a pedestal: a shadow a little above it keeps most
    /// of its light, and the midtones barely move.
    #[test]
    fn the_black_point_is_taken_off_light_not_lightness() {
        let (black, white) = (0.11, 0.99);
        let ev = |l: f32| (levels(l, black, white) / l).powi(3).log2();
        assert!(ev(0.2) > -0.3, "a shadow at L 0.2 lost {} EV", ev(0.2));
        assert!(ev(0.5).abs() < 0.05, "the midtones moved {} EV", ev(0.5));
        assert_eq!(levels(0.0, black, white), 0.0);
        assert!((levels(white, black, white) - 1.0).abs() < 1e-5);
    }

    /// A dim frame with no true white keeps its highlights where they were,
    /// near enough.
    #[test]
    fn a_frame_without_white_is_not_stretched_to_one() {
        let mut dim = Plane::new(100, 100);
        for (i, v) in dim.d.iter_mut().enumerate() {
            *v = 0.2 + 0.6 * (i % 100) as f32 / 100.0;
        }
        let (_, white, _) = auto_levels(&dim, &dim, 0.6, 0.15);
        assert!(white >= LOWEST_WHITE, "white point pulled to {white}");
    }

    /// Greys are not a reason to add colour.
    #[test]
    fn a_grey_frame_is_not_asked_to_be_colourful() {
        let (w, h) = (64, 64);
        let mut chroma = Plane::new(w, h);
        let mut light = Plane::new(w, h);
        for i in 0..w * h {
            light.d[i] = 0.5;
            // Mostly a faint white balance error, a little real colour.
            chroma.d[i] = if i % 10 == 0 { 0.09 } else { 0.012 };
        }
        let (boost, info) = auto_vibrance(&chroma, &light, 0.078, 1.55);
        assert!(boost < 1.05, "the greys asked for {boost}: {info}");
    }

    /// The floor is a property of the capture, so the same floor has to give
    /// the same answer whatever resolution asked for it.
    #[test]
    fn the_noise_level_follows_the_floor_not_the_scale() {
        let (a, _) = noise_from_floor(0.004, 200.0, 0.1);
        let (b, _) = noise_from_floor(0.004, 200.0, 0.1);
        assert_eq!(a, b);
        let (more, _) = noise_from_floor(0.008, 200.0, 0.1);
        assert!(more > a, "a noisier floor should read noisier");
    }
}
