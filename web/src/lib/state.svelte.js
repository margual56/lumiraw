/** The whole wizard's state, as runes. */

import { fileKey, recall } from './memory.js';

export const STEPS = ['step.upload', 'step.framing', 'step.refocus', 'step.brightness',
                      'step.wb', 'step.vibrance', 'step.local', 'step.compare', 'step.looks',
                      'step.download'];

export const UPLOAD = 0, FRAME = 1, REFOCUS = 2, EXPOSURE = 3, WB = 4,
             VIBRANCE = 5, LOCAL = 6, COMPARE = 7, LOOKS = 8, DOWNLOAD = 9;

/** Whether a step is worth showing for the photograph in hand. */
export const stepVisible = (i) => i !== REFOCUS || app.focus?.verdict === 'soft';

export const defaultSettings = () => ({
  framing: { angle: 0, perspective_v: 0, perspective_h: 0, crop: null, auto_fit: true },
  exposure_rect: null,
  exposure_bias: 0,
  shadows: 0,
  midtones: 0,
  highlights: 0,
  refocus: 0,
  clarity: 0,        // local contrast, -1..1, on top of what the tone map kept
  denoise: 0,        // noise reduction, -1..1, on top of what was measured
  // Local work, in fractions of the framed picture.
  filters: [],
  spots: [],         // [{ at, r }]: healed spots
  wb_rect: null,
  temperature: 0,
  tint: 0,
  vibrance: 0,
  preset: 'natural',
  // The grade.
  curves: { look: 'none', strength: 1, rgb: [], r: [], g: [], b: [] },
  // Eight hue bands, each with a hue, a saturation and a lightness. Only the
  // bands that have been touched are here; the rest are absent and mean zero.
  mixer: {},
  // How much of the loaded 3D table to apply, and which one is loaded.
  lut: 0,
  lut_id: '',
  // The three things a curve cannot do, each 0..1. See `core/src/effects.rs`.
  monochrome: 0,
  vignette: 0,
  grain: 0,
  enabled: {},
});

/** The part of the settings that is a grade rather than a correction. */
export const GRADE_KEYS = ['preset', 'curves', 'mixer', 'lut', 'lut_id',
                           'monochrome', 'vignette', 'grain'];

/** Whether these settings carry any grade at all. */
export function hasGrade(settings) {
  const blank = defaultSettings();
  return GRADE_KEYS.some((k) => JSON.stringify(settings[k]) !== JSON.stringify(blank[k]));
}

function readStraight() {
  try {
    return typeof localStorage !== 'undefined' && localStorage.getItem('lumiraw.straight') === '1';
  } catch {
    return false;
  }
}

export function setStraight(on) {
  app.straight = on;
  try {
    localStorage.setItem('lumiraw.straight', on ? '1' : '0');
  } catch {
    // Private windows may refuse; the choice still holds for this visit.
  }
}

export const app = $state({
  id: null,
  version: __APP_VERSION__,   // stamped in at build time, from core/Cargo.toml
  info: null,
  step: UPLOAD,
  settings: defaultSettings(),
  ratio: '',                 // locked crop aspect, '' = free
  keepGrade: true,           // carry the grade to the next photograph
  // Open photographs straight at the download step, everything automatic.
  straight: readStraight(),
  showHistogram: true,       // the histogram in the corner of the picture
  showClipping: false,       // blown and crushed marked on the picture (J)
  coverCrop: null,           // largest empty-corner-free crop for the current tilt
  framingHint: null,         // what the framing step measured and could offer
  focus: null,               // { verdict, edge_px, blur_px } from the pipeline
  toggles: [],
  looks: [],                 // the named grades, read out of the wasm once
  curve: null,               // { rgb, r, g, b } baked tables, for the plot
  lut: null,                 // the loaded .cube, if any: { size, title, id, bytes, name }

  // What is happening right now, for the progress bar.
  busy: false,
  busyKey: '',            // translation key for what is running
  busyParams: {},
  progress: null,     // { fraction, code } straight from the pipeline
  job: null,          // { progress, stage, elapsed }
  message: '',
  exportState: '',

  // Rendered results, each tagged with the settings they were produced from.
  frame: { key: null, image: '', angle: 0 },   // framing step: whole canvas, uncropped
  preview: { key: null, image: '' },    // the cropped picture
  wipe: { key: null, before: '', after: '' },
  ungraded: { key: null, image: '' },   // grade step's "before": everything but the grade
  output: { key: null, image: '' },     // download step, fully graded
  // The full-resolution frame for the 100 % view, developed only while that
  // view is open: { key, bitmap, width, height }.
  full: { key: null, bitmap: null, width: 0, height: 0 },

  // Every raw dropped together, for the filmstrip.
  roll: [],
  rollAt: -1,

  // Which file the open photograph came from, as `memory.fileKey` puts it, so
  // its settings can be remembered.
  fileKey: null,
  restored: false,
});

/** The settings a photograph from the roll is exported with. */
export function settingsFor(index) {
  if (index === app.rollAt) return $state.snapshot(app.settings);
  const own = app.roll[index]?.settings ?? usable(recall(fileKey(app.roll[index]?.file)));
  if (own) return own;
  const grade = Object.fromEntries(GRADE_KEYS.map((k) => [k, $state.snapshot(app.settings[k])]));
  return { ...defaultSettings(), ...grade };
}

/** Remembered settings as they can be used now. */
export function usable(saved) {
  if (!saved) return null;
  const out = { ...defaultSettings(), ...saved };
  if (!app.lut || app.lut.id !== out.lut_id) {
    out.lut = 0;
    out.lut_id = '';
  }
  return out;
}

/** These settings with the grade step's own work taken off. */
export function ungraded(settings) {
  const blank = defaultSettings();
  return { ...$state.snapshot(settings), curves: blank.curves, mixer: blank.mixer,
           lut: 0, monochrome: 0, vignette: 0, grain: 0 };
}

/** Everything a render depends on. */
export function settingsKey() {
  return `${app.id}|${JSON.stringify(app.settings)}`;
}

/** A crop rectangle or a metering box belongs to the picture it was drawn on;
 *  carrying either onto the next file silently develops it wrongly. */
export function resetForNewPhoto() {
  // A loaded 3D table outlives the photograph it was first tried on.
  const table = app.lut && app.keepGrade
    ? { lut: app.settings.lut, lut_id: app.settings.lut_id } : {};
  const grade = app.keepGrade
    ? Object.fromEntries(GRADE_KEYS.filter((k) => !k.startsWith('lut'))
        .map((k) => [k, $state.snapshot(app.settings[k])]))
    : {};
  app.settings = { ...defaultSettings(), ...grade, ...table };
  app.fileKey = null;
  app.restored = false;
  app.ratio = '';
  app.coverCrop = null;
  app.focus = null;
  app.toggles = [];
  app.curve = null;
  app.exportState = '';
  app.job = null;
  app.progress = null;
  app.frame = { key: null, image: '', angle: 0 };
  app.preview = { key: null, image: '' };
  app.wipe = { key: null, before: '', after: '' };
  app.ungraded = { key: null, image: '' };
  app.output = { key: null, image: '' };
  app.full.bitmap?.close();
  app.full = { key: null, bitmap: null, width: 0, height: 0 };
}
