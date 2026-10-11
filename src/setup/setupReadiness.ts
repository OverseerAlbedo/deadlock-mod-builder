import type { Settings, SetupStatus } from '../models';

const requiredFiles = ['Deadlock', 'Original VPK', 'CSDK', 'Resource compiler', 'Source 2 Viewer CLI'];
export function setupReadiness(settings: Settings, status: SetupStatus | null, dirty: boolean) {
  const missing = status ? requiredFiles.filter(name => status.files_found[name] !== true) : [];
  if (status && status.model_files_found !== true) missing.push('Model compiler and game configuration');
  if (!settings.workspace.trim()) missing.push('Workspace folder');
  const ready = Boolean(status && !dirty && missing.length === 0);
  return {
    ready, missing,
    title: dirty ? 'Save your settings to finish this stage' : !status ? 'Check your tool locations' : ready ? 'Tool locations checked · settings saved' : 'Some tools still need attention',
    description: ready ? 'App setup is finished. You can stop here or follow the optional first-project guide. Each project still runs its compiler and compatibility checks.' : dirty ? 'Save and check tools before finishing. Your current project will be kept.' : !status ? 'Save and check tools to see what is available. You can also finish for now and return to Settings later.' : 'You can finish for now and return to Settings later. Missing tools must be configured before the workflows that need them.',
  };
}
