/** The one language that has its own addresses: English is at the root, and
 *  Spanish under /es, so a search engine can find and index each. */
export const match = (param) => param === 'es';
