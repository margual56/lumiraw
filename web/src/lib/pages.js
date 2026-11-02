/** Every page of the site, as the navigation lists them: the tools, then the
 *  pages to read. Plain data, so `svelte.config.js` and the sitemap read the
 *  same list as the pages do. */

export const TOOLS = [
  { path: '/', key: 'site.develop' },
  { path: '/merge', key: 'site.merge' },
  { path: '/cull', key: 'site.cull' },
  { path: '/sky', key: 'site.sky' },
  { path: '/calculators', key: 'site.calc' },
];

/** The sky page's calculators, as their tabs and addresses name them (#sun…). */
export const SKY = ['sun', 'dark', 'stars'];

export const READING = [
  { path: '/guide', key: 'links.guide' },
  { path: '/about', key: 'links.about' },
  { path: '/cameras', key: 'links.cameras' },
  { path: '/source', key: 'links.source' },
];

/** Pages on other sites, listed after the pages to read. Not part of PAGES:
 *  they are not built or put in the sitemap here. */
export const ELSEWHERE = [
  { href: { en: 'https://blog.lumiraw.app', es: 'https://blog.lumiraw.app/es' }, key: 'links.blog' },
];

export const PAGES = [...TOOLS, ...READING].map((p) => p.path);
