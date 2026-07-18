/** The whole wizard's state, as runes. */

export const STEPS = ['step.upload', 'step.framing', 'step.brightness', 'step.wb',
                      'step.vibrance', 'step.compare', 'step.looks', 'step.download'];

export const UPLOAD = 0, FRAME = 1, EXPOSURE = 2, WB = 3,
             VIBRANCE = 4, COMPARE = 5, LOOKS = 6, DOWNLOAD = 7;

export const defaultSettings = () => ({
  framing: { angle: 0, perspective_v: 0, perspective_h: 0, crop: null, auto_fit: true },
  exposure_rect: null,
  exposure_bias: 0,
  shadows: 0,
  midtones: 0,
  highlights: 0,
  wb_rect: null,
  temperature: 0,
  tint: 0,
  vibrance: 0,
  preset: 'natural',
  enabled: {},
});

export const app = $state({
  id: null,
  version: __APP_VERSION__,   // stamped in at build time, from core/Cargo.toml
  info: null,
  step: UPLOAD,
  settings: defaultSettings(),
  chosen: ['original'],
  ratio: '',                 // locked crop aspect, '' = free
  coverCrop: null,           // largest empty-corner-free crop for the current tilt
  toggles: [],

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
  grid: { key: null, tiles: [] },
  output: { key: null, image: '' },     // download step, with the look applied
});

/** Everything a render depends on. */
export function settingsKey() {
  return `${app.id}|${JSON.stringify(app.settings)}`;
}

/** A crop rectangle or a metering box belongs to the picture it was drawn on;
 *  carrying either onto the next file silently develops it wrongly. */
export function resetForNewPhoto() {
  app.settings = defaultSettings();
  app.chosen = ['original'];
  app.ratio = '';
  app.coverCrop = null;
  app.toggles = [];
  app.exportState = '';
  app.job = null;
  app.progress = null;
  app.frame = { key: null, image: '', angle: 0 };
  app.preview = { key: null, image: '' };
  app.wipe = { key: null, before: '', after: '' };
  app.grid = { key: null, tiles: [] };
  app.output = { key: null, image: '' };
}
