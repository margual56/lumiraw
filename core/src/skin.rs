//! What skin looks like in a photograph, read off its faces, so vibrance and
//! the wheels can hold back on exactly that colour rather than on a guess.

use crate::grade::Skin;
use critic::graph::Graph;
use kit::Image;

/// The long edge faces are looked for at.
pub const SEARCH_EDGE: usize = 640;

/// The skin of the faces in a display-linear picture, or `None` when there are
/// none. Each face is read from the middle of its box, below the eyes and above
/// the mouth's corners, where there is most skin and least hair, eye or lip.
pub fn measure(img: &Image, detector: &Graph) -> Option<(Skin, usize)> {
    let rgba: Vec<u8> = img.px().iter()
        .flat_map(|px| {
            let e = px.map(|v| (kit::srgb_encode_scalar(v.clamp(0.0, 1.0)) * 255.0).round() as u8);
            [e[0], e[1], e[2], 255]
        })
        .collect();
    let faces = critic::faces::find(detector, None, &rgba, img.w, img.h);
    let (mut a, mut b) = (Vec::new(), Vec::new());
    for f in &faces {
        let (w, h) = (f.x1 - f.x0, f.y1 - f.y0);
        let (x0, x1) = ((f.x0 + 0.2 * w).max(0.0) as usize, ((f.x1 - 0.2 * w) as usize).min(img.w));
        let (y0, y1) = ((f.y0 + 0.45 * h).max(0.0) as usize, ((f.y0 + 0.75 * h) as usize).min(img.h));
        for y in y0..y1 {
            for x in x0..x1 {
                let px = img.px()[y * img.w + x];
                let (l, pa, pb) = kit::rgb_to_oklab_px(px[0], px[1], px[2]);
                // Neither a shadow too deep to have a colour nor a highlight
                // the light's colour has taken over.
                if l > 0.3 && l < 0.92 && pa.hypot(pb) > 0.015 {
                    a.push(pa);
                    b.push(pb);
                }
            }
        }
    }
    if a.len() < 32 {
        return None;
    }
    let (ma, mb) = (kit::median(&a), kit::median(&b));
    Some((Skin { hue: mb.atan2(ma), chroma: ma.hypot(mb), measured: true }, faces.len()))
}

