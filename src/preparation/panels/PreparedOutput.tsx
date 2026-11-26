// i18n-area: prepare
import type { Dispatch, SetStateAction } from 'react';
import { m, useT } from '../../i18n';
import { api, basename } from '../../api';
import type { Project } from '../../models';
import type { PreparationAnalysis, PreparationResult } from '../models';
import { Section } from '../../components/Primitives';
import { PathValue } from '../../components/Paths';
import { BUTTON_NAMES } from '../pageNames';
import type { PreviewScene } from '../../preview/packet';
import { PrepIcon } from '../PreparationChrome';
import type { Operate, PrepView } from '../preparationShared';
/** The "Prepared output" card. The warnings come from the conversion (Blender) and are shown as they come. */
export function PreparedOutput({result,busy,setView,jump,preparedPreview,previewPrepared,project,operate,analysis,convert,usedResult}:{result:PreparationResult;busy:boolean;setView:Dispatch<SetStateAction<PrepView>>;jump:(id:string)=>void;preparedPreview:PreviewScene|null;previewPrepared:()=>Promise<void>;project:Project;operate:Operate;analysis:PreparationAnalysis|null;convert:(correction?:boolean)=>Promise<void>;usedResult:boolean}){
  const t=useT();
  return (
    <Section title={t('Prepared output')} className="prep-card prep-output" id="prep-output"><div className="prep-output-ready"><PrepIcon name="check"/><div><strong>{t('Your converted model is ready to preview')}</strong><p>{t('Inspect the result, then use it in your project.')}</p></div></div><details className="prep-output-files"><summary>{t('Prepared files')}<PrepIcon name="folder" size={16}/></summary><dl className="summary-list"><dt>{t('Model')}</dt><dd><PathValue path={result.dmx}/></dd><dt>{t('Editable Blender file')}</dt><dd><PathValue path={result.blend}/></dd><dt>{t('Preparation report')}</dt><dd><PathValue path={result.report}/></dd></dl></details>{result.warnings.length>0&&<ul>{result.warnings.map((warning,index)=><li key={index}>{warning}</li>)}</ul>}<div className="prep-output-actions"><button className="prep-primary" disabled={busy} onClick={()=>{setView('result');jump('prep-review-workspace');if(!preparedPreview)void previewPrepared();}}>{t(BUTTON_NAMES.showResult)}</button><button disabled={busy||project.preparation_blend!==result.blend} onClick={()=>void operate(m('Blender could not be opened'),()=>api.preparationOpenBlend(project))}>{t('Open accepted Blender file')}</button><button disabled={busy||!analysis} onClick={()=>void convert(true)}>{t('Re-export corrections')}</button></div>{!usedResult&&<p className="hint">{t('Use {file} to store this preparation in the project before opening its editable file.',{file:basename(result.dmx)})}</p>}</Section>
  );
}
