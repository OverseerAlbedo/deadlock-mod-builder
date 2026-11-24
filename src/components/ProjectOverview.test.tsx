import { render, screen, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ProjectOverview } from './ProjectOverview';
import { inspected, reviewedProject } from '../build/testFixtures';

const native = vi.hoisted(() => ({ heroes: vi.fn(), heroProportions: vi.fn(), validateAssetPath: vi.fn() }));
vi.mock('../api', async original => ({ ...await original<typeof import('../api')>(), api: native }));
beforeEach(() => { vi.clearAllMocks(); native.heroes.mockResolvedValue([]); native.heroProportions.mockResolvedValue(null); });

describe('Character & model with an analysis that is not current', () => {
  // Build & install keeps the last analysis as a working list; this page does not
  // and reads a report that is not current as no analysis, as before.
  it('shows the same next step and progress as without a report', () => {
    const props = { fresh: false, disabled: false, update: vi.fn(), selectHero: vi.fn(), fail: vi.fn(), go: vi.fn(), loadOutfit: vi.fn(), saveOutfit: vi.fn() };
    const project = { ...reviewedProject, name: 'Renamed', preparation_source: '' };
    const summary = () => ({ next: within(screen.getByRole('region', { name: 'Next step' })).getByRole('button').textContent, progress: screen.getByText(/areas ready/).textContent, rows: screen.getByRole('region', { name: 'Configuration progress' }).textContent });
    const view = render(<ProjectOverview {...props} document={{ path: null, project }} report={{ ...inspected, needs_review: true }} />);
    const stale = summary();
    expect(stale.next).toBe('Go to Appearance & weapons →');
    view.rerender(<ProjectOverview {...props} document={{ path: null, project }} report={null} />);
    expect(summary()).toEqual(stale);
  });
});
