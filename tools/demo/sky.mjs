import { start, BASE, durations } from './lib.mjs';
const V = durations('sky');
const CARD = new URL('./card.html', import.meta.url).href;
const r = await start('sky', { timezoneId: 'Europe/Madrid' });
const p = r.page;
const tab = (name) => p.locator('button', { hasText: name }).first();

await p.goto(CARD + '?title=Planning the sky&sub=Sun, moon, stars and clouds');
await r.record();
r.scene('title');
await r.sleep(2800);

await p.goto(BASE + '/sky.html');
await p.waitForTimeout(1500);
r.scene('intro');
await r.hover(tab('Golden hour'), 1000); await r.sleep(500);
await r.hover(tab('Dark skies'), 700); await r.sleep(500);
await r.hover(tab('Sharp stars'), 700);
await r.hold(V.intro + 0.3);

r.scene('spot');
const lat = p.locator('input[type=number]').nth(0), lon = p.locator('input[type=number]').nth(1);
await r.click(lat, 900);
await p.keyboard.type('40.042', { delay: 90 });
await r.click(lon, 600);
await p.keyboard.type('-1.016', { delay: 90 });
await p.keyboard.press('Enter');
await r.sleep(2500);
await r.go(960, 680, 1200);
await r.hold(V.spot + 0.4);

r.scene('golden');
await r.go(400, 620, 600);
await r.wheel(950, 1300);
await r.sleep(800);
const time = p.locator('input[type=range]:visible').first();
const v0 = await time.evaluate((e) => (e.value - e.min) / (e.max - e.min));
await r.slide(time, Math.max(0, v0 - 0.18), 2200);
await r.sleep(400);
await r.slide(time, Math.min(1, v0 + 0.06), 2200);
await r.hover(p.getByText('Golden hour', { exact: true }).last(), 800);
await r.hold(V.golden + 0.4);

r.scene('nights');
const nights = p.getByText('Best nights', { exact: true }).first();
await r.click(nights, 800);
await r.sleep(600);
await r.go(400, 620, 500);
await r.wheel(700, 1200);
await r.sleep(500);
await r.go(1000, 640, 1200);
await r.hold(V.nights + 0.6);

r.scene('dark');
await r.go(400, 500, 500);
await r.wheel(-6000, 800);
await r.click(tab('Dark skies'), 800);
await r.sleep(2500);
await r.wheel(420, 800);
const find = p.locator('button', { hasText: 'Find dark spots' }).first();
await r.click(find, 1000);
await r.sleep(2500);
await r.hold(V.dark + 0.6);

r.scene('stars');
await r.go(400, 500, 400);
await r.wheel(-6000, 700);
await r.click(tab('Sharp stars'), 800);
await r.sleep(1500);
await r.go(400, 600, 400);
await r.wheel(500, 900);
await r.sleep(400);
const exp = p.locator('input[type=range]:visible').first();
await r.slide(exp, 0.85, 1800);
await r.sleep(900);
await r.slide(exp, 0.35, 1600);
await r.hold(V.stars + 0.8);

r.scene('outro');
await p.goto(CARD + '?sub=Raw photos, developed in your browser&url=1');
await r.hold(V.outro + 1.8);
await r.stop();
