//! The whole pipeline, on scenes whose right answer is known: what a fully
//! automatic development must do to a grey card, to white, to a ramp, to
//! colours, to a cast, to a window, to a night street, to a bracket.

mod common;
use common::*;
use lumiraw_core::decode::Meta;
use lumiraw_core::grade::Settings;

/// A grey card comes out grey and in the middle of the range.
#[test]
fn a_grey_card_comes_out_grey_and_middle() {
    let out = develop(scene(300, 200, |_, _, i| [grey() * grain(i, 0.2); 3]));
    let (_, chroma, _) = lch(mean(&out, 0.0, 0.0, 1.0, 1.0));
    assert!(chroma < 0.002, "the grey came out coloured: chroma {chroma}");
    let m = median_code(&out);
    assert!((100.0..=160.0).contains(&m), "middle grey came out at code {m}");
    let (black, white) = (crushed(&out), clipped(&out));
    assert!(black == 0.0 && white == 0.0,
            "a flat grey frame lost pixels to black ({black}) or white ({white})");
}

/// Blown white stays white: neutral, and at the top.
#[test]
fn blown_white_stays_white() {
    // A grey scene with a blown sky across the top third.
    let out = develop(scene(300, 200, |_, y, i| {
        if y < 0.33 { [1.0; 3] } else { [grey() * grain(i, 0.3); 3] }
    }));
    let sky = mean(&out, 0.1, 0.05, 0.9, 0.25);
    let (_, chroma, _) = lch(sky);
    assert!(chroma < 0.003, "the blown sky was tinted: {sky:?}");
    assert!(code(kit::luminance_px(&sky)) > 235.0, "the blown sky came out grey: {sky:?}");
}

/// A ramp of greys over thirteen stops keeps its order, stays neutral, and
/// keeps most of its stops apart.
#[test]
fn a_grey_ramp_keeps_its_order_and_its_greys() {
    let out = develop(scene(600, 60, |x, _, _| [1e-4 * 2f32.powf(13.0 * x); 3]));
    let row = middle_row(&out);
    for (x, px) in row.iter().enumerate() {
        let (_, chroma, _) = lch(*px);
        assert!(chroma < 0.002, "step {x} of the ramp was coloured: {px:?}");
    }
    keeps_order(row, 0.01).unwrap();
    // Stops that still differ by at least a code value: a display holds
    // about eight, and a good development keeps most of them apart.
    let codes: Vec<f32> = row.iter().map(|p| code(kit::luminance_px(p))).collect();
    let per_stop = codes.len() as f32 / 13.0;
    let distinct = (0..13).filter(|s| {
        let a = codes[(*s as f32 * per_stop) as usize];
        let b = codes[(((*s + 1) as f32 * per_stop) as usize).min(codes.len() - 1)];
        b - a >= 1.0
    }).count();
    assert!(distinct >= 8, "only {distinct} of 13 stops are told apart: {codes:?}");
}

/// Colours under neutral light keep their hue, and the greys beside them stay
/// grey, whatever the colours are.
#[test]
fn colours_keep_their_hue_and_greys_stay_grey() {
    // The patches in turn, then a grey.
    let out = develop(scene(300, 100, |x, _, i| {
        let p = PATCHES.get((x * 6.0) as usize).copied().unwrap_or([1.0; 3]);
        p.map(|v| v * 2.0 * grey() * grain(i, 0.2))
    }));
    let (_, grey_chroma, _) = lch(mean(&out, 5.1 / 6.0, 0.2, 5.9 / 6.0, 0.8));
    assert!(grey_chroma < 0.004, "the grey beside the colours came out tinted: {grey_chroma}");
    for (k, p) in PATCHES.iter().enumerate() {
        let x = (k as f32 + 0.5) / 6.0;
        let (_, _, want) = lch(*p);
        let (_, chroma, got) = lch(mean(&out, x - 0.05, 0.2, x + 0.05, 0.8));
        assert!(chroma > 0.03, "patch {k} lost its colour: chroma {chroma}");
        assert!(hue_gap(got, want).abs() < 12.0, "patch {k} moved from hue {want} to {got}");
    }
    let lost = clipped(&out);
    assert!(lost < 0.001, "{lost} of an ordinary scene clipped");
}

