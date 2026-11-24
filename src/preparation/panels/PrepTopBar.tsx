import type { ReactNode } from 'react';
import { basename } from '../../api';
import type { PreparationAnalysis, SceneChoices } from '../models';
import { StudioTopBar, type StudioTone } from '../../studio/Studio';
import { StudioBarMenuButton, StudioBarMenuPanel, useStudioBarMenus } from '../../studio/StudioBarMenu';
import { sceneSummary } from '../SceneObjects';
import { SOURCE_TYPES } from './SourceCard';
import { noteCount } from './InspectionCard';

/** The page's four top-bar menus. */
export type PrepMenu='model'|'mode'|'scene'|'notes';
export interface PrepMenuContent { model:ReactNode; mode?:ReactNode; scene?:ReactNode; notes:ReactNode }
const MODE_NAMES:Record<string,string>={rigged_humanoid:'Rigged humanoid',unrigged_humanoid:'Unrigged humanoid',prepared:'Prepared source',weapon:'Weapon model (gun only)'};

/** The frame's top bar: the model file's name, a badge (checked, or what needs a look), the short
 * stats line, and after a check the four menus (Model file & orientation, Preparation mode, Scene
 * objects, Notes) whose panels open under the bar. `mode`/`scene` are left out when not offered. */
export function PrepTopBar({source,analysis,unmapped,mode,attention,notesShown,content,menus,sceneChoices}:{source:string;analysis:PreparationAnalysis|null;unmapped:number;mode:string;/** The saved Scene objects choices (options.scene). */sceneChoices:SceneChoices;
  /** What needs a look (missing skeleton, role errors), shown as the badge instead of Checked. */attention:string|null;
  /** Whether the Notes menu holds the inspection notes (Expert, or matching shown). */notesShown:boolean;content:PrepMenuContent;menus:ReturnType<typeof useStudioBarMenus<PrepMenu>>}){
  const {open,toggle,close,base}=menus;
  const name=source?basename(source):'Your model';
  const inspection=analysis?.analysis;
  const badge:{text:string;tone:StudioTone}=!analysis?{text:'Not checked yet',tone:'quiet'}:attention?{text:attention,tone:'attention'}:{text:'Checked',tone:'ok'};
  const status=inspection?`${inspection.bones.length.toLocaleString()} bone${inspection.bones.length===1?'':'s'} · ${inspection.meshes.length.toLocaleString()} mesh${inspection.meshes.length===1?'':'es'} · ${inspection.vertices.toLocaleString()} vertices · ${unmapped.toLocaleString()} need${unmapped===1?'s':''} a target`:SOURCE_TYPES;
  const scene=inspection?.scene?sceneSummary(inspection.scene,sceneChoices):null;
  const notes=analysis&&notesShown?noteCount(analysis):0;
  const titles:Record<PrepMenu,string>={model:'Model file & orientation',mode:'Preparation mode',scene:'Scene objects',notes:'Notes'};
  // Before a check the Model file panel is open with no menu; it is the same panel in the same
  // slot as the menu after the check, so typing a new path (which drops the check) keeps the form.
  const shown:PrepMenu|null=!analysis?'model':open&&content[open]?open:null;
  const panel=shown?<StudioBarMenuPanel base={base} id={shown} title={titles[shown]} onClose={analysis?()=>close(shown):undefined} className={`prep-menu-${shown}`}>{content[shown]}</StudioBarMenuPanel>:undefined;
  return <StudioTopBar title={name} badge={badge.text} badgeTone={badge.tone} status={status} below={panel}>
    {analysis&&<>
      <StudioBarMenuButton base={base} id="model" open={open==='model'} onToggle={()=>toggle('model')} label={titles.model}/>
      {content.mode&&<StudioBarMenuButton base={base} id="mode" open={open==='mode'} onToggle={()=>toggle('mode')} hiddenLabel="Preparation mode" label={MODE_NAMES[mode]??mode}/>}
      {content.scene&&scene&&<StudioBarMenuButton base={base} id="scene" open={open==='scene'} onToggle={()=>toggle('scene')} label={titles.scene} detail={`${scene.kept} kept, ${scene.setAside} set aside${scene.pending?`, ${scene.pending} choice${scene.pending===1?'':'s'} of yours`:''}`}/>}
      <StudioBarMenuButton base={base} id="notes" open={open==='notes'} onToggle={()=>toggle('notes')} label={titles.notes} detail={notes?`(${notes})`:undefined} tone={notes?'attention':undefined}/>
    </>}
  </StudioTopBar>;
}
