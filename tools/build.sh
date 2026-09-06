# !/usr/bin/env bash Build the whole application.
set -euo pipefail
cd "$(dirname "$0")/.."

if [[ "${1:-}" == "--db" ]]; then
  python tools/bake_lensfun.py /usr/share/lensfun/version_1 web/src/lib/wasm/lensfun.json
  python tools/split_lensfun.py
fi

if command -v cargo >/dev/null && rustup target list --installed 2>/dev/null | grep -q wasm32-unknown-unknown; then
  # Target flags (simd128, the getrandom backend) live in .cargo/config.toml.
  cargo build --release --manifest-path core/Cargo.toml -p darkroom --target wasm32-unknown-unknown
  cp core/target/wasm32-unknown-unknown/release/darkroom.wasm web/src/lib/wasm/darkroom.wasm
  echo "rebuilt the wasm module from core/darkroom"
else
  test -f web/src/lib/wasm/darkroom.wasm || {
    echo "no Rust toolchain and no committed web/src/lib/wasm/darkroom.wasm" >&2
    exit 1
  }
  echo "no Rust toolchain here, using the committed wasm module"
fi

(cd web && npm ci --no-audit --no-fund && npm run build)

printf '\n%-28s %s\n' "wasm module" "$(du -h web/src/lib/wasm/darkroom.wasm | cut -f1)"
printf '%-28s %s\n' "lens database" "$(du -h web/src/lib/wasm/lensfun.json | cut -f1)"
printf '%-28s %s\n' "site total" "$(du -sh web/build | cut -f1)"
echo
echo "serve it with:  python -m http.server -d web/build 8790"
