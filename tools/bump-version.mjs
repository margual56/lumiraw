/**
 * Set the version everywhere it is written down, which the "Versions agree"
 * CI job requires to be one and the same.
 *
 *     node tools/bump-version.mjs patch|minor|major|X.Y.Z
 *
 * Prints the new version. Cargo.lock follows on the next cargo build, and
 * the wasm modules carry the version too, so rebuild them after this
 * (`tools/build.sh --wasm`).
 */
import { readFileSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';

const ROOT = resolve(import.meta.dirname, '..');
const CRATES = ['core/Cargo.toml', 'core/darkroom/Cargo.toml', 'core/critic/Cargo.toml'];

const read = (path) => readFileSync(join(ROOT, path), 'utf8');
const write = (path, text) => writeFileSync(join(ROOT, path), text);

const current = read('core/Cargo.toml').match(/^version\s*=\s*"([^"]+)"/m)?.[1];
if (!current) throw new Error('no version in core/Cargo.toml');

function next(version, how) {
  if (/^\d+\.\d+\.\d+$/.test(how)) return how;
  const [major, minor, patch] = version.split('.').map(Number);
  if (how === 'major') return `${major + 1}.0.0`;
  if (how === 'minor') return `${major}.${minor + 1}.0`;
  if (how === 'patch') return `${major}.${minor}.${patch + 1}`;
  throw new Error(`not a bump: ${how}`);
}

const how = process.argv[2];
if (!how) {
  console.error('usage: node tools/bump-version.mjs patch|minor|major|X.Y.Z');
  process.exit(2);
}
const version = next(current, how);

// Only the package's own version line: the first `version =` in each manifest.
for (const path of CRATES) {
  write(path, read(path).replace(/^version\s*=\s*"[^"]+"/m, `version = "${version}"`));
}

const pkg = JSON.parse(read('web/package.json'));
pkg.version = version;
write('web/package.json', `${JSON.stringify(pkg, null, 2)}\n`);

const lock = JSON.parse(read('web/package-lock.json'));
lock.version = version;
if (lock.packages?.['']) lock.packages[''].version = version;
write('web/package-lock.json', `${JSON.stringify(lock, null, 2)}\n`);

console.log(version);
