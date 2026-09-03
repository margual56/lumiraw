/** Edits remembered between visits. */

const STORE = 'lumiraw.edits';
const LIMIT = 300;

export const fileKey = (file) => (file ? `${file.name}|${file.size}|${file.lastModified}` : null);

function load() {
  try {
    return JSON.parse(localStorage.getItem(STORE)) ?? {};
  } catch {
    return {};
  }
}

function save(all) {
  try {
    localStorage.setItem(STORE, JSON.stringify(all));
  } catch {
    // Full or refused: the edit still holds for this visit.
  }
}

/** The settings last left on this file, or null. */
export function recall(key) {
  return (key && load()[key]?.settings) || null;
}

export function remember(key, settings) {
  if (!key) return;
  const all = load();
  all[key] = { settings, at: Date.now() };
  const keys = Object.keys(all);
  if (keys.length > LIMIT) {
    keys.sort((a, b) => all[a].at - all[b].at)
      .slice(0, keys.length - LIMIT)
      .forEach((k) => delete all[k]);
  }
  save(all);
}

export function forget(key) {
  if (!key) return;
  const all = load();
  delete all[key];
  save(all);
}
