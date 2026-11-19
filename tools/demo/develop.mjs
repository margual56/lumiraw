import { start, BASE, durations } from './lib.mjs';
import { DEVELOP } from './raws.mjs';
const V = durations('develop');
const CARD = new URL('./card.html', import.meta.url).href;
const r = await start('develop');
const p = r.page;
const btn = (name) => p.getByRole('button', { name, exact: true });
const step = (n) => p.getByRole('button', { name: new RegExp('^' + n + '\\. ') });
const cont = async () => { await r.click(btn('Continue')); await r.skip(700, (async () => { await r.sleep(400); await r.ready(); })()); };

await p.goto(CARD + '?title=Developing a raw photo&sub=Step by step, in your browser');
await r.record();
r.scene('title');
await r.sleep(2800);

await p.goto(BASE + '/');
await p.waitForTimeout(600);
r.scene('intro');
await r.go(960, 640, 1200);
await r.go(1000, 380, 1600);
await r.hold(V.intro + 0.3);

r.scene('open');
const drop = p.getByText('Drop raw files here', { exact: false }).first();
await r.click(drop).catch(() => {});
await p.locator('input[type=file]').first().setInputFiles(DEVELOP);
await r.skip(700, (async () => { await btn('Reset framing').waitFor({ timeout: 60000 }); await r.ready(); })());
await r.go(900, 520, 1500);
await r.hold(V.open + 0.4);

r.scene('framing');
const tilt = p.locator('input[type=range]:visible').first();
await r.slide(tilt, 0.56, 1000);
await r.sleep(800);
await r.slide(tilt, 0.5, 700);
const ratio = p.locator('select:visible').first();
await r.hover(ratio);
await r.sleep(600);
await r.go(1500, 700, 800);
await r.hold(V.framing + 0.5);

r.scene('brightness');
await cont();
const ranges = p.locator('input[type=range]:visible');
await r.slide(ranges.nth(1), 0.62, 1300);
await r.sleep(500);
await r.slide(ranges.nth(3), 0.42, 1100);
await r.hold(V.brightness + 0.4);

r.scene('wb');
await cont();
await r.slide(p.locator('input[type=range]:visible').nth(0), 0.57, 1300);
await r.hold(V.wb + 0.4);

r.scene('compare');
await r.click(step(7));
await r.skip(700, (async () => { await r.sleep(400); await r.ready(); })());
const cb = await p.locator('.compare:visible').first().boundingBox();
const midY = cb.y + cb.height / 2, at = (f) => cb.x + cb.width * f;
await r.drag(at(0.5), midY, at(0.18), midY, 1500);
await r.drag(at(0.18), midY, at(0.82), midY, 1800);
await r.drag(at(0.82), midY, at(0.5), midY, 1000);
const exp = p.getByRole('button', { name: /^Vignetting/ });
await r.click(exp);
await r.sleep(1500);
await r.click(exp);
await r.hold(V.compare + 0.4);

r.scene('grade');
await cont();
await r.click(p.locator('button', { hasText: 'Slide film' }).first());
await r.sleep(1800);
await r.click(p.locator('button', { hasText: 'Warm portrait' }).first());
await r.sleep(1500);
await r.click(p.getByText('Colour grading', { exact: true }).first());
await r.hold(V.grade + 0.6);

r.scene('download');
await r.click(btn('Download'));
await r.skip(700, (async () => { await r.sleep(400); await r.ready(); })());
const fmt = p.locator('select:visible').first();
await r.click(fmt);
await fmt.selectOption({ label: 'JPEG' });
await r.sleep(500);
const dl = p.waitForEvent('download', { timeout: 120000 }).catch(() => null);
await r.click(btn('Develop & download'));
await r.hold(V.download + 0.4);
await r.skip(500, dl);

r.scene('outro');
await p.goto(CARD + '?sub=Raw photos, developed in your browser&url=1');
await r.hold(V.outro + 1.8);
await r.stop();
