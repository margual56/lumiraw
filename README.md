# lumiraw

Raw photo developer that runs entirely in the browser. The pipeline is
written in Rust and compiled to WebAssembly, so photos never leave your
machine.

## Building

```
tools/build.sh
python -m http.server -d web/build 8790
```

Needs Node. Rebuilding the wasm module also needs Rust with the
`wasm32-unknown-unknown` target; otherwise the committed one is used.

## Layout

- `core/` - the pipeline
- `core/kit/` - image, colour and maths helpers
- `core/darkroom/` - API over the pipeline, built to wasm
- `core/vendor/rawler/` - vendored raw decoder (see VENDORED.md)
- `web/` - the SvelteKit app
- `tools/` - build and data scripts

Lens corrections use the lensfun database.
