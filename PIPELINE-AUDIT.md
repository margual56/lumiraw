# Pipeline audit: automatic development accuracy

Found by six per-stage audits (decode, lens/geometry, white balance, exposure,
tone mapping, colour/detail), measured on the demo raws and synthetic scenes.
Symptoms reported: highlights blown, shadows crushed, a slight tint after auto
white balance. Ranked by how much each contributes. Remove this file before
merging.

`[x]` fixed, `[~]` partly, `[-]` left on purpose, `[ ]` open.

Progress over the 20 demo raws (mean per frame; tint is the Oklab chroma ×1000
left on what the camera's own balance made neutral):

| Stage                      | Y<8 % | Y≥250 % | any channel clipped % | median | tint |
|----------------------------|-------|---------|-----------------------|--------|------|
| Before                     | 11.04 | 0.76    | 4.00                  | 82     | 16.4 |
| Display stages (T, C, O2)  | 2.09  | 0.79    | 3.63                  | 94     | 14.4 |
| + exposure, filmic, lift   | 0.71  | 0.02    | 0.47                  | 99     | 14.4 |
| + white balance            | 0.87  | 0.00    | 0.13                  | 95     | 5.2  |
| + decode (D1, D3)          | 0.82  | 0.00    | 0.08                  | 95     | 5.2  |
| + geometry, lens, dehaze   | 0.82  | 0.00    | 0.08                  | 95     | 5.2  |
| + key, blown white (E7, D4)| 0.79  | 0.01    | 0.08                  | 97     | 5.3  |

The last row is unchanged because the bench renders without the lens database
and at default settings (no rotation, no dehaze); those fixes are measured in
their own commits.

## Shadows crushed, highlights blown

- [x] **T1** Auto black point is subtracted in Oklab L, which costs −2…−3 EV at
      p1 and ~−0.3 EV at the median; its guard lets 10 % of the frame go to
      code ~0. Apply levels in linear light, and guard against crushing.
      (`grade.rs` levels, `analyze.rs` auto levels)
- [x] **T2** Auto S-curve re-expands both ends of a frame the tone map
      compressed (`squeezed` term), end slopes 0.37–0.8, pivot up to 0.8.
      Keep end slopes near 1, drop `squeezed`, cap the pivot.
      (`analyze.rs` auto contrast, `grade.rs::s_curve`)
- [x] **E1** Any shadow rescue waives the highlight guard entirely (all 7
      rescued demo frames; the EC −2/+2 bracket comes out ~29 % blown and
      inverted). Never waive the guard; leave the shadows to the tone map.
      (`analyze.rs::auto_exposure`)
- [x] **E2** The guard runs after the crush check and is never re-checked:
      small lamps bury night frames (median code 6). (`analyze.rs`)
- [x] **E3** Exposure soft limit is centred on 0 EV, but a normal Sony raw needs
      ~+2.9 EV: typical frames land 0.3–1 EV dark. (`analyze.rs`)
- [x] **E4** Rescue ignores the exposure-compensation intent (bracket order
      inverted). (`analyze.rs`, `profile.rs::tune`)
- [x] **E5** Highlight guard is luminance only; saturated skies and reds clip
      unseen. Guard on a max-channel blend. (`analyze.rs`)
- [x] **F1** `filmic` scales by luminance, so saturated colours overshoot 1
      (DSC05480: 27 % of pixels with a channel clipped). Use a max-channel norm.
      (`grade.rs::filmic`)
- [x] **D1** Highlight reconstruction also rewrites the channel the sensor did
      record, bleaching clipped skies. (`decode.rs` reconstruction)
- [x] **D2** Decoded frame does not say where the sensor clipped; auto exposure
      counts Y ≥ 0.99 and misses partly clipped pixels. (`decode.rs`, `analyze.rs`)
      *No longer matters: the miscount's only effect was switching the
      highlight guard off, which E1/E6 removed; partly clipped pixels are now
      simply among the brightest the guard keeps below the shoulder.*
- [x] **L1** lensfun vignetting for the E PZ 16-50 over-corrects corners by
      ~1.5 EV (×32 at the extreme corner). Cross-check with the camera's own
      table, cap the gain, smooth the clamp. (`lensdb.rs`, `develop.rs`)
- [-] **T3** ACES toe already carries a contrast look; stacked with T1/T2.
      Revisit once T1/T2 are fixed. (`grade.rs::filmic`)
      *Left as the look: it only crushed when stacked on the old black point
      and S-curve; with T1 and T2 fixed, under 1 % of pixels fall below code 8
      across the demo raws.*

## Tint

- [x] **W1** Auto WB estimator (shades-of-grey p = 6, no colour gate) is pulled
      by saturated bright surfaces; 14/20 demo frames moved warm. Estimate
      from near-neutral pixels, lower p, fall back to the camera.
      (`analyze.rs` WB estimate)
- [x] **W2** Per-channel soft limit turns temperature into green/magenta tint.
      Limit in temperature/tint coordinates. (`analyze.rs`)
- [x] **W3** TINT_TRUST 0.2 leaves 80 % of a real green/magenta cast. Raise once
      W1 is fixed. (`analyze.rs`)
- [x] **W4** Soft limit under-corrects inside its range (0.5 → 0.44 EV).
      (`analyze.rs`)
- [x] **C1** Vibrance applies its full boost to near-neutrals (and near-white),
      amplifying any residual tint 1.3–1.4×. Taper below chroma ~0.03 and in
      highlights; measure p80 over coloured pixels. (`grade.rs`, `analyze.rs`)
- [x] **C2** Chroma compensation exponent 0.35 raises saturation where levels
      and contrast darken (~1.5× in shadows). (`grade.rs`)
- [x] **D3** Negative sensor noise clamped to 0 before WB and demosaic: magenta
      deep shadows. (`decode.rs`)
- [x] **G1** Preview and export decide from different thumbnails: WB 2–3 % and
      exposure ~0.1 EV apart. Decide from a canonical proxy.
      (`develop.rs`, `grade.rs`)
- [x] **D4** Blown highlights neutralised for the camera's WB, then tinted by
      auto WB. (`decode.rs`, `grade.rs`)

## Other

- [x] **G2** Rotating with auto-fit off measures the black wedges (+5.5 EV, 51 %
      clipped); straighten locks onto them. Always measure inside the cover
      crop. (`geometry.rs`, `develop.rs`)
- [x] **G3** Half-pixel offset in `geometry::apply`: every crop is blurred by a
      2×2 box; uncropped canvas gets a black first row/column. (`geometry.rs`)
- [x] **G4** Full-size decisions cache key ignores spots. (`develop.rs`)
- [x] **H1** Dehaze darkens and warms haze-free shadows. (`dehaze.rs`)
- [x] **O1** Dither is half the standard amplitude; previews are not dithered.
      (`output.rs`) *Exports dither at full amplitude. Previews stay
      undithered on purpose: dithering every preview would cost time and make
      them shimmer between renders, to hide errors of one code value.*
- [x] **O2** Auto black and white points move by different fractions (0.6 vs
      0.4). (`analyze.rs`)
- [-] **D5** White level jumps between 15360 and ~16300 frame to frame.
      (`raw.rs`)
      *Left as it is: the declared and observed white levels differ by at most
      0.09 EV, which the automatic exposure renormalises; it only shows with
      automatic exposure off. A real fix needs per-model white levels, which
      the files do not give and which would have to be measured per body.*
- [x] **E6** Clipped fraction measured on a thumbnail averages away small lamps;
      guard switches on/off at a hard 0.5 %. (`analyze.rs`)
      *The guard measures over unclipped pixels and ignores small sources,
      so a thumbnail averaging tiny lamps away loses nothing it would use;
      only the reported clipped fraction is still a thumbnail's.*
- [x] **E7** Key log-mean includes highlights (−0.15 EV on a correct scene).
      (`analyze.rs`)
- [x] **E8** UI's "applied EV" includes the user's bias on one path and not the
      other. (`grade.rs`)
- [~] **L2** Embedded Sony vignetting table may under-correct (~0.3–0.6 EV at
      corners); needs a flat field to settle. (`lensdb.rs`) *On the evenly lit
      wall (DSC05663, 16 mm f/3.5) the camera's table leaves the bottom corners
      within 0.02 EV of their edges. Settling it needs a real flat-field shot
      (an evenly lit white wall or sky, defocused, at a few focal lengths).*
- [x] **S1** Found while fixing G3: the straightening detector stops offering
      a perspective correction once about a third of the near-vertical edges
      disagree; its test only passed that case because of G3's blur.
      (`straighten.rs`) *The fit is a repeated median now, which holds until
      half the edges disagree; the test is back to a third.*
- [-] **D6** WB adaptation in camera space rather than XYZ (saturated colours
      under tungsten). Accuracy improvement, not a tint. (`decode.rs`)
      *Left for now: it changes saturated colours under tungsten, not
      neutrals, and judging it needs a colour reference (a chart shot);
      LibRaw uses the same method, so it is no reference.*
