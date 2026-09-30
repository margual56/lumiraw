/** A small locale store. */

import { base } from '$app/paths';
import en from './locales/en.js';
import es from './locales/es.js';

export const LOCALES = [
  { id: 'en', name: 'English', dict: en },
  { id: 'es', name: 'Español', dict: es },
];

const STORAGE_KEY = 'lumiraw.locale';

export const i18n = $state({ locale: 'en' });

function known(id) {
  return LOCALES.some((locale) => locale.id === id) ? id : null;
}

/** Saved choice first, then the browser's own preference order. */
export function detectLocale() {
  try {
    const saved = known(localStorage.getItem(STORAGE_KEY));
    if (saved) return saved;
  } catch { /* private browsing */ }
  const preferred = navigator.languages?.length ? navigator.languages : [navigator.language];
  for (const tag of preferred) {
    const match = known(String(tag ?? '').toLowerCase().split('-')[0]);
    if (match) return match;
  }
  return 'en';
}

/** The language a page is in, as its address says. */
export function useLocale(id) {
  i18n.locale = known(id) ?? 'en';
  if (typeof document !== 'undefined') document.documentElement.lang = i18n.locale;
}

/** A language picked by hand, remembered for the next visit. */
export function setLocale(id) {
  useLocale(id);
  try {
    localStorage.setItem(STORAGE_KEY, i18n.locale);
  } catch { /* private browsing */ }
}

/** Where `path` (as written for English: '/', '/about') lives in `locale`. */
export function localized(path, locale = i18n.locale) {
  const at = address(path, locale);
  return base ? `${base}${at === '/' ? '' : at}` || '/' : at;
}

/** The same, from the site's root, for an absolute URL. */
export function address(path, locale = i18n.locale) {
  const prefix = locale === 'es' ? '/es' : '';
  return `${prefix}${path === '/' ? '' : path}` || '/';
}

/** A page's address with its language taken off: the English form. */
export function unlocalized(pathname) {
  const rest = pathname.slice(base.length).replace(/^\/es(?=\/|$)/, '');
  return rest.replace(/\/$/, '') || '/';
}

const dictionary = () =>
  (LOCALES.find((locale) => locale.id === i18n.locale) ?? LOCALES[0]).dict;

/** Translate `key`, filling `{placeholders}` from `params`. */
export function t(key, params) {
  const template = dictionary()[key] ?? en[key];
  if (template === undefined) return key;
  if (!params) return template;
  return template.replace(/\{(\w+)\}/g, (whole, name) =>
    (params[name] === undefined ? whole : String(params[name])));
}

/** Numbers in the reader's own notation, 1,36 in Spanish, 1.36 in English. */
export function n(value, digits = 2) {
  if (value === null || value === undefined || Number.isNaN(Number(value))) return '—';
  return new Intl.NumberFormat(i18n.locale, {
    minimumFractionDigits: digits,
    maximumFractionDigits: digits,
  }).format(Number(value));
}

export function signed(value, digits = 2) {
  if (value === null || value === undefined) return '—';
  return `${Number(value) > 0 ? '+' : ''}${n(value, digits)}`;
}

/** Trims trailing zeros: 8 rather than 8.00, 7.1 rather than 7.10. */
export function loose(value) {
  if (value === null || value === undefined) return '—';
  return new Intl.NumberFormat(i18n.locale, { maximumFractionDigits: 2 }).format(Number(value));
}

export function shutter(seconds) {
  if (!seconds) return '—';
  return seconds < 1 ? `1/${Math.round(1 / seconds)} s` : `${loose(seconds)} s`;
}
