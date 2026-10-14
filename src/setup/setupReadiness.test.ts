import { describe, expect, it } from 'vitest';
import type { Settings, SetupStatus } from '../models';
import { setupReadiness } from './setupReadiness';

const settings = {workspace:'C:\\Workspace'} as Settings;
const found: SetupStatus = {files_found:{Deadlock:true,'Original VPK':true,CSDK:true,'Resource compiler':true,'Source 2 Viewer CLI':true},model_status:'model files found',model_files_found:true};
describe('App setup completion', () => {
  it('requires saved fields and every tool presence check, including a model game configuration', () => {
    expect(setupReadiness(settings,found,false).ready).toBe(true);
    for (const status of [null,{...found,model_files_found:undefined},{...found,model_files_found:false},{...found,files_found:{...found.files_found,CSDK:false}}]) {
      expect(setupReadiness(settings,status,false).ready).toBe(false);
    }
    expect(setupReadiness(settings,found,true).ready).toBe(false);
    expect(setupReadiness({workspace:' '} as Settings,found,false).ready).toBe(false);
  });
  it('names missing tools without claiming compilation or project compatibility', () => {
    const result=setupReadiness(settings,{...found,model_files_found:false,files_found:{...found.files_found,'Source 2 Viewer CLI':false}},false);
    expect(result.missing).toEqual(['Source 2 Viewer CLI','Model compiler and game configuration']);
    expect(result.title).toBe('Some tools still need attention');
    expect(result.description).toContain('finish for now');
    expect(setupReadiness(settings,found,false).description).toContain('Each project still runs its compiler and compatibility checks');
  });
});

describe('Wrong paths', () => {
  it('a path that exists but is the wrong one keeps setup unfinished and names the field', () => {
    const status: SetupStatus = {...found, fields: [{field:'resourcecompiler',state:'problem',message:'This is the model compiler',fix:String.raw`C:\CSDK12\game\bin_cs2\win64\resourcecompiler.exe`},{field:'csdk',state:'ok',message:'ok'}]};
    const result = setupReadiness(settings, status, false);
    expect(result.ready).toBe(false);
    expect(result.missing).toEqual(['Resource compiler (wrong path)']);
    expect(result.description).toContain('marked in red');
    expect(setupReadiness(settings, {...found, fields: [{field:'blender',state:'warning',message:'x'}]}, false).ready).toBe(true);
  });
});
