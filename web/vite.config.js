import { sveltekit } from '@sveltejs/kit/vite';

export default {
  plugins: [sveltekit()],
  // The pipeline is a wasm module fetched at runtime, so there is no backend
  // to proxy to any more: `npm run dev` is the whole application.
  worker: { format: 'es' },
};
