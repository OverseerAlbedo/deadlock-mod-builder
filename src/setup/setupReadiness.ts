import type { Settings, SetupStatus } from '../models';

const requiredFiles = ['Deadlock', 'Original VPK', 'CSDK', 'Resource compiler', 'Source 2 Viewer CLI'];
/** Settings field names as the Settings page labels them. */
export const fieldLabels: Record<string, string> = {
  workspace: 'Workspace folder', deadlock: 'Deadlock installation', pak: 'Deadlock game archive', source2viewer: 'Source 2 Viewer CLI',
  resourcecompiler: 'Resource compiler', csdk: 'Community SDK', cswin: 'Model compiler', cswin_resourcecompiler: 'Model compiler executable',
  model_game: 'Model compiler game folder', blender: 'Blender executable', vpk_tool: 'External VPK tool', steam: 'Steam installation', optional_cs2: 'Counter-Strike 2 installation',
};
export function setupReadiness(settings: Settings, status: SetupStatus | null, dirty: boolean) {
  const missing = status ? requiredFiles.filter(name => status.files_found[name] !== true) : [];
  if (status && status.model_files_found !== true) missing.push('Model compiler and game configuration');
  if (!settings.workspace.trim()) missing.push('Workspace folder');
  // A path that exists can still be the wrong one (the SDK's outer folder,
  // the model toolset's compiler in the material field): the field checks say so.
  const wrong = (status?.fields ?? []).filter(check => check.state === 'problem').map(check => fieldLabels[check.field] ?? check.field)
    .filter(label => !missing.includes(label) && !(label.startsWith('Model compiler') && missing.includes('Model compiler and game configuration')));
  missing.push(...wrong.map(label => `${label} (wrong path)`));
  const ready = Boolean(status && !dirty && missing.length === 0);
  return {
    ready, missing,
    title: dirty ? 'Save your settings to finish this stage' : !status ? 'Check your tool locations' : ready ? 'Tool locations checked · settings saved' : 'Some tools still need attention',
    description: ready ? 'App setup is finished. You can stop here or follow the optional first-project guide. Each project still runs its compiler and compatibility checks.' : dirty ? 'Save and check tools before finishing. Your current project will be kept.' : !status ? 'Save and check tools to see what is available. You can also finish for now and return to Settings later.' : 'Fix the fields marked in red (each says what is wrong and offers the right path when it can find it). You can finish for now and return to Settings later.',
  };
}
