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

`probe` develops a raw natively and prints what each stage decided, which is
the quickest way to see why a picture came out the way it did:

```
cargo run --release --bin probe -- photo.ARW out.png
```
