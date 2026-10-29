/** Do the calculators agree with the published tables and almanacs? */
import {
  SHUTTERS, depthOfField, duration, lightOfDay, ndShutter, starRules, sunTimes,
} from '../web/src/lib/calc.js';

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

check('durations read as a photographer writes them',
  duration(1 / 250) === '1/250 s' && duration(1 / 4) === '1/4 s' && duration(0.4) === '0.4 s'
  && duration(0.8) === '0.8 s' && duration(2.5) === '2.5 s' && duration(256) === '4 min 16 s'
  && duration(4080) === '1 h 8 min' && duration(17.07) === '17 s');

if (failures) {
  console.error(`\n${failures} check(s) failed`);
  process.exit(1);
}
