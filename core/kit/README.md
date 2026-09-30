# kit

The generic bits the pipeline is built on. Nothing in here knows about raw
files.

- `image.rs` - `Image` (interleaved RGB f32) and `Plane` (one channel)
- `color.rs` - sRGB transfer, luminance, Oklab, gamut mapping, Bradford
- `matrix.rs` - 3x3 matrices
- `filter.rs` - resizing, box/gaussian blur, guided filter, bilinear sampling
- `math.rs` - fast log2/exp2/atan2/cbrt, percentiles, mean, median
- `report.rs` - `Report`, how a stage says what it decided

Only depends on `serde_json` (for `Report`).
