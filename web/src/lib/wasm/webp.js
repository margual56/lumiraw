/** WebP container surgery. */

const ascii = (text) => Uint8Array.from(text, (c) => c.charCodeAt(0));

/** Put an EXIF block into a WebP the canvas just produced. */
export async function withExif(blob, exifBytes, width, height) {
  const bytes = new Uint8Array(await blob.arrayBuffer());
  const tag = (at) => String.fromCharCode(...bytes.subarray(at, at + 4));
  if (bytes.length < 16 || tag(0) !== 'RIFF' || tag(8) !== 'WEBP') return blob;

  // Chunks are size-prefixed and padded to an even length.
  const chunks = [];
  for (let at = 12; at + 8 <= bytes.length; ) {
    const name = tag(at);
    const size = new DataView(bytes.buffer, bytes.byteOffset + at + 4, 4).getUint32(0, true);
    chunks.push({ name, body: bytes.subarray(at + 8, at + 8 + size) });
    at += 8 + size + (size % 2);
  }
  if (!chunks.length || chunks.some((c) => c.name === 'EXIF')) return blob;

  const vp8x = new Uint8Array(10);
  const existing = chunks.find((c) => c.name === 'VP8X');
  if (existing) vp8x.set(existing.body.subarray(0, 10));
  vp8x[0] |= 0x08; // the bit that says an EXIF chunk is present
  const put24 = (at, v) => {
    vp8x[at] = v & 0xff;
    vp8x[at + 1] = (v >> 8) & 0xff;
    vp8x[at + 2] = (v >> 16) & 0xff;
  };
  put24(4, width - 1);
  put24(7, height - 1);

  const out = [{ name: 'VP8X', body: vp8x }];
  for (const chunk of chunks) {
    if (chunk.name !== 'VP8X') out.push(chunk);
  }
  out.push({ name: 'EXIF', body: exifBytes });

  const parts = [];
  let payload = 4; // the "WEBP" tag itself counts toward the RIFF size
  for (const { name, body } of out) {
    const header = new Uint8Array(8);
    header.set(ascii(name));
    new DataView(header.buffer).setUint32(4, body.length, true);
    parts.push(header, body);
    payload += 8 + body.length;
    if (body.length % 2) {
      parts.push(new Uint8Array(1)); // pad to an even boundary
      payload += 1;
    }
  }
  const riff = new Uint8Array(12);
  riff.set(ascii('RIFF'));
  new DataView(riff.buffer).setUint32(4, payload, true);
  riff.set(ascii('WEBP'), 8);
  return new Blob([riff, ...parts], { type: 'image/webp' });
}
