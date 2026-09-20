/** Turns the backend's structured reports into sentences in the current locale. */

import { NAME } from './brand.js';
import { t, n, signed, loose, shutter } from './i18n.svelte.js';

/**
 * Every raw suffix the decoder (`rawler`, in `core/vendor`) has a reader for, for
 * the file pickers.
 */
export const RAW_ACCEPT = [
  '.arw', '.sr2', '.srf',                 // Sony
  '.cr2', '.cr3', '.crw',                 // Canon
  '.nef', '.nrw',                         // Nikon
  '.raf',                                 // Fujifilm
  '.orf',                                 // OM System, Olympus
  '.rw2', '.raw', '.rwl',                 // Panasonic, Leica
  '.pef',                                 // Pentax
  '.srw',                                 // Samsung
  '.3fr', '.fff', '.iiq', '.mos', '.erf', '.kdc', '.dcr', '.mef', '.mrw', '.ari', '.x3f',
  '.dng',                                 // phones, Leica, Pentax, Ricoh, and converters
].join(',');

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
    case 'refocus':
      return t('detail.refocus', { amount: n(p.amount), blur: n(p.blur, 1) });
    case 'sharpen':
      return t('detail.sharpen', { amount: n(p.amount), radius: n(p.radius) });
    default:
      return '';
  }
}

/** 1, 2 or 3: a little, some, a lot, by where `value` falls against two
 *  thresholds. */
const degree = (value, some, lot) => (value < some ? 1 : value < lot ? 2 : 3);

/** The same line in words a photographer would use, for the list itself. */
export function toggleSummary(detail) {
  if (!detail || !detail.code) return '';
  const p = detail.params ?? {};
  switch (detail.code) {
    case 'white_balance': {
      // The two axes the automatic balance itself works along (see
      // `analyze::auto_white_balance`), in stops.
      const [r, g, b] = (p.gains ?? [1, 1, 1]).map((x) => Math.log2(Math.max(x, 1e-4)));
      const temp = (r - b) / 2;
      const tint = (2 * g - r - b) / 3;
      let text = Math.abs(temp) < 0.08
        ? t('sum.wb.same')
        : t(`sum.wb.${temp < 0 ? 'cooler' : 'warmer'}.${degree(Math.abs(temp), 0.25, 0.6)}`);
      if (Math.abs(tint) >= 0.15) text += t(tint > 0 ? 'sum.wb.green' : 'sum.wb.magenta');
      if (p.source === 'selection') text += t('sum.wb.selection');
      if (p.extreme) text += t('sum.wb.extreme');
      return text;
    }
    case 'tone_map':
      return t(p.compression < 0.97 ? 'sum.tone_map.squeezed' : 'sum.tone_map.fits',
               { stops: n(p.stops, 1) });
    case 'levels': {
      const wider = Math.round((1 / Math.max(p.white - p.black, 1e-3) - 1) * 100);
      return wider < 2 ? t('sum.levels.full') : t('sum.levels.stretched', { pct: wider });
    }
    case 'contrast':
      return t(`sum.contrast.${degree(p.amount, 0.12, 0.3)}`);
    case 'vibrance': {
      const richer = Math.round((p.boost - 1) * 100);
      return richer < 2 ? t('sum.vibrance.same') : t('sum.vibrance.more', { pct: richer });
    }
    case 'denoise':
      return t(`sum.denoise.${degree(p.blend, 0.25, 0.6)}`);
    case 'refocus':
      return t('sum.refocus', { blur: n(p.blur, 1) });
    case 'sharpen':
      return t(`sum.sharpen.${degree(p.amount, 0.3, 0.6)}`);
    default:
      return toggleDetail(detail);
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
/** A failure as a sentence. */
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
