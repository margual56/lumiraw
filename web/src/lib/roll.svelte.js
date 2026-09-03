/** Several photographs at once: the roll, the filmstrip and the batch export. */

import * as api from './api.js';
import { app, resetForNewPhoto, settingsFor, usable, FRAME, UPLOAD, DOWNLOAD } from './state.svelte.js';
import { fileKey, recall } from './memory.js';
import { t } from './i18n.svelte.js';
import { errorText, RAW_ACCEPT } from './format.js';
import { zip } from './zip.js';
import { stamp } from './wasm/stamp.js';

const SUFFIXES = RAW_ACCEPT.split(',');
const isRaw = (name) => SUFFIXES.some((s) => name.toLowerCase().endsWith(s));

let keys = 0;

/** One file opening at a time. */
let opening = false;

/** Everything raw in a drop, folders included, in name order. */
export async function filesFromDrop(dataTransfer) {
  const entries = [...(dataTransfer.items ?? [])]
    .map((item) => item.webkitGetAsEntry?.())
    .filter(Boolean);
  if (!entries.length) return [...dataTransfer.files];
  const out = [];
  const walk = async (entry) => {
    if (entry.isFile) {
      out.push(await new Promise((resolve, reject) => entry.file(resolve, reject)));
    } else if (entry.isDirectory) {
      const reader = entry.createReader();
      for (;;) {
        const batch = await new Promise((resolve, reject) => reader.readEntries(resolve, reject));
        if (!batch.length) break;
        for (const child of batch) await walk(child);
      }
    }
  };
  for (const entry of entries) await walk(entry);
  return out;
}

/** Start a roll from whatever was dropped or chosen, and open its first frame. */
export async function pickMany(files) {
  const raws = [...files].filter((f) => isRaw(f.name))
    .sort((a, b) => a.name.localeCompare(b.name, undefined, { numeric: true }));
  if (!raws.length) {
    // A single non-raw goes through anyway, so the decoder can say what it is.
    if (files.length === 1) return openFile(files[0], { roll: [files[0]] });
    app.message = t('roll.none');
    return;
  }
  const skipped = files.length - raws.length;
  await openFile(raws[0], { roll: raws });
  if (skipped > 0 && app.id) app.message = t('roll.skipped', { count: skipped });
}

function newRoll(files) {
  for (const item of app.roll) if (item.thumb) URL.revokeObjectURL(item.thumb);
  app.roll = files.map((file) => ({
    key: ++keys, name: file.name, file, thumb: '', keep: true, settings: null, failed: false,
  }));
  app.rollAt = -1;
  thumbnails();
}

/** Open one file. */
export async function openFile(file, { roll = null, index = 0 } = {}) {
  if (opening) return;
  opening = true;
  app.busy = true;
  app.busyKey = 'busy.decode';
  app.busyParams = {};
  app.message = t('upload.decoding', { file: file.name });
  // Keep what the one being left was set to before anything can replace it.
  const leaving = app.rollAt;
  const kept = leaving >= 0 && app.id ? $state.snapshot(app.settings) : null;
  try {
    const info = await api.upload(file);
    if (roll) {
      newRoll(roll);
    } else if (leaving >= 0 && app.roll[leaving]) {
      app.roll[leaving].settings = kept;
    }
    // What this photograph was left at: this visit, from the roll, or an
    // earlier one, from this browser's memory of the file.
    const own = roll ? null : app.roll[index]?.settings;
    const saved = own ? null : usable(recall(fileKey(file)));
    const wasOpen = !!app.id;
    resetForNewPhoto();
    if (own) app.settings = $state.snapshot(own);
    else if (saved) app.settings = saved;
    app.fileKey = fileKey(file);
    app.restored = !!saved;
    app.id = info.id;
    app.info = info;
    app.rollAt = roll ? 0 : index;
    app.message = '';
    // From the drop zone into the wizard; from the filmstrip, stay on the
    // step you were on, which is what comparing frames needs.
    if (!wasOpen || app.step === UPLOAD) app.step = app.straight ? DOWNLOAD : FRAME;
  } catch (err) {
    app.message = errorText(err);
    if (!roll && app.roll[index]) app.roll[index].failed = true;
  } finally {
    opening = false;
    app.busy = false;
    app.busyKey = '';
    app.progress = null;
  }
}

export function openAt(index) {
  const item = app.roll[index];
  if (!item || index === app.rollAt) return;
  return openFile(item.file, { index });
}

/** Thumbnails, one after another, behind whatever else the worker is doing.
 *  A roll replaced halfway through stops this one. */
async function thumbnails() {
  const roll = app.roll;
  for (const item of roll) {
    if (app.roll !== roll) return;
    if (item.thumb) continue;
    try {
      item.thumb = await api.thumbnail(item.file);
    } catch {
      // No embedded preview: the strip shows the name instead.
      item.thumb = '';
    }
  }
}

/** Export every photograph marked to keep, into one zip. */
export async function exportRoll({ format, quality, max_size, onstatus }) {
  const chosen = app.roll.map((item, i) => ({ item, i })).filter(({ item }) => item.keep);
  const settings = chosen.map(({ i }) => settingsFor(i));
  const now = stamp();
  const files = [];
  const failed = [];
  const current = app.rollAt;
  try {
    for (const [n, { item }] of chosen.entries()) {
      onstatus?.(t('roll.exporting', { n: n + 1, total: chosen.length, name: item.name }), n / chosen.length);
      try {
        await api.upload(item.file);
        const out = await api.exportOnce({
          settings: settings[n], format, quality, max_size,
          original_name: item.name,
        }, (fraction) => onstatus?.(
          t('roll.exporting', { n: n + 1, total: chosen.length, name: item.name }),
          (n + fraction) / chosen.length));
        files.push({ name: out.filename, blob: out.blob, captured: out.captured });
      } catch (err) {
        failed.push(`${item.name}: ${errorText(err)}`);
      }
    }
  } finally {
    // Put back the photograph that was being developed. The page's own id is
    // kept: its previews and history still describe this same file.
    const item = app.roll[current];
    if (item) await api.upload(item.file).catch(() => {});
  }
  if (!files.length) throw new Error(failed.join('\n') || t('roll.nothing'));
  // Two exports of one stem (a raw and its DNG, say) must not overwrite
  // each other inside the zip.
  const seen = new Map();
  for (const f of files) {
    const count = seen.get(f.name) ?? 0;
    seen.set(f.name, count + 1);
    if (count) f.name = f.name.replace(/(\.[^.]+)$/, `_${count + 1}$1`);
  }
  const blob = await zip(files, now);
  return { blob, count: files.length, failed };
}
