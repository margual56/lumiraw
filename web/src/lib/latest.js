/**
 * Wraps an async `fn` so that calls run one at a time: a call made while one
 * is running waits, and only the latest of those runs next.
 */
export function latest(fn) {
  let running = false;
  let wanted = null;
  return async (...args) => {
    wanted = args;
    if (running) return;
    running = true;
    try {
      while (wanted) {
        const next = wanted;
        wanted = null;
        await fn(...next);
      }
    } finally {
      running = false;
    }
  };
}
