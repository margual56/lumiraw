import { start, BASE, durations } from './lib.mjs';
import { SHOOT } from './raws.mjs';
const V = durations('cull');
const CARD = new URL('./card.html', import.meta.url).href;
const r = await start('cull');
const p = r.page;
const btn = (name) => p.getByRole('button', { name, exact: true });

await p.goto(CARD + '?title=Culling a shoot&sub=Find the keepers, in your browser');
await r.record();
r.scene('title');
await r.sleep(2800);

await p.goto(BASE + '/cull.html');
await p.waitForTimeout(600);
r.scene('intro');
await r.go(700, 200, 1200);
await r.go(960, 300, 1400);
await r.hold(V.intro + 0.3);

r.scene('drop');
await r.click(p.getByText('Drop', { exact: false }).first()).catch(() => {});
await p.locator('input[type=file]').first().setInputFiles(SHOOT);
await r.sleep(1800);
const cards = p.locator('.gallery button.photo');
await r.skip(1500, p.waitForFunction((n) => {
  return !document.querySelector('section.status') && document.querySelectorAll('.gallery button.photo').length >= n;
}, 8, { timeout: 180000 }));
await r.go(900, 640, 1500);
await r.hold(V.drop + 0.4);

r.scene('results');
const n = await cards.count();
for (const i of [0, 3, Math.min(6, n - 1)]) { await r.hover(cards.nth(i), 900); await r.sleep(700); }
await r.wheel(400, 900);
await r.hold(V.results + 0.4);

r.scene('tune');
await r.wheel(-400, 700);
const ranges = p.locator('input[type=range]:visible');
await r.slide(ranges.nth(0), 0.25, 1300);
await r.sleep(500);
await r.slide(ranges.nth(0), 0.6, 900);
const minus = p.locator('.stepper button').nth(0);
await r.click(minus);
for (let i = 0; i < 3; i++) { await r.sleep(350); await p.mouse.down(); await r.sleep(80); await p.mouse.up(); }
await r.sleep(700);
await r.click(p.locator('.seg button').nth(1));
await r.sleep(700);
await r.click(p.locator('.seg button').nth(0));
await r.hold(V.tune + 0.4);

r.scene('pick');
const unpicked = p.locator('.gallery li:not(.picked) button.photo');
await r.click(unpicked.first(), 900);
await r.sleep(1200);
const picked = p.locator('.gallery li.picked button.photo');
await r.click(picked.nth(1), 900);
await r.hold(V.pick + 0.4);

r.scene('develop');
await r.click(btn('Develop the picks'));
await r.skip(800, (async () => { await r.sleep(600); await r.ready(); })());
await r.go(900, 500, 1200);
await r.hold(V.develop + 1.0);

r.scene('outro');
await p.goto(CARD + '?sub=Raw photos, developed in your browser&url=1');
await r.hold(V.outro + 1.8);
await r.stop();
