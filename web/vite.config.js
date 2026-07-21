import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { sveltekit } from '@sveltejs/kit/vite';
import tailwindcss from '@tailwindcss/vite';

/** Cargo owns the version. */
function version() {
  const manifest = fileURLToPath(new URL('../core/Cargo.toml', import.meta.url));
  const found = readFileSync(manifest, 'utf8').match(/^version\s*=\s*"([^"]+)"/m);
  return found ? found[1] : '0.0.0';
}

export default {
  plugins: [tailwindcss(), sveltekit()],
  define: { __APP_VERSION__: JSON.stringify(version()) },
  // The pipeline is a wasm module fetched at runtime, so there is no backend
  // to proxy to any more: `npm run dev` is the whole application.
  worker: { format: 'es' },
};
