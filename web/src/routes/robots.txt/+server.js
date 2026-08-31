// Everything may be crawled; the sitemap says where the pages are.
import { SITE } from '$lib/brand.js';

export const prerender = true;

export function GET() {
  return new Response(`User-agent: *\nAllow: /\n\nSitemap: ${SITE}/sitemap.xml\n`,
                      { headers: { 'Content-Type': 'text/plain' } });
}
