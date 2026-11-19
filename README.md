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

### Desktop app

`desktop/` wraps the built site in a Tauri window. After building the site:

```
cd desktop && npm ci && npx tauri build --bundles appimage   # or nsis on Windows
```

On Linux it needs WebKitGTK 4.1 and GTK 3 development packages.

## Deploying

The site is static, so it goes on Cloudflare Pages as it is, with no
Functions. Connect the repository and set:

- Build command: `npm run build`
- Build output directory: `web/build`
- Root directory: empty (the repository's root)
- Production branch: `main`

Pages picks the Node version from `.node-version`. It doesn't need Rust, as
the wasm modules are committed. `web/static/_headers` (the CSP and caching)
and `web/static/_redirects` end up in the build and Pages applies them; a
site that embeds the app on another domain needs adding to `frame-ancestors`
there. To deploy by hand instead:

```
npm run build
npx wrangler pages deploy web/build --project-name lumiraw
```

## Releasing

Nothing to do by hand. Once CI passes on main, the Release workflow bumps the
version everywhere, rebuilds the wasm modules, commits that to main, tags it
and publishes a GitHub release. The size of the bump comes from the commit or
pull request titles since the last release: `feat:` is minor, `feat!:` (or a
`BREAKING CHANGE:` line) is major, anything else is a patch. Pushes that only
touch docs, CI config or `tools/` aren't released. "Run workflow" on the
Release workflow forces one, with the size chosen there. Each release also
gets the desktop app attached: an AppImage built on Linux and an installer
`.exe` built on Windows.

## Layout

- `core/` - the pipeline
- `core/kit/` - image, colour and maths helpers
- `core/darkroom/` - API over the pipeline, built to wasm
- `core/critic/` - NIMA photo scoring for the culling page, its own wasm module
- `core/vendor/rawler/` - vendored raw decoder (see VENDORED.md)
- `web/` - the SvelteKit app
- `desktop/` - the same app as a desktop program (Tauri)
- `tools/` - build and data scripts

Lens corrections use the lensfun database. The culling page scores photos with
idealo's NIMA models; the developer uses MediaPipe's face detector and DeepLab
v3, and Megvii's PMRID for deep noise reduction (all Apache 2.0, see
`web/src/lib/wasm/critic/README.md`).
