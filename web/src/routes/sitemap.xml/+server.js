// Every page worth finding, for search engines. Written out at build time
// from `SITE`, so it follows the domain wherever that goes.
import { SITE } from '$lib/brand.js';

export const prerender = true;

const PAGES =['/', '/merge', '/about', '/cameras'];

export function GET() {
  const urls = PAGES.map((path) => `  <url><loc>${SITE}${path}</loc></url>`).join('\n');
  const body = `<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
${urls}
</urlset>
`;
  return new Response(body, { headers: { 'Content-Type': 'application/xml' } });
}
