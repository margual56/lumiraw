//! A real bracket, merged and developed fully automatically: the merge must
//! hold every highlight the darkest frame held, and every shadow the brightest
//! frame held. The raws stay out of the repository; point the test at them:
//!
//! ```text
//! LUMIRAW_BRACKET=~/Pictures/bracket cargo test --release --test bracket_raws -- --ignored --nocapture
//! ```
//!
//! Each frame and the merge are developed fully automatically and lined up
//! through the map the merge used. Pixel by pixel, nothing the darkest frame
//! held under its ceiling may come out clipped, and nothing the brightest frame
//! saw clear of its noise may come out black. In blocks, for reading, how far
//! the merge strays above the darkest frame's development in the highlights
//! and below the brightest frame's in the shadows. With LUMIRAW_DUMP set to a
//! folder it writes each frame, the merge, the merge with the blocks that
//! strayed marked, and the merge with the pixels that broke the rule marked.

mod common;
use common::*;
use kit::Image;
use lumiraw_core::develop::Development;
use lumiraw_core::grade::Settings;
use lumiraw_core::merge::{self, Frame, Options};

const EDGE: usize = 1200;

/// Fully automatic, but with the lens's geometry left as shot, so that a pixel
/// of the development and of each frame can be lined up through the merge's
/// own map. Vignetting stays: it moves light, not pixels, and moves the
/// ceiling a highlight is held under with it.
fn settings() -> Settings {
    let mut s = Settings::default();
    for stage in ["lens_distortion", "lens_tca"] {
        s.enabled.insert(stage.into(), false);
    }
    s
}

fn dump(img: &Image, name: &str) {
    if let Ok(dir) = std::env::var("LUMIRAW_DUMP") {
        let bytes = lumiraw_core::output::save(img, "png8", 90, &[], None).expect("png");
        std::fs::write(format!("{dir}/{name}.png"), bytes).expect("written");
    }
}

fn report(name: &str, img: &Image) {
    println!("{name:<24} crushed {:5.2} %  near white {:5.2} %  clipped {:5.2} %  median {:3.0}",
             crushed(img) * 100.0, near_white(img) * 100.0, clipped(img) * 100.0, median_code(img));
}