/// A bright, saturated orange keeps its gradation and its hue rather than
/// clipping flat in its brightest channel.
#[test]
fn a_bright_orange_is_not_clipped_flat() {
    // A grey frame with a band of orange brightening across it, two stops up
    // to the point where the sensor's red clips.
    let bright = ORANGE.map(|v| v / ORANGE[0]);
    let out = develop(scene(400, 200, |x, y, i| {
        if y > 0.4 && y < 0.6 { bright.map(|v| v * 0.25 * 2f32.powf(2.0 * x)) }
        else { [grey() * grain(i, 0.3); 3] }
    }));
    let band = middle_row(&out);
    let flat = share(band, |p| code(p[0]) >= 254.5);
    assert!(flat < 0.2, "{:.0} % of the orange band clipped flat in red", flat * 100.0);
    let (_, _, want) = lch(bright);
    for (x, p) in band.iter().enumerate().step_by(20) {
        let (_, _, got) = lch(*p);
        assert!(hue_gap(got, want).abs() < 15.0, "the orange turned to hue {got} at {x}");
    }
}

/// Greys under a moderate colour cast come out grey.
#[test]
fn a_cast_is_taken_out() {
    let cast = [1.18f32, 1.0, 0.82];
    let out = develop(scene(300, 200, |x, _, i| {
        // Greys of several levels, and a few coloured surfaces.
        let base = if (x * 10.0) as usize % 5 == 0 { [0.6, 0.4, 0.2] } else { [1.0; 3] };
        std::array::from_fn(|c| base[c] * cast[c] * grey() * 2.0 * grain(i, 0.5))
    }));
    let before = lch(cast.map(|v| v * grey())).1;
    let (_, after, _) = lch(mean(&out, 0.12, 0.1, 0.48, 0.9));
    assert!(after < before * 0.3, "the cast was barely taken out: chroma {before} to {after}");
}

/// A colourful scene under neutral light is left neutral: the sky and the
/// rock are not a cast.
#[test]
fn a_colourful_scene_is_not_mistaken_for_a_cast() {
    let surfaces: [[f32; 3]; 4] = [[0.35, 0.55, 1.0], [1.0, 0.55, 0.3], [0.4, 0.8, 0.25], [1.0; 3]];
    let out = develop(scene(400, 200, |x, _, i| {
        surfaces[(x * 4.0) as usize % 4].map(|v| v * grey() * 2.0 * grain(i, 0.4))
    }));
    let (_, chroma, _) = lch(mean(&out, 0.77, 0.1, 0.98, 0.9));
    assert!(chroma < 0.004, "the grey was pushed to chroma {chroma} by the colours around it");
}

/// A room with a bright window: the window keeps its detail, and the room is
/// not held dark for it.
#[test]
fn a_window_keeps_its_detail_and_the_room_its_light() {
    let out = develop(scene(200, 150, |x, y, i| {
        if x > 0.6 && x < 0.8 && y > 0.2 && y < 0.5 {
            // Outside: bright, with texture, five stops over the room.
            [grey() * 32.0 * grain(i, 0.4); 3]
        } else {
            [grey() * grain(i, 0.4); 3]
        }
    }));
    let window = mean(&out, 0.62, 0.22, 0.78, 0.48);
    assert!(code(window[0]) < 254.0, "the window was blown: {window:?}");
    let room = code(kit::luminance_px(&mean(&out, 0.05, 0.55, 0.5, 0.95)));
    assert!(room > 70.0, "the room was held dark at code {room}");
}

/// A night scene with a few lamps is not buried for the lamps' sake.
#[test]
fn lamps_do_not_bury_a_night_scene() {
    let out = develop(scene(200, 150, |_, _, i| {
        if i % 97 == 0 { [0.9, 0.7, 0.4] } else { [0.002 * grain(i, 0.6); 3] }
    }));
    let (m, black) = (median_code(&out), crushed(&out));
    assert!(m > 25.0, "the night scene came out at code {m}");
    assert!(black < 0.1, "{:.0} % crushed", black * 100.0);
}

