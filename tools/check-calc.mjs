/** Do the calculators agree with the published tables and almanacs? */
import {
  SHUTTERS, cameraName, depthOfField, starTrail, duration, lightOfDay, ndShutter, starRules, sunPosition, sunTimes,
} from '../web/src/lib/calc.js';
import { LAYERS, STEP, landAt, skyline } from '../web/src/lib/horizon.js';

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

if (failures) {
  console.error(`\n${failures} check(s) failed`);
  process.exit(1);
}
