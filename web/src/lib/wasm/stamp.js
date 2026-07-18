/** The current time, as EXIF spells it. */
export function stamp(when = new Date()) {
  const pad = (n) => String(Math.floor(Math.abs(n))).padStart(2, '0');
  const minutes = -when.getTimezoneOffset();
  const offset = `${minutes < 0 ? '-' : '+'}${pad(minutes / 60)}:${pad(minutes % 60)}`;
  return `${when.getFullYear()}:${pad(when.getMonth() + 1)}:${pad(when.getDate())} `
       + `${pad(when.getHours())}:${pad(when.getMinutes())}:${pad(when.getSeconds())}${offset}`;
}
