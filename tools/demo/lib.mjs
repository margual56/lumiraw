// What every video's script uses: the browser, a visible cursor (headless
// Chromium draws none), the frames as Chromium paints them, with the time of
// each, and the scene marks the narration is laid on afterwards.
//
// The window is 1280×720 at 1.5×, so the frames are 1080p and the app's text
// is a third bigger than in a 1920-wide window.
import { chromium } from 'playwright-core';
import { mkdirSync, writeFileSync, rmSync, readFileSync } from 'node:fs';

export const BASE = process.env.DEMO_URL || 'http://localhost:8790';
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const CURSOR = `
(() => {
  const add = () => {
    if (document.getElementById('__cur')) return;
    const c = document.createElement('div');
    c.id = '__cur';
    c.innerHTML = '<svg width="20" height="20" viewBox="0 0 24 24"><path d="M4 2 L4 19 L8.5 14.8 L11.6 21.4 L14.4 20.1 L11.3 13.6 L17.5 13.6 Z" fill="#fff" stroke="#111" stroke-width="1.4" stroke-linejoin="round"/></svg>';
    Object.assign(c.style, { position: 'fixed', left: '0', top: '0', zIndex: 2147483647, pointerEvents: 'none',
      transform: 'translate(var(--x,-100px), var(--y,-100px))', filter: 'drop-shadow(0 1px 2px rgba(0,0,0,.5))' });
    document.documentElement.appendChild(c);
    const st = document.createElement('style');
    st.textContent = '.__rip{position:fixed;width:34px;height:34px;margin:-17px 0 0 -17px;border-radius:50%;border:2px solid rgba(240,168,48,.9);pointer-events:none;z-index:2147483646;animation:__r .5s ease-out forwards}@keyframes __r{from{transform:scale(.3);opacity:1}to{transform:scale(1.4);opacity:0}}';
    document.documentElement.appendChild(st);
    const x = sessionStorage.getItem('__cx'), y = sessionStorage.getItem('__cy');
    if (x) { c.style.setProperty('--x', x + 'px'); c.style.setProperty('--y', y + 'px'); }
  };
  const move = (e) => { const c = document.getElementById('__cur'); if (!c) return;
    c.style.setProperty('--x', e.clientX + 'px'); c.style.setProperty('--y', e.clientY + 'px');
    try { sessionStorage.setItem('__cx', e.clientX); sessionStorage.setItem('__cy', e.clientY); } catch {} };
  addEventListener('mousemove', move, true);
  addEventListener('pointermove', move, true);
  addEventListener('mousedown', (e) => { const r = document.createElement('div'); r.className = '__rip';
    r.style.left = e.clientX + 'px'; r.style.top = e.clientY + 'px'; document.documentElement.appendChild(r);
    setTimeout(() => r.remove(), 600); }, true);
  if (document.readyState === 'loading') addEventListener('DOMContentLoaded', add); else add();
})();`;

