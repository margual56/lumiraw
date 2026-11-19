/** The raw files the videos open: our own photos, kept out of the repository. */
import { join } from 'node:path';

const DIR = process.env.DEMO_RAWS || join(process.env.HOME, 'Pictures/lumiraw-demo');
const at = (name) => join(DIR, name);

/** The developer: a cat, underexposed by almost two stops. */
export const DEVELOP = at('DSC05465.arw');

/** A shoot to cull, with DSC05596-97 as its burst. No people in any of them, and nothing that shows where we live. */
export const SHOOT = ['0661235956.arw', '2167907073.arw', 'DSC05104.arw', 'DSC05407.arw', 'DSC05465.arw',
  'DSC05471.ARW', 'DSC05476.ARW', 'DSC05480.ARW', 'DSC05486.ARW', 'DSC05488.ARW', 'DSC05498.ARW', 'DSC05514.ARW',
  'DSC05578.ARW', 'DSC05596.ARW', 'DSC05597.ARW', 'DSC05631.ARW', 'DSC05655.ARW', 'DSC05663.ARW'].map(at);

/** A hand-held bracket of the coast, a little over three stops apart. */
export const BRACKET = ['DSC05824.ARW', 'DSC05825.ARW'].map(at);