#[test]
#[ignore = "needs LUMIRAW_BRACKET pointing at a folder holding one bracket"]
fn a_merged_bracket_neither_blows_nor_crushes() {
    let dir = std::env::var("LUMIRAW_BRACKET").expect("set LUMIRAW_BRACKET");
    let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap()
        .filter_map(|e| e.ok().map(|e| e.path())).collect();
    files.sort();
    let mut frames = Vec::new();
    let mut developed = Vec::new();
    for path in &files {
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        let bytes = std::fs::read(path).unwrap();
        let Ok(frame) = Frame::open(&name, &bytes) else { continue };
        let mut dev = Development::open(&name, &bytes, None).unwrap();
        let (out, _) = dev.render(&settings(), Some(EDGE), true, None);
        report(&name, &out);
        dump(&out, &name);
        developed.push(out);
        frames.push(frame);
    }
    let started = std::time::Instant::now();
    let (image, notes) = merge::merge(&frames, Options::default()).unwrap();
    println!("merged in {:.1} s: stops {:?}  shifts {:?}  uncovered {:.4}  ghosted {:.4}",
             started.elapsed().as_secs_f32(), notes.stops, notes.shifts, notes.uncovered,
             notes.ghosted);
    // How far each frame was moved at its corners: nothing, on a tripod.
    for (j, w) in notes.warps.iter().enumerate() {
        let corner = |x: f32, y: f32| {
            let (u, v) = w.map(x, y);
            ((u - x).round(), (v - y).round())
        };
        let (fw, fh) = (frames[j].linear.w as f32, frames[j].linear.h as f32);
        println!("frame {j}: corners moved {:?} {:?} {:?} {:?}", corner(0.0, 0.0), corner(fw, 0.0),
                 corner(0.0, fh), corner(fw, fh));
    }
    let meta = merge::merged_meta(&frames, &notes);
    let mut dev = Development::from_frame("merged", image, meta, Vec::new(), None);
    let (out, _) = dev.render(&settings(), Some(EDGE), true, None);
    report("merged", &out);
    dump(&out, "merged");
    // And as the app develops it, lens and all, to look at.
    if std::env::var("LUMIRAW_DUMP").is_ok() {
        dump(&dev.render(&Settings::default(), Some(EDGE), true, None).0, "merged_as_app");
    }

    // Each frame's development, lined up with the merge's, in blocks so that
    // what alignment leaves over does not count.
    let full_w = frames[0].linear.w as f32;
    let by_stops = |a: &usize, b: &usize| notes.stops[*a].partial_cmp(&notes.stops[*b]).unwrap();
    // Of frames tied for darkest, the reference, which the merge is drawn in.
    let darkest = (0..frames.len()).filter(|j| notes.stops[*j] <= notes.stops[notes.reference])
        .min_by(|a, b| by_stops(a, b).then((*a != notes.reference).cmp(&(*b != notes.reference))))
        .unwrap();
    let brightest = (0..frames.len()).max_by(by_stops).unwrap();
    let blocks = |img: &Image, j: Option<usize>| -> Vec<Option<f32>> {
        let y = kit::luminance(img);
        let k = img.w as f32 / full_w;
        let warp = j.map(|j| notes.warps[j]).unwrap_or(merge::Warp::IDENTITY);
        let mut out = Vec::new();
        for by in 0..img.h / BLOCK {
            for bx in 0..img.w / BLOCK {
                // Where the block's corner falls in this frame, through the
                // map the merge used.
                let (sx, sy) = warp.map((bx * BLOCK) as f32 / k, (by * BLOCK) as f32 / k);
                let (x0, y0) = ((sx * k).round() as i32, (sy * k).round() as i32);
                if x0 < 0 || y0 < 0 || x0 as usize + BLOCK > img.w || y0 as usize + BLOCK > img.h {
                    out.push(None);
                    continue;
                }
                let mut sum = 0.0;
                for yy in 0..BLOCK {
                    for xx in 0..BLOCK {
                        sum += y.d[(y0 as usize + yy) * img.w + x0 as usize + xx];
                    }
                }
                out.push(Some(sum / (BLOCK * BLOCK) as f32));
            }
        }
        out
    };
    let m = blocks(&out, None);
    let d = blocks(&developed[darkest], Some(darkest));
    let b = blocks(&developed[brightest], Some(brightest));
    let (mut hi, mut hi_over, mut lo, mut lo_under) = (0, 0, 0, 0);
    let mut marked = out.clone();
    let per_row = out.w / BLOCK;
    // The blocks that strayed, tinted on the merge: red blown, blue buried.
    let mut mark = |i: usize, colour: [f32; 3]| {
        let (bx, by) = (i % per_row * BLOCK, i / per_row * BLOCK);
        for y in by..by + BLOCK {
            for x in bx..bx + BLOCK {
                let k = (y * marked.w + x) * 3;
                for c in 0..3 {
                    marked.d[k + c] = 0.5 * marked.d[k + c] + 0.5 * colour[c];
                }
            }
        }
    };
    let (mut hi_gap, mut lo_gap) = (Vec::new(), Vec::new());
    for i in 0..m.len() {
        let (Some(m), Some(d), Some(b)) = (m[i], d[i], b[i]) else { continue };
        if d >= HIGHLIGHT {
            hi += 1;
            hi_gap.push((m / d).log2());
            if m > d * TOLERANCE.exp2() {
                hi_over += 1;
                mark(i, [1.0, 0.0, 0.0]);
            }
        }
        if b <= SHADOW && code(b) >= 8.0 {
            lo += 1;
            lo_gap.push((m / b.max(1e-6)).log2());
            if m < b * (-TOLERANCE).exp2() {
                lo_under += 1;
                mark(i, [0.0, 0.4, 1.0]);
            }
        }
    }
    dump(&marked, "marked");

    // The rule itself, pixel by pixel: nothing the darkest frame held below
    // its ceiling may clip, and nothing the brightest frame saw clear of the
    // noise may go to black.
    let k = out.w as f32 / full_w;
    let (mut held, mut blown, mut seen, mut buried) = (0usize, 0usize, 0usize, 0usize);
    let mut breaches = out.clone();
    for y in 0..out.h {
        for x in 0..out.w {
            let px = out.px()[y * out.w + x];
            let (fx, fy) = (x as f32 / k, y as f32 / k);
            // What a frame recorded over the patch this pixel of the
            // development covers: its mean level, and its fullest pixel.
            let read = |j: usize| {
                let f = &frames[j];
                let (cx, cy) = notes.warps[j].map(fx + 0.5 / k, fy + 0.5 / k);
                let r = 0.5 / k;
                let (x0, y0) = ((cx - r).round() as i64, (cy - r).round() as i64);
                let (x1, y1) = ((cx + r).round() as i64, (cy + r).round() as i64);
                if x0 < 0 || y0 < 0 || x1 > f.linear.w as i64 || y1 > f.linear.h as i64 {
                    return None;
                }
                let (mut sum, mut fullest, mut n) = (0.0, 0.0f32, 0.0);
                for sy in y0 as usize..y1 as usize {
                    for sx in x0 as usize..x1 as usize {
                        let i = sy * f.linear.w + sx;
                        let v = f.linear.px()[i];
                        sum += v[0].max(v[1]).max(v[2]);
                        fullest = fullest.max(f.fullness(i, v));
                        n += 1.0;
                    }
                }
                (n > 0.0).then(|| (sum / n, fullest))
            };
            if let Some((_, full)) = read(darkest) {
                if full < CLIP_FULL {
                    held += 1;
                    if px.iter().any(|v| code(*v) >= 254.5) {
                        blown += 1;
                        breaches.d[(y * out.w + x) * 3..][..3].copy_from_slice(&[1.0, 0.0, 0.0]);
                    }
                }
            }
            if let Some((level, _)) = read(brightest) {
                if level > SEEN_LEVEL {
                    seen += 1;
                    if code(kit::luminance_px(&px)) < 8.0 {
                        buried += 1;
                        breaches.d[(y * out.w + x) * 3..][..3].copy_from_slice(&[0.0, 0.4, 1.0]);
                    }
                }
            }
        }
    }
    dump(&breaches, "breaches");
    println!("pixel rule: {blown} of {held} held highlights clipped ({:.4} %), {buried} of {seen} seen shadows crushed ({:.4} %)",
             blown as f32 / held.max(1) as f32 * 100.0, buried as f32 / seen.max(1) as f32 * 100.0);
    let median = |v: &mut Vec<f32>| {
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        v.get(v.len() / 2).copied().unwrap_or(0.0)
    };
    let hi_share = hi_over as f32 / hi.max(1) as f32;
    let lo_share = lo_under as f32 / lo.max(1) as f32;
    println!("highlights: {hi} blocks, {:.1} % brighter than the darkest frame, median {:+.2} EV",
             hi_share * 100.0, median(&mut hi_gap));
    println!("shadows:    {lo} blocks, {:.1} % darker than the brightest frame, median {:+.2} EV",
             lo_share * 100.0, median(&mut lo_gap));
    // The blocks are for reading: each frame's own development chose its own
    // exposure, so a merge darker than the brightest frame's is a choice, not
    // a loss. The rule is the pixels.
    assert_eq!(blown, 0, "the merge blew highlights the darkest frame held");
    assert_eq!(buried, 0, "the merge buried shadows the brightest frame saw");
}

/// Blocks of the developed frames compared, in pixels.
const BLOCK: usize = 8;
/// Display-linear luminance above which a block of the darkest frame is a
/// highlight, and below which one of the brightest is a shadow.
const HIGHLIGHT: f32 = 0.45;
const SHADOW: f32 = 0.06;
/// A pixel of the darkest frame below this fullness was held, not clipped;
/// one of the brightest above this level was seen, not lost in the noise.
const CLIP_FULL: f32 = 0.97;
const SEEN_LEVEL: f32 = 0.02;
/// How far, in stops, the merge may stray past either frame.
const TOLERANCE: f32 = 0.25;
