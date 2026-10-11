"""Reviewed equipment mappings and exact existing-bone rigid prop transforms."""
from copy import deepcopy
import math
import numpy as np

from .. import model
from ..safety import BuildError


def _vector(value, name):
    if not isinstance(value, (list, tuple)) or len(value) != 3 or any(isinstance(x,bool) or not isinstance(x,(int,float)) or not math.isfinite(x) for x in value):
        raise BuildError(name+' requires three finite numeric values.')


def validate(state):
    value = state.get('equipment', {})
    if not isinstance(value,dict) or not isinstance(value.get('enabled',False),bool):raise BuildError('Equipment must be an enabled or disabled object.')
    rows = value.get('meshes', [])
    if not isinstance(rows,list) or len(rows)>256:raise BuildError('Equipment supports at most 256 mesh choices.')
    seen=set()
    for row in rows:
        if not isinstance(row,dict) or not isinstance(row.get('target',''),str):raise BuildError('Invalid equipment mesh choice.')
        target=row.get('target','')
        if target and target in seen:raise BuildError('Duplicate equipment mesh choice: '+target)
        seen.add(target)
        if row.get('action','keep') not in {'replace','hide','keep'}:raise BuildError('Choose Replace, Hide or Keep for equipment.')
        sources=row.get('sources',[])
        if not isinstance(sources,list) or any(not isinstance(x,str) or not x or x in {model.KEEP,model.DISABLE} for x in sources) or len(set(sources))!=len(sources):
            raise BuildError('Equipment replacement sources must be unique prepared mesh names.')
        if not isinstance(row.get('include_lods',True),bool):raise BuildError('Include LODs must be enabled or disabled.')
    props=value.get('props',[])
    if not isinstance(props,list) or len(props)>64:raise BuildError('Equipment supports at most 64 attached props.')
    for row in props:
        if not isinstance(row,dict) or not isinstance(row.get('enabled',True),bool):raise BuildError('Invalid attached prop.')
        for key in ('source','bone'):
            if not isinstance(row.get(key,''),str) or '\x00' in row.get(key,''):raise BuildError('Invalid prop '+key+'.')
        for key in ('translation','rotation'):_vector(row.get(key,[0,0,0]), 'Prop '+key)
        scale=row.get('scale',1.)
        if isinstance(scale,bool) or not isinstance(scale,(int,float)) or not math.isfinite(scale) or scale<=0:
            raise BuildError('Prop scale must be a positive finite number.')


def selected_props(state):
    validate(state)
    if not state.get('enabled') or not state.get('equipment',{}).get('enabled'):return []
    return [deepcopy(row) for row in state['equipment'].get('props',[]) if row.get('enabled',True)]


def apply_mappings(original, mappings, state, source_names):
    """Override existing render-node choices; normal ModelDoc patch handles refs."""
    validate(state);result=deepcopy(mappings)
    if not state.get('enabled') or not state.get('equipment',{}).get('enabled'):return result
    nodes={row['name']:row for row in original['meshes']}
    available=set(source_names);assigned={}
    for row in state['equipment'].get('meshes',[]):
        target=row.get('target','')
        if target not in nodes:raise BuildError('Choose equipment from the original character mesh list: '+repr(target))
        action=row.get('action','keep')
        if action=='replace':
            sources=row.get('sources',[])
            if not sources:raise BuildError('Choose the prepared replacement mesh(es) for '+target)
            missing=set(sources)-available
            if missing:raise BuildError('Equipment mesh is not present in the prepared model: '+', '.join(sorted(missing)))
            value=sources[0] if len(sources)==1 else list(sources)
        else:value=model.DISABLE if action=='hide' else model.KEEP
        affected=[target]
        if row.get('include_lods',True):
            family=model.canonical(model.lod_base(target))
            affected=[name for name in nodes if model.canonical(model.lod_base(name))==family]
        for name in affected:
            if name in assigned and assigned[name]!=value:raise BuildError('Equipment choices conflict across the LOD family: '+name)
            assigned[name]=deepcopy(value)
    result.update(assigned)
    if result and all(value==model.DISABLE for value in result.values()):raise BuildError('Keep at least one visible character mesh.')
    return result


