import adapter from '@sveltejs/adapter-static';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';
import { PAGES } from './src/lib/pages.js';

/** Everything runs in the browser, so the build is a plain static site. */
export default {
  preprocess: vitePreprocess(),
  kit: {
    adapter: adapter({
      pages: 'build',
      assets: 'build',
      fallback: '404.html',
      precompress: false,
      strict: true,
    }),
    // Registered by hand in the root layout, and not at all in the desktop
    // app, which has its files on disk already.
    serviceWorker: { register: false },
    // The language picker isn't a link, so list the Spanish pages here.
    prerender: {
      entries: ['*', ...PAGES.map((path) => (path === '/' ? '/es' : `/es${path}`))],
    },
  },
};