export async function start(name, opts = {}) {
  const dir = new URL(`./out/${name}/rec/`, import.meta.url).pathname;
  rmSync(dir, { recursive: true, force: true });
  mkdirSync(dir + 'frames', { recursive: true });
  // CHROMIUM picks a browser by hand; otherwise Playwright's own.
  const browser = await chromium.launch({ executablePath: process.env.CHROMIUM || undefined });
  const ctx = await browser.newContext({ viewport: { width: 1280, height: 720 }, deviceScaleFactor: 1.5, acceptDownloads: true,
    timezoneId: opts.timezoneId || 'Europe/Madrid', locale: 'en-GB' });
  await ctx.addInitScript(CURSOR);
  const page = await ctx.newPage();
  page.on('pageerror', (e) => console.log('pageerror', e.message));
  const frames = [];
  let cdp, n = 0;
  const mouse = { x: 640, y: 360 };
  const Z = 1280 / 1920;
  const rec = {
    page, dir, marks: [], cuts: [],
    async record() {
      cdp = await ctx.newCDPSession(page);
      cdp.on('Page.screencastFrame', async (f) => {
        const file = `frames/${String(n++).padStart(6, '0')}.jpg`;
        writeFileSync(dir + file, Buffer.from(f.data, 'base64'));
        frames.push({ t: f.metadata.timestamp, file });
        cdp.send('Page.screencastFrameAck', { sessionId: f.sessionId }).catch(() => {});
      });
      await cdp.send('Page.startScreencast', { format: 'jpeg', quality: 93, maxWidth: 1920, maxHeight: 1080, everyNthFrame: 1 });
      this.t0 = Date.now() / 1000;
    },
    // A scene starts now: its narration clip plays from this moment.
    scene(id) { this.marks.push({ id, t: Date.now() / 1000 }); console.log('scene', id); },
    // Drop the time between cutFrom() and cutTo() from the video (long waits).
    cutFrom() { this._cut = Date.now() / 1000; },
    cutTo() { const c = [this._cut - 0.25, Date.now() / 1000 - 0.05]; if (c[1] > c[0]) this.cuts.push(c); },
    // Wait out something slow (a render) and drop it from the video.
    async skip(ms, until) { this.cutFrom(); if (until) await until.catch(() => {}); await sleep(ms); this.cutTo(); },
    // Wait until `secs` after the scene began (to let the narration finish).
    // Wait until the scene has run `secs` of *output* time (cuts don't count).
    async hold(secs) { const m = this.marks.at(-1);
      const cut = this.cuts.filter((c) => c[0] >= m.t).reduce((a, c) => a + c[1] - c[0], 0);
      const left = m.t + cut + secs - Date.now() / 1000; if (left > 0) await sleep(left * 1000); },
    async moveTo(x, y, ms = 700) {
      const steps = Math.max(8, Math.round(ms / 16)), x0 = mouse.x, y0 = mouse.y;
      for (let i = 1; i <= steps; i++) {
        const k = i / steps, e = k < 0.5 ? 4 * k * k * k : 1 - Math.pow(-2 * k + 2, 3) / 2;
        await page.mouse.move(x0 + (x - x0) * e, y0 + (y - y0) * e);
        await sleep(ms / steps / 2);
      }
      mouse.x = x; mouse.y = y;
    },
    // Move to a point given in 1920×1080 terms.
    go(x, y, ms) { return this.moveTo(x * Z, y * Z, ms); },
    async box(loc) { await loc.scrollIntoViewIfNeeded(); return loc.boundingBox(); },
    async hover(loc, ms) { const b = await this.box(loc); await this.moveTo(b.x + b.width / 2, b.y + b.height / 2, ms); return b; },
    async click(loc, ms) { await this.hover(loc, ms); await sleep(150); await page.mouse.down(); await sleep(90); await page.mouse.up(); await sleep(250); },
    // Drag a range input's thumb to a fraction (0..1) of its track.
    async slide(loc, to, ms = 1100) {
      const b = await this.box(loc);
      const v = await loc.evaluate((e) => (e.value - e.min) / (e.max - e.min));
      const y = b.y + b.height / 2, x = (f) => b.x + 8 + (b.width - 16) * f;
      await this.moveTo(x(v), y, 600); await sleep(120); await page.mouse.down();
      const steps = Math.round(ms / 25);
      for (let i = 1; i <= steps; i++) { const k = i / steps, e = k < 0.5 ? 2 * k * k : 1 - Math.pow(-2 * k + 2, 2) / 2;
        const xx = x(v + (to - v) * e); await page.mouse.move(xx, y); mouse.x = xx; await sleep(25); }
      await sleep(100); await page.mouse.up();
    },
    async drag(x1, y1, x2, y2, ms = 1200) {
      await this.moveTo(x1, y1, 600); await sleep(100); await page.mouse.down();
      const steps = Math.round(ms / 25);
      for (let i = 1; i <= steps; i++) { const k = i / steps, e = k < 0.5 ? 2 * k * k : 1 - Math.pow(-2 * k + 2, 2) / 2;
        const xx = x1 + (x2 - x1) * e, yy = y1 + (y2 - y1) * e; await page.mouse.move(xx, yy); mouse.x = xx; mouse.y = yy; await sleep(25); }
      await sleep(100); await page.mouse.up();
    },
    async wheel(dy, ms = 900) { dy *= Z; const steps = Math.round(ms / 30); for (let i = 0; i < steps; i++) { await page.mouse.wheel(0, dy / steps); await sleep(30); } },
    // The picture is drawn: no busy viewer, and every visible image in it loaded.
    ready(timeout = 90000) {
      return page.waitForFunction(() => {
        const v = [...document.querySelectorAll('.viewer, .compare')].filter((e) => e.offsetParent);
        if (!v.length) return false;
        if (v.some((e) => e.classList.contains('busy') || e.classList.contains('empty'))) return false;
        const imgs = v.flatMap((e) => [...e.querySelectorAll('img')]).filter((i) => i.offsetParent && i.getAttribute('src'));
        return imgs.length > 0 && imgs.every((i) => i.complete && i.naturalWidth > 0);
      }, null, { timeout, polling: 100 });
    },
    sleep,
    async stop() {
      await sleep(600);
      await cdp.send('Page.stopScreencast');
      await sleep(300);
      writeFileSync(dir + 'timeline.json', JSON.stringify({ frames, marks: this.marks, cuts: this.cuts, end: Date.now() / 1000 }, null, 1));
      await browser.close();
      console.log('frames', frames.length);
    },
  };
  return rec;
}

export const durations = (name) => JSON.parse(readFileSync(new URL(`./out/${name}/voice.json`, import.meta.url)));
