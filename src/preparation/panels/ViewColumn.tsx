// i18n-area: prepare
import { Suspense, type Dispatch, type SetStateAction } from 'react';
import { m, useT } from '../../i18n';
import type { Project } from '../../models';
import type { PreparationAnalysis, PreparationOptions, PreparationResult, Role, Vec3 } from '../models';
import type { GltfPreview } from '../../preview/gltf-packet';
import type { PreviewScene } from '../../preview/packet';
import type { ModelLabelSet } from '../../studio/ModelLabels';
import { StudioView } from '../../studio/Studio';
import { PrepIcon } from '../PreparationChrome';
import type { LitBone } from '../MappingViewport';
import { GltfViewport, MappingViewport, Viewport, type PrepView } from '../preparationShared';
const VIEW_NAMES={model:m('Your model'),joints:m('Joint check'),result:m('Result')} as const;
/** The 3D view card on the left of every tab: the Your model / Joint check / Result switch and
 * its three views. Each view loads only on a click. The page keeps this card in one place (same
 * parent, same element) so a tab change never remounts it or drops a loaded preview. The open tab's
 * labels on the character (`labels`) and the bone whose skin is lit (`lit`) reach the Joint check
 * view only once it is loaded; nothing is loaded to show them. */
export function ViewColumn({view,setView,result,preparedPreview,previewPrepared,mappingPreview,analysis,options,selectedRole,selectRole,selectedBone,busy,loadPreview,preview,source,project,rotation,labels,lit,labelHint}:{view:PrepView;setView:Dispatch<SetStateAction<PrepView>>;result:PreparationResult|null;preparedPreview:PreviewScene|null;previewPrepared:()=>Promise<void>;mappingPreview:GltfPreview|null;analysis:PreparationAnalysis;options:PreparationOptions;selectedRole:Role;selectRole:(role:Role)=>void;selectedBone:string;busy:boolean;loadPreview:(mapping?:boolean)=>Promise<void>;preview:GltfPreview|null;source:string;project:Project;rotation:Vec3;
  /** Labels on the character for the open tab (Joint check only). */labels?:ModelLabelSet;
  /** The bone whose skin is lit in Joint check (null: none). */lit?:LitBone|null;
  /** A short line beside the view's name while labels are shown (shown as given). */labelHint?:string}){
  const t=useT();
  const rotated=rotation.some(v=>v!==0)&&view!=='result';
  const hint=view==='joints'&&mappingPreview&&labels?.items.length?labelHint:undefined;
  return (
    <StudioView title={t('3D view')} className="prep-view"
      bar={<><strong className="prep-view-title">{t(VIEW_NAMES[view])}</strong>{hint&&<span className="prep-view-hint">{hint}</span>}</>}
      barEnd={<div className="studio-view-group prep-view-switch" role="group" aria-label={t('What the 3D view shows')}>{(['model','joints','result'] as const).map(id=><button key={id} type="button" aria-pressed={view===id} disabled={id==='result'&&!result} onClick={()=>{setView(id);if(id==='result'&&!preparedPreview)void previewPrepared();}}>{t(VIEW_NAMES[id])}</button>)}</div>}
      footer={rotated?<p className="hint">{t('The viewer shows the source’s original orientation. Conversion uses the inspected rotation: {rotation}°.',{rotation:rotation.join(', ')})}</p>:undefined}>
      {view==='joints'&&(mappingPreview?<Suspense fallback={<p className="studio-view-wait">{t('Loading mapping renderer…')}</p>}><MappingViewport preview={mappingPreview} target={analysis.target} sourceBones={analysis.analysis.bones} options={options} selected={selectedRole} select={selectRole} selectedBone={selectedBone} labels={labels} lit={lit}/></Suspense>:<div className="prep-preview-empty"><div className="prep-preview-orbit"><PrepIcon name="roles" size={88}/></div><h3>{t('See how your joints line up')}</h3><p>{t('Select a joint and try head, arm and knee movements with the current matches.')}</p><button disabled={busy} onClick={()=>void loadPreview(true)}>{t('Load model for mapping review')}</button></div>)}
      {view==='model'&&(preview?<Suspense fallback={<p className="studio-view-wait">{t('Loading source renderer…')}</p>}><GltfViewport preview={preview}/></Suspense>:<div className="prep-preview-empty"><div className="prep-preview-orbit"><PrepIcon name="eye" size={88}/></div><h3>{t('Your model as it is in your file')}</h3><p>{t('Plays its own animation, if it has one.')}</p><button disabled={busy||!source} onClick={()=>void loadPreview()}>{t('Load your model')}</button></div>)}
      {view==='result'&&(preparedPreview?<Suspense fallback={<p className="studio-view-wait">{t('Loading converted model…')}</p>}><Viewport scene={preparedPreview} project={project} disabled={busy} floor={0}/></Suspense>:<div className="prep-preview-empty"><div className="prep-preview-orbit"><PrepIcon name="model" size={88}/></div><h3>{t('The converted model')}</h3><button disabled={busy||!result} onClick={()=>void previewPrepared()}>{t('Preview converted model')}</button></div>)}
    </StudioView>
  );
}
