import type { Dispatch, SetStateAction } from 'react';
import { api, basename } from '../../api';
import type { Project } from '../../models';
import type { PreparationAnalysis, PreparationResult } from '../models';
import { Section } from '../../components/Primitives';
import { PathValue } from '../../components/Paths';
import { BUTTON_NAMES } from '../pageNames';
import type { PreviewScene } from '../../preview/packet';
import { PrepIcon } from '../PreparationChrome';
import type { Operate, PrepView } from '../preparationShared';
/** The "Prepared output" card. */
export function PreparedOutput({result,busy,setView,jump,preparedPreview,previewPrepared,project,operate,analysis,convert,usedResult}:{result:PreparationResult;busy:boolean;setView:Dispatch<SetStateAction<PrepView>>;jump:(id:string)=>void;preparedPreview:PreviewScene|null;previewPrepared:()=>Promise<void>;project:Project;operate:Operate;analysis:PreparationAnalysis|null;convert:(correction?:boolean)=>Promise<void>;usedResult:boolean}){
  return (
    <Section title="Prepared output" className="prep-card prep-output" id="prep-output"><div className="prep-output-ready"><PrepIcon name="check"/><div><strong>Your converted model is ready to preview</strong><p>Inspect the result, then use it in your project.</p></div></div><details className="prep-output-files"><summary>Prepared files<PrepIcon name="folder" size={16}/></summary><dl className="summary-list"><dt>Model</dt><dd><PathValue path={result.dmx}/></dd><dt>Editable Blender file</dt><dd><PathValue path={result.blend}/></dd><dt>Preparation report</dt><dd><PathValue path={result.report}/></dd></dl></details>{result.warnings.length>0&&<ul>{result.warnings.map((warning,index)=><li key={index}>{warning}</li>)}</ul>}<div className="prep-output-actions"><button className="prep-primary" disabled={busy} onClick={()=>{setView('result');jump('prep-review-workspace');if(!preparedPreview)void previewPrepared();}}>{BUTTON_NAMES.showResult}</button><button disabled={busy||project.preparation_blend!==result.blend} onClick={()=>void operate('Blender could not be opened',()=>api.preparationOpenBlend(project))}>Open accepted Blender file</button><button disabled={busy||!analysis} onClick={()=>void convert(true)}>Re-export corrections</button></div>{!usedResult&&<p className="hint">Use {basename(result.dmx)} to store this preparation in the project before opening its editable file.</p>}</Section>
  );
}
