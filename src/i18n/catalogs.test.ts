// @vitest-environment node
import { describe, expect, it } from 'vitest';
// The same check as `node scripts/i18n-check.mjs` (plain run), here so that every build runs it
// after npm ci (the repository check runs before the TypeScript package is installed).
// @ts-expect-error: a plain JavaScript module without types.
import { checkI18n } from '../../scripts/i18n-check.mjs';

describe('The language catalogs', () => {
  it('match the code: nothing unused, wrong, translated two ways or flowing where it must not', async () => {
    const result = await checkI18n() as { failed: boolean; lines: string[] };
    expect(result.lines.filter(line => !/missing in /.test(line)).join('\n')).toMatch(/i18n check passed/);
    expect(result.failed).toBe(false);
  }, 60000);
});
