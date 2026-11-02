/** What was kept under `key` in this browser, or an empty object if nothing can be read. */
export function readStored(key) {
  try {
    return JSON.parse(localStorage.getItem(key)) ?? {};
  } catch {
    return {};
  }
}
