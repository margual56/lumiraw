import adapter from '@sveltejs/adapter-static';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

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
    // The language picker isn't a link, so list the Spanish pages here.
    prerender: {
      entries: ['*', '/es', '/es/about', '/es/cameras', '/es/merge', '/es/guide', '/es/source'],
    },
  },
};