/// A bracket comes out in order: the frame shot two stops under is darker
/// than the one shot two stops over, and neither is blown.
#[test]
fn a_bracket_keeps_its_order() {
    let frame = |stops: f32| {
        let img = scene(300, 200, |x, y, i| {
            let v = if y < 0.3 { 0.2 + 0.3 * x } else { grey() * grain(i, 0.5) };
            [v * 2f32.powf(stops); 3]
        });
        develop_meta(img, Meta { exposure_comp: stops, ..Meta::default() })
    };
    let (under, over) = (frame(-2.0), frame(2.0));
    let (dark, light) = (median_code(&under), median_code(&over));
    assert!(dark + 10.0 < light, "the bracket came out {dark} under and {light} over");
    for (name, img) in [("under", &under), ("over", &over)] {
        let lost = clipped(img);
        assert!(lost < 0.01, "the {name} frame clipped {:.1} %", lost * 100.0);
    }
}

/// What is shown is what is saved: a full-size render decides as the preview
/// did. Only the long edge has to be past the preview's size.
#[test]
fn the_export_matches_the_preview() {
    let img = scene(1800, 300, |x, y, i| {
        let warm = [1.1, 1.0, 0.85];
        let v = grey() * (0.3 + 3.0 * x * y) * grain(i, 0.3);
        warm.map(|c| c * v)
    });
    let mut dev = development(img, Meta::default());
    let preview = dev.render(&Settings::default(), Some(1300), true, None).0;
    let export = kit::thumbnail(&dev.render(&Settings::default(), None, true, None).0, 1300);
    let (p, e) = (mean(&preview, 0.0, 0.0, 1.0, 1.0), mean(&export, 0.0, 0.0, 1.0, 1.0));
    for c in 0..3 {
        assert!((p[c] / e[c] - 1.0).abs() < 0.01, "channel {c}: preview {p:?}, export {e:?}");
    }
}

/// The same settings give the same picture.
#[test]
fn developing_twice_gives_the_same_picture() {
    let g = grey();
    let img = scene(150, 100, |x, y, i| [g * (1.0 + x) * grain(i, 0.3), g * (1.0 + y), g]);
    assert_eq!(develop(img.clone()).d, develop(img).d);
}

/// A night is dark on purpose: the same street, its lights well over a tenth
/// of the frame, comes out darker when the file says it was shot at night
/// than when it says nothing, and is not buried either way.
#[test]
fn a_night_stays_a_night() {
    let street = || scene(200, 150, |x, y, i| {
        if (x * 20.0) as usize % 6 == 0 && y > 0.3 { [0.8, 0.6, 0.35] }
        else { [0.004 * grain(i, 0.6); 3] }
    });
    // ISO 6400, f/2.8, a tenth of a second: about EV 0.
    let night = Meta { iso: 6400.0, aperture: 2.8, shutter_s: 0.1, focal_mm: 35.0,
                       ..Meta::default() };
    let at_night = develop_meta(street(), night);
    let unsaid = develop(street());
    let (dark, plain) = (median_code(&at_night), median_code(&unsaid));
    assert!(dark + 15.0 < plain, "night {dark} against unsaid {plain}");
    let black = crushed(&at_night);
    assert!(black < 0.15, "{:.0} % of the night crushed", black * 100.0);
}

/// A camera that balanced the shot itself is trusted with it: the picture
/// comes out as with the automatic balance switched off.
#[test]
fn the_cameras_own_balance_is_kept() {
    let warm = || scene(200, 150, |_, _, i| [1.1, 1.0, 0.85].map(|c| c * grey() * grain(i, 0.4)));
    let camera = develop_meta(warm(), Meta { camera_auto_wb: Some(true), ..Meta::default() });
    let mut off = Settings::default();
    off.enabled.insert("white_balance".into(), false);
    let as_shot = development(warm(), Meta::default()).render(&off, Some(600), true, None).0;
    assert_eq!(camera.d, as_shot.d, "the camera's own balance was changed");
}
