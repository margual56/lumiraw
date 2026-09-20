/** Installing the page as an app, where the browser can do it. */
export const install = $state({ available: false });

let prompt = null;

function offer(event) {
  event.preventDefault();
  prompt = event;
  install.available = true;
}

if (typeof window !== 'undefined') {
  if (window.__install) offer(window.__install);
  addEventListener('beforeinstallprompt', offer);
  addEventListener('appinstalled', () => { prompt = null; install.available = false; });
}

/** Open the browser's own install dialog. A prompt can be used once; if it is
 *  declined, the browser sends a new one when it is willing to ask again. */
export async function installApp() {
  if (!prompt) return;
  const used = prompt;
  prompt = null;
  install.available = false;
  await used.prompt();
}
