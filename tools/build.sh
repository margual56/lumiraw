# !/usr/bin/env bash Build the whole application.
set -euo pipefail
cd "$(dirname "$0")/.."

if [[ "${1:-}" == "--db" ]]; then
  python tools/bake_lensfun.py /usr/share/lensfun/version_1 web/src/lib/wasm/lensfun.json
fi

if command -v cargo >/dev/null && rustup target list --installed 2>/dev/null | grep -q wasm32-unknown-unknown; then
  # Target flags (simd128, the getrandom backend) live in .cargo/config.toml;
  # setting RUSTFLAGS here would silently replace them.
  cargo build --release --manifest-path core/Cargo.toml --target wasm32-unknown-unknown
  cp core/target/wasm32-unknown-unknown/release/autoraw_core.wasm web/src/lib/wasm/autoraw_core.wasm
  echo "rebuilt the wasm module from core/"
else
  test -f web/src/lib/wasm/autoraw_core.wasm || {
    echo "no Rust toolchain and no committed web/src/lib/wasm/autoraw_core.wasm" >&2
    exit 1
  }
  echo "no Rust toolchain here, using the committed wasm module"
fi

(cd web && npm ci --no-audit --no-fund && npm run build)

printf '\n%-28s %s\n' "wasm module" "$(du -h web/src/lib/wasm/autoraw_core.wasm | cut -f1)"
printf '%-28s %s\n' "lens database" "$(du -h web/src/lib/wasm/lensfun.json | cut -f1)"
printf '%-28s %s\n' "site total" "$(du -sh web/build | cut -f1)"
echo
echo "serve it with:  python -m http.server -d web/build 8790"
