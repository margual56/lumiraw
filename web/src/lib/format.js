/** Turns the backend's structured reports into sentences in the current locale. */

import { NAME } from './brand.js';
import { t, n, signed, loose, shutter } from './i18n.svelte.js';

/** A look's name in the reader's language. */
export const lookLabel = (id, fallback = '') => {
  const key = `look.${id}`;
  const shown = t(key);
  return shown === key ? (fallback || id) : shown;
};
export const lookDescription = (id, fallback = '') => {
  const key = `look.${id}.description`;
  const shown = t(key);
  return shown === key ? fallback : shown;
};
export const formatLabel = (id) => t(`format.${id}`);

/** One line under a correction's name, saying what it did. */
export function toggleDetail(detail) {
  if (!detail || !detail.code) return '';
  const p = detail.params ?? {};
  switch (detail.code) {
    case 'reason':
      return t(`reason.${p.reason}`);
    case 'vignetting':
      return p.corner_ev === null || p.corner_ev === undefined
        ? t('detail.vignetting.plain', { lens: p.lens, aperture: p.aperture })
        : t('detail.vignetting',
             { ev: signed(p.corner_ev), lens: p.lens, aperture: p.aperture });
    case 'lens':
      return t('detail.lens', { lens: p.lens, focal: p.focal });
    case 'white_balance': {
      const gains = (p.gains ?? []).map((g) => n(g)).join('/');
      const key = p.extreme ? 'detail.white_balance.extreme' : 'detail.white_balance';
      return t(key, { source: t(`source.${p.source}`), gains });
    }
    case 'exposure':
      return t('detail.exposure', { ev: signed(p.ev), source: t(`source.${p.source}`) });
    case 'tone_map':
      return t('detail.tone_map', { stops: n(p.stops, 1), compression: n(p.compression) });
    case 'levels':
      return t('detail.levels', { black: n(p.black, 3), white: n(p.white, 3) });
    case 'contrast':
      return t('detail.contrast', { amount: n(p.amount), spread: n(p.spread) });
    case 'vibrance':
      return t('detail.vibrance', { boost: n(p.boost) });
    case 'denoise':
      return t('detail.denoise', { blend: n(p.blend), radius: loose(p.radius) });
    case 'sharpen':
      return t('detail.sharpen', { amount: n(p.amount), radius: n(p.radius) });
    default:
      return '';
  }
}

/** The capture profile, as label/value pairs ready to render. */
export function profileRows(profile) {
  if (!profile) return [];
  return [
    ['profile.camera', profile.camera],
    ['profile.lens', profile.lens || t('profile.unknownLens')],
    ['profile.iso', String(profile.iso)],
    ['profile.aperture', `f/${loose(profile.aperture)}`],
    ['profile.focal', t('profile.focalValue', {
      focal: loose(profile.focal_mm), eq: n(profile.focal35_mm, 0),
    })],
    ['profile.shutter', shutter(profile.shutter_s)],
    ['profile.sensor', t('profile.sensorValue', {
      mp: n(profile.megapixels, 1), pitch: n(profile.pixel_pitch_um),
      crop: n(profile.crop_factor),
    })],
    ['profile.diffraction', t('profile.diffractionValue', {
      um: n(profile.airy_um, 1), px: n(profile.diffraction_ratio, 1),
    })],
    ['profile.noise', n(profile.noise_prior)],
    ['profile.stops', n(profile.usable_stops, 1)],
  ].map(([key, value]) => ({ label: t(key), value }));
}

export function noteText(note) {
  const p = note?.params ?? {};
  switch (note?.code) {
    case 'diffraction':
      return t('note.diffraction', {
        aperture: loose(p.aperture), pitch: n(p.pitch), ratio: n(p.ratio, 1), sigma: n(p.sigma),
      });
    case 'noise':
      return t('note.noise', { iso: String(p.iso), stops: n(p.stops, 1) });
    case 'shake':
      return t('note.shake', { shutter: shutter(p.shutter_s), focal35: n(p.focal35, 0) });
    case 'exposure_comp':
      return t('note.exposure_comp', { ev: signed(p.ev, 1) });
    default:
      return '';
  }
}

/** What a running export is doing right now. */
export function stageText(stage, params = {}) {
  const filled = { ...params };
  if (filled.format) filled.format = formatLabel(filled.format);
  return t(`stage.${stage}`, filled);
}

/** Server errors carry a code when they have one; fall back to its English. */
export function errorText(error) {
  if (error?.reason) {
    // `message` is always available as a placeholder: some reports carry
    // detail the sentence wants to quote rather than paraphrase.
    const translated = t(`error.${error.reason}`,
                         { app: NAME, message: error.message ?? '',
                           ...(error.params ?? {}) });
    if (translated !== `error.${error.reason}`) return translated;
  }
  return error?.message ?? String(error);
}
