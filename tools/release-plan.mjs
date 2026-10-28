/**
 * What the next release should be, from what reached main since the last
 * one. Prints `patch`, `minor`, `major`, `current` (the version was bumped
 * by hand and is not released yet) or `none`.
 *
 *     node tools/release-plan.mjs [auto|patch|minor|major]
 *
 * The size comes from conventional titles on the commits (and so on squashed
 * pull requests): `feat:` is minor, `type!:` or a "BREAKING CHANGE" is
 * major, anything else is a patch. Changes to nothing but docs, CI config
 * and the scripts in tools/ are not released: none of them reaches the site.
 */
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';

const git = (...args) => execFileSync('git', args, { encoding: 'utf8' }).trim();

/** Paths whose changes alone are not worth a release. */
export const quiet = (path) =>
  /(^|\/)[^/]*\.md$|^\.github\/|^tools\/|^LICENSE$|^\.gitignore$|^\.gitattributes$/.test(path);

/** The bump a list of commits ({ subject, body }) asks for. */
export function size(commits) {
  if (commits.some(({ subject, body }) => /^\w+(\([^)]*\))?!:/.test(subject)
                                        || /^BREAKING[ -]CHANGE:/m.test(body))) return 'major';
  if (commits.some(({ subject }) => /^feat(\([^)]*\))?:/i.test(subject))) return 'minor';
  return 'patch';
}

function plan(asked) {
  const version = readFileSync('core/Cargo.toml', 'utf8').match(/^version\s*=\s*"([^"]+)"/m)[1];
  const tags = git('tag', '--list', 'v*').split('\n').filter(Boolean);
  if (!tags.includes(`v${version}`)) return 'current';
  if (asked && asked !== 'auto') return asked;

  const last = git('describe', '--tags', '--abbrev=0', '--match', 'v*');
  const changed = git('diff', '--name-only', `${last}..HEAD`).split('\n').filter(Boolean);
  if (!changed.some((path) => !quiet(path))) return 'none';

  const log = git('log', '--format=%s%x1f%b%x1e', `${last}..HEAD`);
  const commits = log.split('\x1e').map((entry) => entry.trim()).filter(Boolean)
    .map((entry) => {
      const [subject, body = ''] = entry.split('\x1f');
      return { subject: subject.trim(), body };
    });
  return size(commits);
}

if (import.meta.url === `file://${process.argv[1]}`) {
  console.log(plan(process.argv[2]));
}
