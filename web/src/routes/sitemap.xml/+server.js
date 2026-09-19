// Every page worth finding, in both languages, for search engines.
import { SITE } from '$lib/brand.js';

export const prerender = true;

const PAGES = ['/', '/guide', '/about', '/cameras', '/merge'];

const address = (path, lang) =>
  SITE + (lang === 'es' ? '/es' : '') + (path === '/' ? (lang === 'es' ? '' : '/') : path);

export function GET() {
  const today = new Date().toISOString().slice(0, 10);
  const urls = PAGES.flatMap((path) => ['en', 'es'].map((lang) => `  <url>
    <loc>${address(path, lang)}</loc>
    <lastmod>${today}</lastmod>
    <xhtml:link rel="alternate" hreflang="en" href="${address(path, 'en')}"/>
    <xhtml:link rel="alternate" hreflang="es" href="${address(path, 'es')}"/>
    <xhtml:link rel="alternate" hreflang="x-default" href="${address(path, 'en')}"/>
  </url>`)).join('\n');
  const body = `<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9"
        xmlns:xhtml="http://www.w3.org/1999/xhtml">
${urls}
</urlset>
`;
  return new Response(body, { headers: { 'Content-Type': 'application/xml' } });
}
