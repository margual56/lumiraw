import adapter from '@sveltejs/adapter-static';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

/** Everything runs in the browser, so the build is a plain static site. */
export default {
  preprocess: vitePreprocess(),
  kit: {
    adapter: adapter({
      pages: 'build',
      assets: 'build',
      // Served by Cloudflare Pages, with a 404 status, for any address that is
      // not a page.
      fallback: '404.html',
      precompress: false,
      strict: true,
    }),
    // The Spanish pages are reached through the language picker, which is a
    // select rather than a link, so the prerenderer is told about them.
    prerender: {
      entries: ['*', '/es', '/es/about', '/es/cameras', '/es/merge', '/es/guide'],
    },
  },
};
