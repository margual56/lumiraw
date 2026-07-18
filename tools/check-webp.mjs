/** Does the WebP export still decode, and does it carry the metadata? */
import { stamp } from '../web/src/lib/wasm/stamp.js';
import { withExif } from '../web/src/lib/wasm/webp.js';

const ascii = (text) => Uint8Array.from(text, (c) => c.charCodeAt(0));

/** A minimal simple-format WebP: RIFF, WEBP, one bitstream chunk. */
function simpleWebp(payload) {
  const body = ascii(payload);
  const out = new Uint8Array(12 + 8 + body.length);
  const view = new DataView(out.buffer);
  out.set(ascii('RIFF'));
  view.setUint32(4, 4 + 8 + body.length, true);
  out.set(ascii('WEBP'), 8);
  out.set(ascii('VP8 '), 12);
  view.setUint32(16, body.length, true);
  out.set(body, 20);
  return new Blob([out], { type: 'image/webp' });
}

function chunks(bytes) {
  const found = [];
  const name = (at) => String.fromCharCode(...bytes.subarray(at, at + 4));
  for (let at = 12; at + 8 <= bytes.length; ) {
    const size = new DataView(bytes.buffer, bytes.byteOffset + at + 4, 4).getUint32(0, true);
    found.push({ name: name(at), body: bytes.subarray(at + 8, at + 8 + size) });
    at += 8 + size + (size % 2);
  }
  return found;
}

let failures = 0;
const check = (what, ok) => {
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${what}`);
  if (!ok) failures += 1;
};

const exif = ascii('II*\0EXIFPAYLOAD');
const out = await withExif(simpleWebp('bitstream-goes-here'), exif, 4000, 3000);
const bytes = new Uint8Array(await out.arrayBuffer());
const parts = chunks(bytes);
const named = (n) => parts.find((c) => c.name === n);

check('still a RIFF/WEBP file', String.fromCharCode(...bytes.subarray(0, 4)) === 'RIFF'
  && String.fromCharCode(...bytes.subarray(8, 12)) === 'WEBP');
check('the RIFF size matches the payload', new DataView(bytes.buffer).getUint32(4, true)
  === bytes.length - 8);
check('a VP8X chunk was added first', parts[0]?.name === 'VP8X' && parts[0].body.length === 10);
check('the EXIF flag is set', ((named('VP8X')?.body[0] ?? 0) & 0x08) !== 0);
check('the canvas size is the picture size', (() => {
  const b = named('VP8X').body;
  const at = (i) => b[i] | (b[i + 1] << 8) | (b[i + 2] << 16);
  return at(4) === 3999 && at(7) === 2999;
})());
check('the original bitstream survived untouched',
  String.fromCharCode(...(named('VP8 ')?.body ?? [])) === 'bitstream-goes-here');
check('the EXIF chunk is there, byte for byte',
  String.fromCharCode(...(named('EXIF')?.body ?? [])) === String.fromCharCode(...exif));

// Odd-length payloads are the classic way to corrupt a RIFF file.
const odd = await withExif(simpleWebp('odd'), ascii('12345'), 2, 2);
const oddBytes = new Uint8Array(await odd.arrayBuffer());
check('odd-sized chunks stay word aligned',
  new DataView(oddBytes.buffer).getUint32(4, true) === oddBytes.length - 8
  && chunks(oddBytes).length === 3);

// Running it twice must not bolt on a second EXIF chunk.
const again = await withExif(out, exif, 4000, 3000);
check('it refuses to add a second EXIF chunk',
  (await again.arrayBuffer()).byteLength === bytes.length);

// The export time the pipeline stamps into every file.
const when = stamp(new Date(2026, 8, 22, 16, 30, 5));
check('the timestamp is EXIF shaped', /^\d{4}:\d{2}:\d{2} \d{2}:\d{2}:\d{2}[+-]\d{2}:\d{2}$/.test(when));
check('it is local time, not UTC', when.startsWith('2026:09:22 16:30:05'));
check('single digits are padded', stamp(new Date(2026, 0, 2, 3, 4, 5)).startsWith('2026:01:02 03:04:05'));

process.exit(failures ? 1 : 0);
