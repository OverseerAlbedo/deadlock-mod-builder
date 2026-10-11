"""Differential source goldens from retained Python; development only."""
from pathlib import Path
import json, sys
sys.path.insert(0,str(Path(__file__).resolve().parents[4]))
import sys as _reference_sys
from pathlib import Path as _ReferencePath
_reference_sys.path.insert(0, str(next(p for p in _ReferencePath(__file__).resolve().parents if (p/'legacy/python/deadlock_builder').is_dir())/'legacy/python'))
from deadlock_builder import model,modeldoc
from deadlock_builder.customization import equipment
from tests.fixtures import VMDL
from tests.test_bodygroup_compatibility import SOURCE
from tests.test_lod_all import original,TEMPLATE,MAPPINGS,ALL
from tests.test_source_skeleton_aliases import source,authority,export_names
cases=[]
def add(name,operation,**data):
    case=dict(name=name,operation=operation,**data)
    try:
        if operation=='equipment': result=equipment.apply_mappings(model.analyze(data['source']),data['mappings'],data['state'],data['source_names'])
        elif operation=='equipment_project': result=equipment.project_mappings(model.analyze(data['source']),data['effective'],data['baseline'],data['state'])
        elif operation=='analyze': result=model.analyze(data['source'])
        elif operation=='patch':result=model.patch(data['source'],data['mappings'],data['source_paths'])
        elif operation=='merge':result=model.merge_template(data['source'],data['template'],data['mappings'],data['source_path'],data['strategy'],data.get('template_targets'),data.get('original_report'))
        elif operation=='suggest':
            mappings,unresolved=model.suggest_mappings(model.analyze(data['source']),data['replacements'],data.get('mappings',{}),data.get('strategy','reuse'),data.get('auto',True));result={'mappings':mappings,'unresolved':unresolved}
        elif operation=='plan':result=modeldoc.migration_plan(data['source'],data.get('target'))
        elif operation=='references':result=model.source_references(data['source'])
        elif operation=='isolated':result=model.isolated_filename(data['hero_root'],data['source_name'])
        elif operation=='diagnostic':result=modeldoc.diagnostic(data['output'])
        else:raise AssertionError(operation)
        case['expected']=result
    except Exception as error:case['error']=str(error)
    cases.append(case)
paths={'replacement':'models/new/body.dmx','part_a':'models/new/a.dmx','part_b':'models/new/b.dmx','π😀':'models/身体/a.dmx'}
base={'body':'replacement','body_lod1':model.DISABLE,'weapon':model.KEEP,'weapon_lod1':model.DISABLE}
add('analyze_standard','analyze',source=VMDL)
add('analyze_duplicates','analyze',source='{ children = [{_class="RenderMeshFile" name="body"}, {_class="RenderMeshFile" name="body"}] }')
add('keep_exact_bytes','patch',source=SOURCE,mappings={m['name']:model.KEEP for m in model.analyze(SOURCE)['meshes']},source_paths={})
add('disable_lods_bodygroup','patch',source=SOURCE,mappings=base,source_paths=paths)
add('split_disable_one_array','patch',source=SOURCE,mappings=dict(base,body=['part_a','part_b']),source_paths=paths)
add('disabled_body_choice_empty','patch',source=SOURCE,mappings=dict(base,body=model.DISABLE),source_paths=paths)
add('unicode_filters','patch',source=SOURCE,mappings=dict(base,body='π😀'),source_paths=paths)
for extra in ['{_class="UnknownGroup" meshes=["body_lod1"]}', '{_class="BodyGroupChoice" custom_mesh="body_lod1" meshes=[]}']:
    add('unknown_disabled_'+str(len(cases)),'patch',source=SOURCE.replace('children = [','children = ['+extra+',',1),mappings=base,source_paths=paths)
add('mixed_bodygroup','patch',source=SOURCE.replace('["body", "body_lod1"]','["body_lod1", {mesh_name="body"}]'),mappings=base,source_paths=paths)
add('missing_mesh_action','patch',source=SOURCE,mappings={'body':'replacement'},source_paths=paths)
add('unsafe_source_path','patch',source=SOURCE,mappings=base,source_paths=dict(paths,replacement='../outside.dmx'))
for kind in ['scalar_reference','unknown_array','nonarray_render']:
    if kind=='scalar_reference':s='{ mesh={_class="RenderMeshFile" name="body" filename="old.dmx"} ref={mesh_name="body"}}'
    elif kind=='unknown_array':s='{ meshes=[{_class="RenderMeshFile" name="body"}] unknown=["body"]}'
    else:s='{ mesh={_class="RenderMeshFile" name="body" filename="old.dmx"}}'
    add(kind,'patch',source=s,mappings={'body':['part_a','part_b']},source_paths=paths)
add('source_refs','references',source='{ f=resource:"models/body.vmdl" x="models/a.dmx" nested={ material="materials/a.vmat" graph="models/a.vnmgraph" not="HTTPS://unrelated" number=3 } }')
for strategy in ['reuse','single','disable','preserve','custom','choose']:
    add('suggest_'+strategy,'suggest',source=original(),replacements=['body_a','body_b','effect'],mappings={'body':['body_a','body_b'],'body_lod1':'effect'},strategy=strategy)
add('suggest_auto_off','suggest',source=original(),replacements=['body','effect'],mappings={},strategy='reuse',auto=False)
add('suggest_canonical_prefix','suggest',source=VMDL,replacements=['models/random/model.vmdl_c.body','weapon'])
for extra,label in [(ALL,'all'),('','no_all'),(ALL.replace('every_level','second_all'),'second')]:
    add('single_'+label,'merge',source=original(extra=extra),template=TEMPLATE,mappings=MAPPINGS,source_path='models/replacement.dmx',strategy='single')
