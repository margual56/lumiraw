# lumiraw-core

The raw processing pipeline. Plain Rust, nothing browser-specific in here;
`darkroom/` wraps it for the web app and `kit/` has the generic helpers it
uses.

## src/

- `raw.rs`, `decode.rs` - read the file through the vendored rawler, demosaic,
  highlight reconstruction, camera matrix
- `profile.rs`, `exif.rs` - camera metadata, and writing it back out
- `analyze.rs` - measurements the automatic corrections work from
- `grade.rs` - the main chain: white balance, exposure, tone map, colour
- `develop.rs` - one photo, from open to export
- `curve.rs`, `looks.rs`, `mixer.rs`, `lut.rs`, `effects.rs`, `local.rs` -
  manual grading
- `geometry.rs`, `straighten.rs` - crop, rotation, levelling
- `lensdb.rs` - lensfun corrections
- `merge.rs` - exposure brackets
- `output.rs` - PNG, JPEG, TIFF

## Tests

```
cargo test --workspace
```

Besides each module's own tests, two suites in `tests/` develop synthetic
scenes whose right answer is known:

- `pipeline.rs` runs the whole automatic development on a grey card, blown
  white, a grey ramp, colours, a cast, a window, a night street, a bracket,
  and checks what a correct development must do to each.
- `stages.rs` runs each stage on its own against what it must never do: tint
  a grey, reverse two greys, turn a colour off its hue, leave the gamut.

`real_raws.rs` holds real photographs to bounds on crushed, near-white and
clipped pixels and on the tint left on greys, per frame and on average. The
raws stay out of the repository, so it is skipped unless pointed at a folder:

```
LUMIRAW_RAWS=~/Pictures/raws cargo test --release --test real_raws -- --ignored --nocapture
```

`probe` develops a raw natively and prints what each stage decided, which is
the quickest way to see why a picture came out the way it did:

```
cargo run --release --bin probe -- photo.ARW out.png
```
