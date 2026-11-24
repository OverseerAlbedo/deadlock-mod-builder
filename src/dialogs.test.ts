import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const dialog = vi.hoisted(() => ({ open: vi.fn(), save: vi.fn() }));
vi.mock('@tauri-apps/plugin-dialog', () => dialog);

import { DIALOG_FOLDERS, folderOf, joinPath, lastFolder, openFor, PURPOSE_OF_KIND, rememberFolder, saveFor } from './dialogs';

beforeEach(() => { localStorage.clear(); dialog.open.mockReset(); dialog.save.mockReset(); });
afterEach(() => vi.restoreAllMocks());

describe('file windows per purpose', () => {
  it('starts each window in the folder last used for its purpose', async () => {
    dialog.open.mockResolvedValueOnce('D:\\Characters\\hero\\model.fbx');
    await openFor('model-source', { title: 'Model', multiple: false });
    // The first window had no folder to start in.
    expect(dialog.open).toHaveBeenLastCalledWith({ title: 'Model', multiple: false });
    dialog.open.mockResolvedValueOnce(null);
    await openFor('model-source', { title: 'Model', multiple: false });
    expect(dialog.open).toHaveBeenLastCalledWith({ title: 'Model', multiple: false, defaultPath: 'D:\\Characters\\hero' });
  });
  it('keeps the purposes apart', async () => {
    dialog.open.mockResolvedValueOnce('D:\\Characters\\hero\\model.fbx');
    await openFor('model-source', { title: 'Model' });
    dialog.save.mockResolvedValueOnce('E:\\Mods\\Projects\\first.dmb.json');
    await saveFor('project', { title: 'Save project', defaultPath: 'first.dmb.json' });
    dialog.open.mockResolvedValueOnce('F:\\Sounds\\shot.wav');
    await openFor('sound-file', { title: 'Sound' });
    expect(lastFolder('model-source')).toBe('D:\\Characters\\hero');
    expect(lastFolder('project')).toBe('E:\\Mods\\Projects');
    expect(lastFolder('sound-file')).toBe('F:\\Sounds');
    expect(lastFolder('gun-file')).toBeUndefined();
    // Opening a project starts where the last project was saved (one purpose for both).
    dialog.open.mockResolvedValueOnce(null);
    await openFor('project', { title: 'Open project' });
    expect(dialog.open).toHaveBeenLastCalledWith({ title: 'Open project', defaultPath: 'E:\\Mods\\Projects' });
    // A cancelled window changes nothing.
    expect(lastFolder('project')).toBe('E:\\Mods\\Projects');
  });
  it('records a picked folder itself, not its parent', async () => {
    dialog.open.mockResolvedValueOnce('C:\\Tools\\Blender 5.2');
    await openFor('tools-folder', { title: 'Blender', directory: true });
    expect(lastFolder('tools-folder')).toBe('C:\\Tools\\Blender 5.2');
  });
  it('builds a save path from the folder and the suggested name with the folder\'s own separator', async () => {
    rememberFolder('project', 'E:\\Mods\\Projects');
    dialog.save.mockResolvedValue(null);
    await saveFor('project', { title: 'Save project', defaultPath: 'my_character.dmb.json' });
    expect(dialog.save).toHaveBeenLastCalledWith({ title: 'Save project', defaultPath: 'E:\\Mods\\Projects\\my_character.dmb.json' });
    rememberFolder('outfit-preset', '/home/user/outfits');
    await saveFor('outfit-preset', { title: 'Save outfit', defaultPath: 'a.outfit.json' });
    expect(dialog.save).toHaveBeenLastCalledWith({ title: 'Save outfit', defaultPath: '/home/user/outfits/a.outfit.json' });
    // A full path (the project's own file) is used as it is.
    await saveFor('project', { title: 'Save project', defaultPath: 'C:\\Elsewhere\\own.dmb.json' });
    expect(dialog.save).toHaveBeenLastCalledWith({ title: 'Save project', defaultPath: 'C:\\Elsewhere\\own.dmb.json' });
    // No folder yet: the suggested name alone.
    await saveFor('log', { title: 'Save log', defaultPath: 'deadlock-build.log' });
    expect(dialog.save).toHaveBeenLastCalledWith({ title: 'Save log', defaultPath: 'deadlock-build.log' });
  });
  it('joins and splits paths', () => {
    expect(joinPath('C:\\', 'a.json')).toBe('C:\\a.json');
    expect(joinPath('D:\\x', 'a.json')).toBe('D:\\x\\a.json');
    expect(joinPath('/srv', 'a.json')).toBe('/srv/a.json');
    expect(joinPath('', 'a.json')).toBe('a.json');
    expect(folderOf('C:\\a.json')).toBe('C:\\');
    expect(folderOf('D:/x/y/a.json')).toBe('D:/x/y');
    expect(folderOf('a.json')).toBe('');
  });
  it('survives storage that throws or holds rubbish', async () => {
    localStorage.setItem(DIALOG_FOLDERS, '{not json');
    expect(lastFolder('project')).toBeUndefined();
    localStorage.setItem(DIALOG_FOLDERS, '[1,2]');
    expect(lastFolder('project')).toBeUndefined();
    vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => { throw new Error('blocked'); });
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => { throw new Error('blocked'); });
    dialog.open.mockResolvedValueOnce('D:\\x\\model.fbx');
    await expect(openFor('model-source', { title: 'Model' })).resolves.toBe('D:\\x\\model.fbx');
    dialog.save.mockResolvedValueOnce('D:\\x\\p.dmb.json');
    await expect(saveFor('project', { title: 'Save', defaultPath: 'p.dmb.json' })).resolves.toBe('D:\\x\\p.dmb.json');
    expect(dialog.save).toHaveBeenLastCalledWith({ title: 'Save', defaultPath: 'p.dmb.json' });
  });
  it('gives every kind of asset path a purpose', () => {
    expect(PURPOSE_OF_KIND.weapon).toBe('gun-file');
    expect(PURPOSE_OF_KIND.garment).toBe(PURPOSE_OF_KIND.prop);
    expect(PURPOSE_OF_KIND.variant_project).toBe('project');
  });
});
