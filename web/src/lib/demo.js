/**
 * The developer inside a blog post (blog.lumiraw.app): the post hands it a
 * sample raw file with postMessage, and the step to open it at, and the
 * reader tries that step on a real photo. The post can move it to another
 * step, and is told whenever the reader moves, to keep its text alongside. The file comes from the page that
 * frames the app, so the app itself still connects to nothing but its own
 * address. Only the blog may frame it (web/static/_headers), and only the
 * blog's messages are read.
 */
import { openFile } from './roll.svelte.js';
import { app, stepVisible, FRAME, DOWNLOAD } from './state.svelte.js';

/** The pages allowed to hand it a file: the blog and its previews. */
const EMBEDDERS = [/^https:\/\/blog\.lumiraw\.app$/, /^https:\/\/[a-z0-9-]+\.margual56\.workers\.dev$/];

/** The steps a post can open at, by the name it uses for them. */
export const STEP_NAMES = {
  framing: FRAME, focus: 2, brightness: 3, 'white-balance': 4, colour: 5,
  local: 6, compare: 7, grade: 8, download: DOWNLOAD,
};

/** Whether this page is the developer inside a post: framed, and asked to be. */
export const embedded = () => window.parent !== window && new URLSearchParams(location.search).has('embed');

/** Whether a message from `origin` may open a file. A copy running on this
 *  machine also takes them from another one, to try a post locally. */
export function allowed(origin, hostname = location.hostname) {
  if (EMBEDDERS.some((pattern) => pattern.test(origin))) return true;
  return hostname === 'localhost' && /^http:\/\/localhost(:\d+)?$/.test(origin);
}

/** The step to show for a post's name for it, past any this photo doesn't need. */
export function stepFor(name, visible = stepVisible) {
  let step = STEP_NAMES[name] ?? FRAME;
  while (step < DOWNLOAD && !visible(step)) step += 1;
  return step;
}

// The post that sent the photo, the only one told where the reader is.
let embedder = null;

/** Tells the post which step the reader is on, by its name. */
export function report(step) {
  const name = Object.keys(STEP_NAMES).find((key) => STEP_NAMES[key] === step);
  if (embedder && name) window.parent.postMessage({ type: 'lumiraw-at', step: name }, embedder);
}

/** Waits for the post's file, opens it, and goes to the step it asks for,
 *  then to any other the post asks for. Says it is ready first, so the post
 *  knows when to send it. Returns the way to stop listening. */
export function listen() {
  const receive = async (event) => {
    if (!allowed(event.origin)) return;
    if (event.data?.type === 'lumiraw-step') {
      if (app.id) app.step = stepFor(event.data.step);
      return;
    }
    if (event.data?.type !== 'lumiraw-demo') return;
    const { file, name, step } = event.data;
    if (!(file instanceof Blob) || typeof name !== 'string') return;
    embedder = event.origin;
    const raw = new File([file], name);
    await openFile(raw, { roll: [raw] });
    if (app.id) app.step = stepFor(step);
  };
  addEventListener('message', receive);
  window.parent.postMessage({ type: 'lumiraw-ready' }, '*');
  return () => removeEventListener('message', receive);
}