add('single_all_split','merge',source=original(),template=TEMPLATE,mappings=dict(MAPPINGS,effect=['glow_a','glow_b']),source_path='models/replacement.dmx',strategy='single')
add('single_all_disabled','merge',source=original(),template=TEMPLATE,mappings=dict(MAPPINGS,effect=model.DISABLE),source_path='models/replacement.dmx',strategy='single')
add('single_other_children','merge',source=original(extra=ALL.replace('every_level','second_all'),between=ALL+', {_class="FutureLodMetadata" values=[1,2,3]},'),template=TEMPLATE,mappings=MAPPINGS,source_path='models/replacement.dmx',strategy='single')
for source_text,label in [(original().replace('switch_threshold = 0','switch_threshold = 5'),'bad_threshold'),(original().replace('_class = "LODGroupList"','_class="Other"'),'missing_lod_list')]:
    add(label,'merge',source=source_text,template=TEMPLATE,mappings=MAPPINGS,source_path='models/replacement.dmx',strategy='single')
add('aliases_hash_authorized','merge',source=source(export_names()),template=source(template=True),mappings={'body':'replacement'},source_path='models/replacement.dmx',strategy='reuse',original_report=authority())
add('aliases_no_authority','merge',source=source(export_names()),template=source(template=True),mappings={'body':'replacement'},source_path='models/replacement.dmx',strategy='reuse')
for mutation,label in [('hash','stale_hash'),('flags','wrong_flags'),('parent','bad_parent')]:
    report=authority();s=source(export_names())
    if mutation=='hash':report['compiled_sha256']='cd'*32
    if mutation=='flags':report['runtime_motion']['skeleton']['m_nFlag']=[0x40]*4
    if mutation=='parent':s=source(export_names(),[-1,0,-1,1])
    add(label,'merge',source=s,template=source(template=True),mappings={'body':'replacement'},source_path='models/replacement.dmx',strategy='reuse',original_report=report)
for version in [28,32,40,41]:
    for target in [None,40,41]:add(f'plan_{version}_{target}','plan',source=modeldoc.header(version)+'\n{rootNode={_class="RootNode" children=[{_class="Future" thing=#[00ff]}]}}',target=target)
add('wrong_guid','plan',source=modeldoc.header(28).replace('modeldoc28','modeldoc40')+'\n{rootNode={_class="RootNode" children=[]}}',target=40)
add('bad_root','plan',source=modeldoc.header(28)+'\n{rootNode=[]}',target=40)
add('bad_children','plan',source=modeldoc.header(28)+'\n{rootNode={_class="RootNode" children={}}}',target=40)
for output in ['ParticleFloatType_t\nMismatch - Member Count',"No valid format conversion from 'modeldoc41' to 'modeldoc40'",'WARNING: Unknown class NmSkeletonReference','NmSkeletonList class not registered\nAnimGraph2List class not registered','Unknown attribute x','Mesh is too large and uses GPU Animated Vertex Cache\nRenderMesh: body\nVertex Count: 612345','everything okay']:
    add('diagnostic_'+str(len(cases)),'diagnostic',output=output)
for name in ['body','models/foo.vmdl_c.body','π😀 very long '*8,'']:
    add('isolated_'+str(len(cases)),'isolated',hero_root='models/heroes/test',source_name=name)
# The reference preserves piece order after 99; lexical map sorting cannot drive cloning.
pieces=[f'piece{i}' for i in range(1,104)]
add('split_above_99_order','patch',source='{ meshes=[{_class="RenderMeshFile" name="body" filename="old.dmx"}] refs=[{mesh_name="body"}] }',mappings={'body':pieces},source_paths={p:f'models/{p}.dmx' for p in pieces})
for label,rows in [('weapon',[{'target':'weapon','action':'replace','sources':['replacement']}]),('hide_lods',[{'target':'weapon_lod1','action':'hide'}]),('conflict',[{'target':'weapon','action':'hide'},{'target':'weapon_lod1','action':'keep'}]),('all_hidden',[{'target':'body','action':'hide'},{'target':'weapon','action':'hide'}]),('missing_source',[{'target':'weapon','action':'replace','sources':['missing']}])]:
    state={'enabled':True,'equipment':{'enabled':True,'meshes':rows}}
    add('equipment_'+label,'equipment',source=SOURCE,mappings={m['name']:model.KEEP for m in model.analyze(SOURCE)['meshes']},state=state,source_names=['replacement'])
state={'enabled':True,'equipment':{'enabled':True,'meshes':[{'target':'weapon','action':'hide'}]}}
baseline={m['name']:model.KEEP for m in model.analyze(SOURCE)['meshes']}
effective=equipment.apply_mappings(model.analyze(SOURCE),baseline,state,[])
add('equipment_project_does_not_bake','equipment_project',source=SOURCE,baseline=baseline,effective=effective,state=state)
add('equipment_project_rejects_conflict','equipment_project',source=SOURCE,baseline=baseline,effective=dict(effective,weapon='replacement'),state=state)
path=Path(__file__).with_name('fixtures')/'reference.json';path.write_text(json.dumps({'cases':cases},indent=2,ensure_ascii=False)+'\n')
print(f'Wrote {len(cases)} model reference cases, including {sum("error" in c for c in cases)} rejection cases')