def prop_matrix(target, prop):
    from ..preparation.blender_scripts.rigid_props import prop_matrix as transform
    validate({'equipment':{'props':[prop]}})
    try:return transform(target,prop)
    except ValueError as ex:raise BuildError(str(ex)) from ex


def attach_mesh(mesh, target, prop):
    from ..preparation.blender_scripts.rigid_props import attach_mesh as attach
    validate({'equipment':{'props':[prop]}})
    try:return attach(mesh,target,prop)
    except ValueError as ex:raise BuildError(str(ex)) from ex


def discover(report):
    """Small frontend inventory using actual analyzed names, never hero guesses."""
    original=report.get('model',{})
    if not isinstance(original,dict):original={}
    meshes=original.get('meshes',[])
    summary=report.get('dmx',{})
    if not isinstance(summary,dict):summary={}
    bones=summary.get('bones',[])
    return {'materials':[{'path':path,'label':path.rsplit('/',1)[-1]} for path in report.get('materials',{})],
            'meshes':[{'name':row['name'],'label':row['name'],'lower_lod':row.get('lower_lod',False)} for row in meshes],
            'source_meshes':[{'name':row['name'],'label':row['name']} for row in summary.get('meshes',[])],
            'bones':[{'name':name,'label':name} for name in bones],
            'morphs':[{'name':row['name'],'label':row['name']} for row in report.get('analysis',{}).get('morphs',[])]}


def validate_face(face):
    if not isinstance(face,dict) or not isinstance(face.get('enabled',False),bool):raise BuildError('Face must be an enabled or disabled object.')
    if face.get('mode','static')!='static':raise BuildError('Only explicit static facial shape baking is supported; runtime facial animation requires a validated morph exporter.')
    weights=face.get('weights',{})
    if not isinstance(weights,dict) or len(weights)>512:raise BuildError('Face supports at most 512 static shape values.')
    for name,value in weights.items():
        if not isinstance(name,str) or not name or '\x00' in name:raise BuildError('Invalid facial shape name.')
        if isinstance(value,bool) or not isinstance(value,(int,float)) or not math.isfinite(value) or not 0<=value<=1:
            raise BuildError('Static facial shape values must be finite numbers from 0 to 1.')


def face_selection(state):
    face=state.get('face',{});validate_face(face)
    return deepcopy(face) if state.get('enabled') and face.get('enabled') else None


def project_mappings(original, effective, baseline, state):
    """Persist reviewer choices without baking optional equipment into defaults.

An equipment-controlled row is editable in Equipment, not in the parallel
legacy mapping view. Uncontrolled reviewer edits remain normal project choices.
"""
    validate(state);result=deepcopy(effective)
    if not state.get('enabled') or not state.get('equipment',{}).get('enabled'):return result
    nodes={row['name']:row for row in original['meshes']};controlled=set()
    for row in state['equipment'].get('meshes',[]):
        target=row.get('target','')
        if target not in nodes:raise BuildError('Equipment target changed; analyze the selected character again.')
        if row.get('include_lods',True):
            family=model.canonical(model.lod_base(target))
            controlled.update(name for name in nodes if model.canonical(model.lod_base(name))==family)
        else:controlled.add(target)
    source_names={name for row in state['equipment'].get('meshes',[]) for name in row.get('sources',[])}
    expected=apply_mappings(original,baseline,state,source_names)
    for name in controlled:
        if effective.get(name)!=expected.get(name):
            raise BuildError('This mesh is controlled by Equipment. Change or remove its Equipment choice before editing the legacy mesh mapping: '+name)
        if name in baseline:result[name]=deepcopy(baseline[name])
        else:result.pop(name,None)
    return result
