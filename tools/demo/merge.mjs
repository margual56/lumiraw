import { start, BASE, durations } from './lib.mjs';
import { basename } from 'node:path';
import { BRACKET } from './raws.mjs';
const V = durations('merge');
const CARD = new URL('./card.html', import.meta.url).href;
const r = await start('merge');
const p = r.page;
const btn = (name) => p.getByRole('button', { name, exact: true });
const mode = (name) => p.locator('button', { hasText: name }).first();

await p.goto(CARD + '?title=Merging and stacking&sub=Several frames, one picture');
await r.record();
r.scene('title');
await r.sleep(2800);

await p.goto(BASE + '/merge.html');
await p.waitForTimeout(600);
r.scene('intro');
await r.go(760, 300, 1400);
await r.hold(V.intro + 0.3);

r.scene('modes');
const w = V.modes / 5.6;
await r.hover(mode('Exposure merge'), 700); await r.sleep(w * 1000 - 700);
for (const m of ['Average', 'Median', 'Lighten', 'Focus stack']) { await r.click(mode(m), 700); await r.sleep(w * 1000 - 1100); }
await r.click(mode('Exposure merge'), 700);
await r.hold(V.modes + 0.4);

r.scene('drop');
await r.click(p.getByText('Drop the frames', { exact: false }).first()).catch(() => {});
await p.locator('input[type=file]').first().setInputFiles(BRACKET);
await r.skip(500, btn('Merge these frames').waitFor({ timeout: 60000 }));
await r.hover(p.getByText(basename(BRACKET[0])).first(), 900);
await r.sleep(1200);
await r.hover(p.getByText(basename(BRACKET[1])).first(), 700);
await r.hold(V.drop + 0.4);

r.scene('merge');
await r.hover(p.getByText('Line the frames up').first(), 800);
await r.sleep(900);
await r.hover(p.getByText('Remove ghosts').first(), 700);
await r.sleep(700);
await r.click(btn('Merge these frames'));
await r.sleep(1500);
await r.hold(V.merge + 0.2);
await r.skip(600, btn('Develop this picture').waitFor({ timeout: 180000 }));

r.scene('result');
await r.hover(p.getByText('Merged', { exact: true }).first(), 700);
await r.sleep(1200);
await r.click(btn('Develop this picture'));
await r.skip(800, (async () => { await r.sleep(600); await r.ready(); })());
await r.go(800, 520, 1400);
await r.hold(V.result + 1.2);

r.scene('outro');
await p.goto(CARD + '?sub=Raw photos, developed in your browser&url=1');
await r.hold(V.outro + 1.8);
await r.stop();
