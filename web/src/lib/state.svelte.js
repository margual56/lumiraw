/** The whole wizard's state, as runes. */

export const STEPS = ['step.upload', 'step.framing', 'step.refocus', 'step.brightness',
                      'step.wb', 'step.vibrance', 'step.compare', 'step.looks',
                      'step.download'];

export const UPLOAD = 0, FRAME = 1, REFOCUS = 2, EXPOSURE = 3, WB = 4,
             VIBRANCE = 5, COMPARE = 6, LOOKS = 7, DOWNLOAD = 8;

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

export const app = $state({
  id: null,
  version: __APP_VERSION__,   // stamped in at build time, from core/Cargo.toml
  info: null,
  step: UPLOAD,
  settings: defaultSettings(),
  ratio: '',                 // locked crop aspect, '' = free
  keepGrade: true,           // carry the grade to the next photograph
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
  output: { key: null, image: '' },     // download step, fully graded
});

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
  app.output = { key: null, image: '' };
}
