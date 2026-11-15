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
    expect(result.missing).toEqual(['Source 2 Viewer CLI','Model compiler']);
    expect(result.title).toBe('Some tools still need attention');
    expect(result.description).toContain('finish for now');
    expect(setupReadiness(settings,found,false).description).toContain('Each project still runs its compiler and compatibility checks');
  });
});

describe('A missing Community SDK', () => {
  it('is listed once as Reduced CSDK 13, not also as its resource and model compilers', () => {
    const status: SetupStatus = {...found, model_files_found:false, files_found:{...found.files_found, CSDK:false, 'Resource compiler':false}, fields:[
      {field:'csdk',state:'problem',message:'Choose the extracted Reduced_CSDK_13 folder'},
      {field:'resourcecompiler',state:'problem',message:'Choose resourcecompiler.exe'},
      {field:'cswin',state:'problem',message:'The model compiler is not set up.'}]};
    expect(setupReadiness(settings, status, false).missing).toEqual(['Reduced CSDK 13 (Community SDK)']);
    // With the SDK found, a missing compiler and model compiler are named on their own.
    const parts: SetupStatus = {...found, model_files_found:false, files_found:{...found.files_found, 'Resource compiler':false}, fields:[{field:'cswin',state:'problem',message:'The model compiler is not set up.'}]};
    expect(setupReadiness(settings, parts, false).missing).toEqual(['Resource compiler','Model compiler']);
  });
  it('an empty Blender never blocks setup but is mentioned', () => {
    const result = setupReadiness({...settings, blender:''} as Settings, found, false);
    expect(result.ready).toBe(true);
    expect(result.description).toContain('Blender is not set yet');
    expect(setupReadiness({...settings, blender:String.raw`C:\Blender\blender.exe`} as Settings, found, false).description).not.toContain('Blender');
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
  it('Reduced CSDK 13 without a model toolset is a finished setup', () => {
    // The checks report the empty model toolset as fine when the SDK compiles models.
    const status: SetupStatus = {...found, model_status:'Models are compiled with the Community SDK (Reduced CSDK 13); no separate model toolset is needed.', fields: [
      {field:'resourcecompiler',state:'ok',message:String.raw`Material compiler found (game\bin\win64, the tested copy).`},
      {field:'csdk',state:'ok',message:'Reduced CSDK 13 found. It compiles materials and hero models.'},
      {field:'cswin',state:'ok',message:'Models are compiled with Reduced CSDK 13; no separate model toolset is needed.'}]};
    expect(setupReadiness({...settings, cswin:''} as Settings, status, false)).toMatchObject({ready:true, missing:[]});
    // The bin_server copy is a warning with a fix, not a reason to stay unfinished.
    const copy: SetupStatus = {...status, fields: [{field:'resourcecompiler',state:'warning',message:'other copy',fix:String.raw`C:\CSDK13\game\bin\win64\resourcecompiler.exe`}]};
    expect(setupReadiness(settings, copy, false).ready).toBe(true);
  });
});
