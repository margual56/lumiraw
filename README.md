# lumiraw

Raw photo developer that runs entirely in the browser. The pipeline is
written in Rust and compiled to WebAssembly, so photos never leave your
machine.

https://lumiraw.app

## Building

```
tools/build.sh
python -m http.server -d web/build 8790
```

Needs Node. Rebuilding the wasm module also needs Rust with the
`wasm32-unknown-unknown` target; otherwise the committed one is used.

## Releasing

Nothing to do by hand. Once CI passes on main, the Release workflow bumps the
version everywhere, rebuilds the wasm modules, commits that to main, tags it
and publishes a GitHub release. The size of the bump comes from the commit or
pull request titles since the last release: `feat:` is minor, `feat!:` (or a
`BREAKING CHANGE:` line) is major, anything else is a patch. Pushes that only
touch docs, CI config or `tools/` aren't released. "Run workflow" on the
Release workflow forces one, with the size chosen there.

## Layout

- `core/` - the pipeline
- `core/kit/` - image, colour and maths helpers
- `core/darkroom/` - API over the pipeline, built to wasm
- `core/critic/` - NIMA photo scoring for the culling page, its own wasm module
- `core/vendor/rawler/` - vendored raw decoder (see VENDORED.md)
- `web/` - the SvelteKit app
- `tools/` - build and data scripts

Lens corrections use the lensfun database. The culling page scores photos with
idealo's NIMA models (Apache 2.0, see `web/src/lib/wasm/critic/README.md`).
