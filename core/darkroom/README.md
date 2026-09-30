# darkroom

The API over the pipeline, and the wasm module the web app loads.

```rust
let lenses = darkroom::Lenses::parse(&lensfun_json).unwrap();
let mut photo = darkroom::open("IMG_0001.CR3", &bytes, Some(&lenses)).unwrap();

let edit = darkroom::Edit::default();
let preview = photo.develop(&edit, darkroom::Size::LongEdge(1600), None);

let out = photo.export(&edit, &darkroom::Export::new("jpeg"), None).unwrap();
```

WebP isn't encoded here: `export` hands back the pixels and the EXIF and the
browser does the encoding.

`src/wasm.rs` is the same API as plain `extern "C"` functions (`ar_open`,
`ar_render`, `ar_export`, ...) passing JSON and pixel buffers through shared
memory. The web app's worker (`web/src/lib/wasm/worker.js`) is the other side
of it.

## Building

```
cargo build --release -p darkroom --target wasm32-unknown-unknown
```

or just `tools/build.sh` from the repo root, which also copies the module into
`web/src/lib/wasm/`. The target flags are in `.cargo/config.toml`.
