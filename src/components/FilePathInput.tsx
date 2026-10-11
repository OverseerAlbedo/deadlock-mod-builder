import { useEffect, useRef, useState } from 'react';
import { open } from '@tauri-apps/plugin-dialog';
import { api } from '../api';
import type { PathKind } from '../models';
import { CompactPathField } from './Paths';

const extensions: Record<PathKind, string[]> = {
  texture_directory:[],texture:['png','tga','jpg','jpeg','tif','tiff','exr'],physics:['vmdl_c'],garment:['blend','glb','gltf','fbx'],icon:['png','jpg','jpeg'],
  animation:['vnmclip','vnmclip_c','blend','fbx','glb','gltf'],effect:['vpcf_c'],audio:['wav','mp3','flac','ogg','vsnd_c'],
  prop:['blend','glb','gltf','fbx'],variant_project:['json'],
};
export function FilePathInput({label,value,kind,update,disabled,fail}: {
  label:string;value:string;kind:PathKind;update:(path:string)=>void;disabled:boolean;fail:(title:string,error:unknown)=>void;
}) {
  const node = useRef<HTMLDivElement>(null);
  const [draft,setDraft] = useState(value), [validating,setValidating] = useState(false);
  useEffect(()=>setDraft(value),[value]);
  const commit = async (path:string) => {
    if (disabled || validating) return;
    if (!path.trim()) {update('');setDraft('');return;}
    setValidating(true);
    try {const checked = await api.validateAssetPath(path.trim(),kind);update(checked);setDraft(checked);}
    catch(error){setDraft(value);fail(`Could not select ${label.toLowerCase()}`,error);}
    finally{setValidating(false);}
  };
  const commitRef=useRef(commit); useEffect(()=>{commitRef.current=commit;});
  useEffect(()=>{
    const element=node.current;
    const drop=(event:Event)=>{const paths=(event as CustomEvent<string[]>).detail;if(paths.length===1)void commitRef.current(paths[0]);else fail('Choose one source','Drop one local file or folder for this field.');};
    element?.addEventListener('dmb-file-drop',drop);
    return()=>element?.removeEventListener('dmb-file-drop',drop);
  },[fail]);
  async function browse(){try{const path=await open({title:label,multiple:false,directory:kind==='texture_directory',...(extensions[kind].length?{filters:[{name:label,extensions:extensions[kind]}]}:{})});if(typeof path==='string')await commit(path);}catch(error){fail(`Could not select ${label.toLowerCase()}`,error);}}
  return <div className="path-field asset-path" ref={node} data-native-drop={disabled?undefined:kind}><CompactPathField folder={kind==='texture_directory'} label={label} value={draft} committed={value} update={setDraft} browse={browse} disabled={disabled||validating} checking={validating} placeholder={kind==='texture_directory'?'Drop a folder here or browse…':'Drop a file here or browse…'} onBlur={()=>{if(draft!==value)void commit(draft);}} onKeyDown={event=>{if(event.key==='Enter'){event.preventDefault();void commit(draft);}}}/></div>;
}
