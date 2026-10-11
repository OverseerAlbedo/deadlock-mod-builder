"""Development-only reference generation, never required by application/Cargo."""
import sys,json,copy
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[4]))
import sys as _reference_sys
from pathlib import Path as _ReferencePath
_reference_sys.path.insert(0, str(next(p for p in _ReferencePath(__file__).resolve().parents if (p/'legacy/python/deadlock_builder').is_dir())/'legacy/python'))
from tests.preparation_fixture import humanoid,skeleton,mesh_report
from deadlock_builder.preparation.blender_scripts.algorithms import identify,suggest_mapping,name_role
from deadlock_builder.preparation.review import review_issues
from deadlock_builder.preparation.target_skeleton import prepare_target
cases=[]
def add(label,operation,**args):
 c=dict(name=label,operation=operation,args=copy.deepcopy(args))
 try:
  if operation=='identify':v=identify(args['bones'],args['overrides'])
  elif operation=='suggest':v=suggest_mapping(args['source'],args['target'])
  elif operation=='review':v=review_issues(args['analysis'],args['mode'],args['options'])
  elif operation=='name':v=name_role(args['name'])
  c['expected']=v
 except Exception as e:c['error']=str(e)
 cases.append(c)
for style in ('target','mixamo','bip'):add(style,'suggest',source=humanoid(style),target=humanoid())
for label,mutate in [('ambiguous',lambda b:b.append(dict(b[1],name='Spine1'))),('foot_overlap',lambda b:b[-1].update(position=b[-4]['position'])),('hierarchy',lambda b:b[4].update(parent=b[-1]['name'])),('cycle',lambda b:b[0].update(parent='head')),('duplicate',lambda b:b.append(b[0])),('missing_parent',lambda b:b[0].update(parent='missing'))]:
 b=humanoid();mutate(b);add(label,'identify',bones=b,overrides={})
b=humanoid();b.append(dict(b[1],name='Spine1'));add('explicit_override','identify',bones=b,overrides={'spine':'spine'})
for n in ['Bip01 L ForeArm','mixamorig:RightArm','ns:左骨','arm.left','Spine03','left_hip','Bip２ Left Hand','Head']:
 add('name_'+n,'name',name=n)
a={'target':humanoid(),'analysis':{'bones':humanoid('mixamo'),'weighted_bones':[b['name']for b in humanoid('mixamo')],'suggested_mode':'rigged_humanoid'}}
s=suggest_mapping(a['analysis']['bones'],a['target']);good={'target_roles':s['target']['roles'],'source_roles':s['source']['roles'],'mapping':s['mapping']}
for label,mode,options in [('empty','rigged_humanoid',{}),('wrongmode','unrigged_humanoid',good),('complete','rigged_humanoid',good),('conflict','rigged_humanoid',dict(good,mapping={}))]:add('review_'+label,'review',analysis=a,mode=mode,options=options)
a['analysis']['suggested_mode']='unrigged_humanoid';a['analysis']['bones']=[];add('landmarks_missing','review',analysis=a,mode='unrigged_humanoid',options={'target_roles':good['target_roles']})
a['analysis']['weighted_bones']=[]
fixture={'cases':cases,'skeleton':skeleton(),'target':prepare_target(skeleton())[0],'scale_audit':prepare_target(skeleton())[1],'report':mesh_report(),'inspection':{'status':'ok','armatures':1,'bones':humanoid('mixamo'),'vertices':3,'bounds':[[0,0,1],[1,0,2]],'meshes':[{'name':'body','vertices':3,'polygons':1,'materials':['models/test/materials/cloth.vmat'],'disconnected_components':1}],'morphs':[],'weighted_bones':['mixamorig:Hips'],'suggested_mode':'rigged_humanoid','humanoid':identify(humanoid('mixamo'))}}
Path(__file__).with_name('fixtures').joinpath('reference.json').write_text(json.dumps(fixture,ensure_ascii=False,indent=2),encoding='utf8');print(len(cases),'algorithm/review cases')
