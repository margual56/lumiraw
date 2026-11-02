import { loadLocale } from '$lib/i18n.svelte.js';

// Rendered once, at build time, into static HTML.
export const prerender = true;

export const load = ({ params }) => loadLocale(params.lang);
