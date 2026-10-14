import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import reference from '../../src-tauri/crates/deadlock-project/tests/fixtures/reference.json';
import type { Project } from '../models';

const mocks = vi.hoisted(() => ({motionPreview: vi.fn()}));
vi.mock('../api', () => ({api: mocks}));
import { CharacterPreview } from './CharacterPreview';

const project = {...structuredClone(reference.cases[0].expected), dmx: 'C:\Work\prepared.dmx'} as unknown as Project;
const packet = () => {
  const header = new TextEncoder().encode(JSON.stringify({bones: [{name: 'root', parent: -1, position: [0, 0, 0]}], clips: [{name: 'run', sources: ['weapon_run_n'], fps: 30, frames: 1, segments: [1], offset: 0}]}));
  const buffer = new ArrayBuffer(Math.ceil((4 + header.length) / 4) * 4 + 48);
  new DataView(buffer).setUint32(0, header.length, true);
  new Uint8Array(buffer, 4).set(header);
  return buffer;
};

describe('Your character', () => {
  it('loads the character and the hero clips for the movement view', async () => {
    mocks.motionPreview.mockResolvedValue(packet());
    const load = vi.fn();
    render(<CharacterPreview project={project} preview={null} load={load} busy={false}/>);
    fireEvent.click(screen.getByRole('radio', {name: 'Movement'}));
    expect(load).toHaveBeenCalled();
    await waitFor(() => expect(mocks.motionPreview).toHaveBeenCalledWith(project));
    expect(screen.getByText(/Loading your character and the hero’s stand, run and jump clips/)).toBeInTheDocument();
  });
  it('goes back to the still view when the clips cannot be read', async () => {
    mocks.motionPreview.mockRejectedValue({kind: 'validation', message: 'Prepare the model first'});
    const fail = vi.fn();
    render(<CharacterPreview project={project} preview={null} load={vi.fn()} busy={false} fail={fail}/>);
    fireEvent.click(screen.getByRole('radio', {name: 'Movement'}));
    await waitFor(() => expect(fail).toHaveBeenCalledWith('The movement preview could not be loaded', expect.anything()));
    expect(screen.getByRole('radio', {name: 'Still'})).toHaveAttribute('aria-checked', 'true');
  });
});
