/** Do the calculators agree with the published tables and almanacs? */
import {
  EXPOSURES, SHUTTERS, cameraName, depthOfField, duration, fieldOfView, frameOutline, lightOf, lightOfDay, ndShutter, nearestStep, starRules,
  skyFrom, starTrail, sunPosition, sunTimes,
} from '../web/src/lib/calc.js';
import { bestNights, coreBest, darkSpan, faintest, moonCoords, moonPhase, nightOf, planets, skyBrightness, skyOfNight } from '../web/src/lib/night.js';
import { LAYERS, STEP, landAt, skyline } from '../web/src/lib/horizon.js';
import { decode, pickSpots, zoneOf } from '../web/src/lib/darksky.js';
import { readLink, writeLink } from '../web/src/lib/spotlink.js';

let failures = 0;
const check = (what, ok) => {
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${what}`);
  if (!ok) failures += 1;
};
const near = (a, b, tolerance) => Math.abs(a - b) <= tolerance;

// 50 mm at f/8 focused at 5 m on full frame: 3.39 to 9.53 m, hyperfocal 10.5 m,
// as DOFMaster's tables give it with a 0.03 mm circle.
const dof = depthOfField({ focal: 50, aperture: 8, distance: 5, crop: 1 });
check(`depth of field, 50 mm f/8 at 5 m (${dof.near.toFixed(2)}–${dof.far.toFixed(2)} m)`,
  near(dof.near, 3.39, 0.01) && near(dof.far, 9.53, 0.01) && near(dof.hyperfocal, 10.47, 0.01));
const past = depthOfField({ focal: 24, aperture: 11, distance: 3, crop: 1 });
check('focused past the hyperfocal distance, sharp to infinity', past.far === Infinity);
const atHyper = depthOfField({ focal: 24, aperture: 11, distance: past.hyperfocal, crop: 1 });
check('focused at the hyperfocal distance, sharp from half of it',
  near(atHyper.near, past.hyperfocal / 2, 0.02) && atHyper.far === Infinity);
const crop = depthOfField({ focal: 50, aperture: 8, distance: 5, crop: 1.5 });
check('the same lens on a smaller sensor has less depth of field', crop.total < dof.total);

check('ten stops on 1/60 s is 17 s', near(ndShutter(1 / 60, 10), 17.07, 0.01));
check('no filter, no change', ndShutter(1 / 125, 0) === 1 / 125);
check('every shutter speed on the list reads differently',
  new Set(SHUTTERS.map(duration)).size === SHUTTERS.length);

const stars = starRules({ focal: 14, crop: 1, aperture: 2.8, megapixels: 24 });
check(`NPF for 14 mm f/2.8 on 24 MP full frame (${stars.npf.toFixed(1)} s, 6 µm)`,
  near(stars.pitch, 6, 0.01) && near(stars.npf, 19.86, 0.01));
check('the 500 rule for 14 mm', near(stars.rule500, 35.71, 0.01));
// 14 mm for 20 s on 6 µm pixels: the sky turns 15" a second, 0.0204 mm on
// the sensor, 3.4 pixels; nothing at the pole.
check('a star trails 3.4 pixels in 20 s at 14 mm on the equator',
  near(starTrail({ focal: 14, seconds: 20, pitch: 6 }), 3.40, 0.02)
  && near(starTrail({ focal: 14, seconds: 20, pitch: 6, declination: 90 }), 0, 1e-9));
check('exposures snap to the nearest step by ratio, 25 s to 25 s and 35 s to 40 s',
  EXPOSURES[nearestStep(EXPOSURES, 25)] === 25 && EXPOSURES[nearestStep(EXPOSURES, 35.7)] === 40);
check('the 500 rule shortens with the crop',
  near(starRules({ focal: 14, crop: 1.5, aperture: 2.8, megapixels: 24 }).rule500, 23.81, 0.01));

// Madrid on the June solstice of 2024: sunrise 04:44 and sunset 19:48 UTC,
// as the almanacs have it.
const hm = (date) => date.getUTCHours() * 60 + date.getUTCMinutes() + date.getUTCSeconds() / 60;
const madrid = sunTimes(new Date(2024, 5, 21), 40.4168, -3.7038);
check(`sunrise in Madrid (${madrid.horizon.rise.toISOString()})`, near(hm(madrid.horizon.rise), 4 * 60 + 44, 3));
check(`sunset in Madrid (${madrid.horizon.set.toISOString()})`, near(hm(madrid.horizon.set), 19 * 60 + 48, 3));
// Sydney on the December solstice: sunrise 05:41 AEDT (18:41 UTC the day
// before) and sunset 20:05 AEDT (09:05 UTC).
const sydney = sunTimes(new Date(2024, 11, 21), -33.8688, 151.2093);
check(`sunset in Sydney (${sydney.horizon.set.toISOString()})`, near(hm(sydney.horizon.set), 9 * 60 + 5, 3));

const tromso = lightOfDay(new Date(2024, 5, 21), 69.65, 18.96);
check('the midnight sun never sets', tromso.sunset === null && tromso.eveningBlue === null);
const day = lightOfDay(new Date(2024, 9, 15), 51.5, -0.12);
check('the day runs blue, golden, noon, golden, blue',
  day.morningBlue.to <= day.morningGolden.from && day.morningGolden.to < day.noon
  && day.noon < day.eveningGolden.from && day.eveningGolden.to <= day.eveningBlue.from);

// The same day in Madrid: at sunset the sun sets at 302° (NOAA's calculator),
// its centre a quarter of a degree under the horizon as seen, and at noon it stands 73° up, 90° less the latitude and the tilt.
const dusk = sunPosition(madrid.horizon.set, 40.4168, -3.7038);
check(`the sun sets in Madrid at ${dusk.bearing.toFixed(1)}°, on the horizon`,
  near(dusk.bearing, 302.1, 1) && near(dusk.altitude, -0.27, 0.3));
const high = sunPosition(madrid.noon, 40.4168, -3.7038);
check('at sunset the sun is where the times put it, and the light turns golden to blue',
  near(dusk.geometric, -0.833, 0.2) && lightOf(3) === 'golden' && lightOf(-2) === 'dusk'
  && lightOf(-5) === 'blue' && lightOf(-7) === null && lightOf(20) === 'day');
check(`noon in Madrid, ${high.altitude.toFixed(1)}° up, due south`,
  near(high.altitude, 90 - 40.4168 + 23.44, 0.3) && near(high.bearing, 180, 1));

// A ridge 1000 m high 10 km to the west of a spot at sea level: 5.6° up that
// way, the earth's curve (7 m at that distance, the air's bend counted) taken
// off, and nothing to the east; within 5 km it is not there yet.
const west = (la, lo) => (lo < -0.09 && lo > -0.2 ? 1000 : 0);
const ridge = skyline(west, 0, 0);
const angle = Math.atan2(1000 - 1.7 - 6.8, 10008) * 180 / Math.PI;
check(`a ridge 10 km west stands ${landAt(ridge.layers.at(-1), 270).toFixed(2)}° up`,
  near(landAt(ridge.layers.at(-1), 270), angle, 0.4) && landAt(ridge.layers.at(-1), 90) < 0
  && landAt(ridge.layers[LAYERS.indexOf(5000)], 270) < 0 && ridge.layers[0].length === 360 / STEP);

// The light pollution atlas: a tile of one value, 100 (the brightness ratio
// 0.15), rising by one a point east along the bottom row.
const flat = new Int8Array(600 * 600 + 1);
flat[1] = 100;
for (let col = 1; col < 600; col++) flat[1 + col] = 1;
const ratios = decode(flat);
check('an atlas tile decodes as its own page reads it',
  near(ratios[0], (5 / 195) * (Math.exp(1.95) - 1), 1e-6) && near(ratios[599], (5 / 195) * (Math.exp(0.0195 * 699) - 1), 1e-3)
  && ratios[600] === ratios[0]);
check('a natural sky is 22 mag/arcsec², Bortle 1; a city\'s white, Bortle 9',
  near(zoneOf(0).sqm, 22, 1e-9) && zoneOf(0).bortle === 1 && zoneOf(50).zone === '7b' && zoneOf(2).bortle === 5);
const spots = pickSpots([
  { lat: 40, lon: -3, ratio: 0.05, km: 30 }, { lat: 40.01, lon: -3, ratio: 0.05, km: 29 },
  { lat: 40.5, lon: -3, ratio: 0.05, km: 20 }, { lat: 41, lon: -3, ratio: 3, km: 5 },
], { count: 3, apart: 5 });
check('dark spots: the darkest first, the nearest of equals, none on top of another',
  spots.length === 3 && spots[0].km === 20 && spots[1].km === 29 && spots[2].ratio === 3);

check('cameras go by the names on them, with lensfun\'s codes after',
  cameraName('Sony', 'ILCE-7M3') === 'Sony A7 III (ILCE-7M3)'
  && cameraName('Sony', 'ILCE-7RM4A') === 'Sony A7R IVA (ILCE-7RM4A)'
  && cameraName('Sony', 'ILCE-6400') === 'Sony A6400 (ILCE-6400)'
  && cameraName('Sony', 'DSC-RX100M5A') === 'Sony RX100 VA (DSC-RX100M5A)'
  && cameraName('Canon', 'Canon EOS R6m2') === 'Canon EOS R6 Mark II (EOS R6m2)'
  && cameraName('Canon', 'Canon EOS R5') === 'Canon EOS R5'
  && cameraName('Nikon Corporation', 'Z 6_2') === 'Nikon Z 6II (Z 6_2)'
  && cameraName('Panasonic', 'DC-GH5M2') === 'Panasonic Lumix GH5 II (DC-GH5M2)'
  && cameraName('Panasonic', 'DMC-GM1') === 'Panasonic Lumix GM1 (DMC-GM1)'
  && cameraName('Olympus Corporation', 'E-M1MarkII') === 'Olympus OM-D E-M1 Mark II (E-M1MarkII)'
  && cameraName('OM Digital Solutions', 'OM-1') === 'OM System OM-1');

check('durations read as a photographer writes them',
  duration(1 / 250) === '1/250 s' && duration(1 / 4) === '1/4 s' && duration(0.4) === '0.4 s'
  && duration(0.8) === '0.8 s' && duration(2.5) === '2.5 s' && duration(256) === '4 min 16 s'
  && duration(4080) === '1 h 8 min' && duration(17.07) === '17 s');

// 2 November 2026 at 22:00 UTC, as JPL's Horizons gives it (astrometric,
// from the earth's centre): the moon 38 % lit and waning, and the planets.
const night = new Date('2026-11-02T22:00:00Z');
const moon = moonCoords(night);
const phase = moonPhase(night);
check(`the moon (${moon.ra.toFixed(2)}°, ${moon.dec.toFixed(2)}°, ${Math.round(phase.lit * 100)} % lit)`,
  near(moon.ra, 146.11, 1) && near(moon.dec, 13.86, 1) && near(phase.lit, 0.384, 0.03) && !phase.waxing);
const HORIZONS = {
  mercury: [221.24, -16.94, 5.01], venus: [201.49, -13.76, -4.31], mars: [142.16, 16.62, 0.82],
  jupiter: [146.72, 14.12, -2.01], saturn: [9.13, 1.02, 0.47],
};
check('the planets, to a fifth of a degree and their brightness to half a magnitude',
  planets(night).every((p) => {
    const [ra, dec, mag] = HORIZONS[p.name];
    return near(p.ra, ra, 0.2) && near(p.dec, dec, 0.2) && near(p.mag, mag, 0.5);
  }));
const polaris = skyFrom(night, 40.4, -3.7)(37.95, 89.26);
check(`the pole star stands as high as the place is north (${polaris.altitude.toFixed(2)}°)`,
  near(polaris.altitude, 40.4, 1) && (polaris.bearing < 2 || polaris.bearing > 358));
check('a natural sky shows stars to 6.6, a town\'s to 4',
  near(faintest(skyBrightness({ sun: -30, moon: -10, lit: 1 })), 6.62, 0.05) && near(faintest(18), 3.97, 0.05));
check('twilight and the moon brighten the sky',
  skyBrightness({ sun: -12, moon: -10, lit: 0 }) < 19 && skyBrightness({ sun: -30, moon: 60, lit: 1 }) < 19.5
  && skyBrightness({ sun: -30, moon: 60, lit: 0.1 }) > 21.9);

const settings = { lat: 40.417, lon: -3.704, day: '2026-11-02', minutes: 1605, facing: 243, around: true, dome: false };
const code = writeLink(settings);
const linked = readLink(code);
check(`a shared code is short and gives back all it holds (${code})`, code.length === 16
  && linked.tool === 'sun' && Object.entries(settings).every(([key, value]) => linked[key] === value));
check('a code for the far corners of the map survives',
  readLink(writeLink({ ...settings, lat: -90, lon: 180, facing: 359.7, minutes: 0 })).lat === -90 && readLink(writeLink({ ...settings, lon: 180 })).lon === 180);
check('an old link, the tab alone, still opens the tab', readLink('#dark').tool === 'dark' && readLink('#dark').lat === undefined);
check('a damaged code asks for no place', readLink(`#1${'z'.repeat(14)}`).lat === undefined);

const winter = bestNights({ lat: 40.4, lon: -3.7, first: '2026-12-01', count: 31 });
const newMoon = winter.find((night) => night.day === '2026-12-08');
const fullMoon = winter.find((night) => night.day === '2026-12-23');
check(`a new moon's night has more moonless dark than a full moon's (${newMoon.moonless} h, ${fullMoon.moonless} h)`,
  newMoon.moonless > 10 && fullMoon.moonless < 2 && newMoon.score > fullMoon.score);
const summer = bestNights({ lat: 40.4, lon: -3.7, first: '2026-08-10', count: 1 })[0];
check(`the core stands high on a summer evening (${summer.core?.altitude.toFixed(0)}°)`, summer.core.altitude > 15 && summer.core.bearing > 150 && summer.core.bearing < 240);
const cloudy = bestNights({ lat: 40.4, lon: -3.7, first: '2026-12-08', count: 1, cloudAt: () => 80 })[0];
check('forecast cloud lowers a night', cloudy.cloud === 80 && near(cloudy.score, newMoon.score * 0.2, 0.3));
check('no night is fully dark in a Tromsø June', bestNights({ lat: 69.6, lon: 18.9, first: '2026-06-21', count: 1 })[0].from === null);

const town = skyOfNight({ lat: 40.4, lon: -3.7, day: '2026-12-08', minutes: 720, ratio: 20 });
const wild = skyOfNight({ lat: 40.4, lon: -3.7, day: '2026-12-08', minutes: 720, ratio: 0 });
check(`from noon, a sky taken at its darkest; a town shows fewer stars (${town.limit.toFixed(1)}, ${wild.limit.toFixed(1)})`,
  town.minutes > 1200 && wild.limit > 6 && town.limit < wild.limit - 1);

const sameNight = bestNights({ lat: 40.4, lon: -3.7, first: '2026-08-10', count: 1 })[0];
const samples = nightOf({ lat: 40.4, lon: -3.7, day: '2026-08-10' });
check('the calendar says what the night\'s own facts say',
  sameNight.from === darkSpan(samples).from && sameNight.core.altitude === coreBest(samples).altitude);

const twenty = fieldOfView({ focal: 20, crop: 1 });
check(`a 20 mm lens on full frame takes in 84° by 62° (${twenty.width.toFixed(1)}°, ${twenty.height.toFixed(1)}°)`,
  near(twenty.width, 84, 0.1) && near(twenty.height, 61.9, 0.1));
const level = frameOutline({ facing: 180, tilt: 0, ...twenty });
const middleTop = level[8];
check('a level frame looks where it is told, centred, half its height up',
  near(middleTop.bearing, 180, 0.01) && near(middleTop.altitude, twenty.height / 2, 0.01));
const firstCorner = level[0];
check('straight edges bow on the sky: a corner stands lower than the top\'s middle',
  firstCorner.altitude < middleTop.altitude && near(firstCorner.bearing, 180 - twenty.width / 2, 0.01));
check('the outline closes', level[0].bearing === level.at(-1).bearing && level[0].altitude === level.at(-1).altitude);

if (failures) {
  console.error(`\n${failures} check(s) failed`);
  process.exit(1);
}
