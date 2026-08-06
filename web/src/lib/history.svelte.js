/** Undo and redo, over the settings. */

import { app } from './state.svelte.js';

/** How long the settings must hold still before they count as a step. */
const SETTLE_MS = 450;
const DEPTH = 100;

let past = [];
let future = [];
let settled = null;     // the snapshot the next change will be undone to
let timer = null;

export const history = $state({ canUndo: false, canRedo: false });

function publish() {
  history.canUndo = past.length > 0;
  history.canRedo = future.length > 0;
}

/** Forget everything. For a new photograph, and on first load. */
export function resetHistory() {
  clearTimeout(timer);
  past = [];
  future = [];
  settled = JSON.stringify(app.settings);
  publish();
}

/** Called whenever the settings may have changed. */
export function observe() {
  const now = JSON.stringify(app.settings);
  if (settled === null) {
    settled = now;
    return;
  }
  clearTimeout(timer);
  if (now === settled) {
    // Moved back to where it was before it settled: nothing happened.
    publish();
    return;
  }
  // Undoable already, before it has settled into a step of its own.
  history.canUndo = true;
  timer = setTimeout(() => {
    past.push(settled);
    if (past.length > DEPTH) past.shift();
    future = [];
    settled = now;
    publish();
  }, SETTLE_MS);
}

function restore(snapshot) {
  clearTimeout(timer);
  // Set before the settings change, so `observe` sees nothing new to record.
  settled = snapshot;
  app.settings = JSON.parse(snapshot);
  publish();
}

export function undo() {
  // A change still settling is the most recent step: undo that first.
  const now = JSON.stringify(app.settings);
  if (now !== settled) {
    future.push(now);
    restore(settled);
    return;
  }
  if (!past.length) return;
  future.push(now);
  restore(past.pop());
}

export function redo() {
  if (!future.length) return;
  past.push(JSON.stringify(app.settings));
  restore(future.pop());
}

/** Ctrl+Z / Cmd+Z, and Shift or Y for redo. Left alone inside a text field,
 *  where the browser's own undo is what the person means. */
export function onKey(event) {
  if (!(event.ctrlKey || event.metaKey) || event.altKey) return;
  const target = event.target;
  const typing = target?.isContentEditable
    || (target?.tagName === 'INPUT' && !['range', 'checkbox', 'radio', 'button'].includes(target.type))
    || target?.tagName === 'TEXTAREA';
  if (typing || !app.id) return;
  const key = event.key.toLowerCase();
  if (key === 'z' && !event.shiftKey) {
    event.preventDefault();
    undo();
  } else if ((key === 'z' && event.shiftKey) || key === 'y') {
    event.preventDefault();
    redo();
  }
}
