/** Runs only while the site is rendered at build time. */
import { base } from '$app/paths';

export async function handle({ event, resolve }) {
  const spanish = new RegExp(`^${base}/es(/|$)`).test(event.url.pathname);
  return resolve(event, {
    transformPageChunk: ({ html }) => html.replace('%lang%', spanish ? 'es' : 'en'),
  });
}
