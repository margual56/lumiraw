/** The lens chosen by hand for each camera and lens name, so the next
 *  photograph taken with them starts from it. */

import { readStored } from './stored.js';

const STORE = 'lumiraw.lenses';

/** What a photograph's camera and lens are known by, or '' without a camera. */
const keyOf = (info) => {
  const camera = info?.profile?.camera;
  return camera ? `${camera}|${info.profile.lens ?? ''}` : '';
};

/** The choice made before for this camera and lens, if it can still be made. */
export function rememberedLens(info) {
  const key = keyOf(info);
  const choice = key ? readStored(STORE)[key] : undefined;
  if (choice === 'camera') return info.lens_camera ? choice : '';
  return info?.lens_choices?.some((c) => c.model === choice) ? choice : '';
}

/** Keep a choice for this camera and lens; '' forgets it. */
export function rememberLens(info, choice) {
  const key = keyOf(info);
  if (!key) return;
  const all = readStored(STORE);
  if (choice) all[key] = choice;
  else delete all[key];
  try {
    localStorage.setItem(STORE, JSON.stringify(all));
  } catch {
    // Private windows may refuse; the choice still holds for this photograph.
  }
}
