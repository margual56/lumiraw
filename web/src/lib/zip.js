/** A zip of finished pictures, dated properly. */

const encoder = new TextEncoder();

/**
 * EXIF spells a time `YYYY:MM:DD HH:MM:SS`, with no zone, meaning local time
 * where the photograph was taken.
 */
function fromExif(text) {
  const m = /^(\d{4}):(\d{2}):(\d{2})[ T](\d{2}):(\d{2}):(\d{2})/.exec(text ?? '');
  if (!m) return null;
  const [, y, mo, d, h, mi, sec] = m.map(Number);
  const when = new Date(y, mo - 1, d, h, mi, sec);
  return Number.isNaN(when.getTime()) ? null : when;
}

/** A date as MS-DOS packs it, which is what a zip entry carries. */
function dosTime(when) {
  const year = Math.max(1980, when.getFullYear());
  return {
    date: ((year - 1980) << 9) | ((when.getMonth() + 1) << 5) | when.getDate(),
    time: (when.getHours() << 11) | (when.getMinutes() << 5) | (when.getSeconds() >> 1),
  };
}

/** The extended timestamp and NTFS extra fields, which carry the times the DOS field cannot. */
function timeExtras(modified, created, { local }) {
  const unix = (when) => Math.floor(when.getTime() / 1000);
  // 0x5455, with bit 0 for modification and bit 2 for creation.
  const flags = created ? 0b101 : 0b001;
  const times = local && created ? [unix(modified), unix(created)] : [unix(modified)];
  const ut = new DataView(new ArrayBuffer(5 + times.length * 4));
  ut.setUint16(0, 0x5455, true);
  ut.setUint16(2, 1 + times.length * 4, true);
  ut.setUint8(4, flags);
  times.forEach((t, i) => ut.setInt32(5 + i * 4, t, true));

  // 0x000a, as Windows file times: 100 ns ticks since 1601.
  const filetime = (when) => (BigInt(when.getTime()) + 11644473600000n) * 10000n;
  const ntfs = new DataView(new ArrayBuffer(36));
  ntfs.setUint16(0, 0x000a, true);
  ntfs.setUint16(2, 32, true);
  ntfs.setUint16(8, 0x0001, true);
  ntfs.setUint16(10, 24, true);
  ntfs.setBigUint64(12, filetime(modified), true);
  ntfs.setBigUint64(20, filetime(modified), true);
  ntfs.setBigUint64(28, filetime(created ?? modified), true);

  const out = new Uint8Array(ut.buffer.byteLength + 36);
  out.set(new Uint8Array(ut.buffer), 0);
  out.set(new Uint8Array(ntfs.buffer), ut.buffer.byteLength);
  return out;
}

/**
 * Stored-only ZIP: the members are already compressed images, so deflating them
 * again would cost seconds and save nothing.
 */
export async function zip(files, now) {
  const parts = [];
  const central = [];
  let offset = 0;
  const table = crcTable();
  const modified = fromExif(now) ?? new Date();
  const stamp = dosTime(modified);
  for (const file of files) {
    const name = encoder.encode(file.name);
    const created = fromExif(file.captured);
    const extraLocal = timeExtras(modified, created, { local: true });
    const extraDir = timeExtras(modified, created, { local: false });
    const data = new Uint8Array(await file.blob.arrayBuffer());
    const crc = crc32(data, table);
    const local = new DataView(new ArrayBuffer(30));
    local.setUint32(0, 0x04034b50, true);
    local.setUint16(4, 20, true);
    local.setUint16(6, 0, true);
    local.setUint16(8, 0, true);          // stored
    local.setUint16(10, stamp.time, true);
    local.setUint16(12, stamp.date, true);
    local.setUint32(14, crc, true);
    local.setUint32(18, data.length, true);
    local.setUint32(22, data.length, true);
    local.setUint16(26, name.length, true);
    local.setUint16(28, extraLocal.length, true);
    parts.push(new Uint8Array(local.buffer), name, extraLocal, data);

    const dir = new DataView(new ArrayBuffer(46));
    dir.setUint32(0, 0x02014b50, true);
    dir.setUint16(4, 20, true);
    dir.setUint16(6, 20, true);
    dir.setUint16(10, 0, true);
    dir.setUint16(12, stamp.time, true);
    dir.setUint16(14, stamp.date, true);
    dir.setUint32(16, crc, true);
    dir.setUint32(20, data.length, true);
    dir.setUint32(24, data.length, true);
    dir.setUint16(28, name.length, true);
    dir.setUint16(30, extraDir.length, true);
    dir.setUint32(42, offset, true);
    central.push(new Uint8Array(dir.buffer), name, extraDir);
    offset += 30 + name.length + extraLocal.length + data.length;
  }
  const centralSize = central.reduce((n, part) => n + part.length, 0);
  const end = new DataView(new ArrayBuffer(22));
  end.setUint32(0, 0x06054b50, true);
  end.setUint16(8, files.length, true);
  end.setUint16(10, files.length, true);
  end.setUint32(12, centralSize, true);
  end.setUint32(16, offset, true);
  return new Blob([...parts, ...central, new Uint8Array(end.buffer)], { type: 'application/zip' });
}

function crcTable() {
  const table = new Uint32Array(256);
  for (let i = 0; i < 256; i += 1) {
    let c = i;
    for (let k = 0; k < 8; k += 1) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    table[i] = c >>> 0;
  }
  return table;
}

function crc32(bytes, table) {
  let c = 0xffffffff;
  for (let i = 0; i < bytes.length; i += 1) c = table[(c ^ bytes[i]) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}
