"""Development-only reference fixtures; never invoked by the Rust application."""
import sys,json
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[4]))
import sys as _reference_sys
from pathlib import Path as _ReferencePath
_reference_sys.path.insert(0, str(next(p for p in _ReferencePath(__file__).resolve().parents if (p/'legacy/python/deadlock_builder').is_dir())/'legacy/python'))
from deadlock_builder import ag2,modeldoc
header=modeldoc.header(28)
source=header+'\n{ rootNode = { _class = "RootNode" children = [ { _class = "RenderMeshList" children = [ { _class="RenderMeshFile" name="body" filename="body.dmx" } ] }, { _class="AnimationList" mystery={disabled=false} children=[] }, { _class="AnimGraph" disabled=false }, { _class="EmptyAnimGraph" disabled=true } ] } }'
refs=['models/a.vnmskel_c','models/a.vnmgraph_c','models/ui.vnmgraph_c']
data='stats\n{ m_vecNmSkeletonRefs=[resource:"models/a.vnmskel"] m_animGraph2Refs=[{m_sIdentifier="default" m_hGraph=resource:"models/a.vnmgraph"},{m_sIdentifier="UI 雪😀" m_hGraph=resource:"models/ui.vnmgraph"}] }'
nodes=ag2.from_data_dump(data)
cases=[]
def add(name,operation,**args):
 case=dict(name=name,operation=operation,args=args)
 try:
  if operation=='data':out=ag2.from_data_dump(args['text'])
  if operation=='reconcile':out=ag2.reconcile(args['text'],args['refs'],args['recovered'])
  if operation=='compiler':out=ag2.compiler_source(args['text'],args['policy'],args['target'])
  if operation=='preview':out=ag2.preview_source(args['text'])
  if operation=='source':out=ag2.source_nodes(args['text'])
  case['expected']=out
 except Exception as error:case['error']=str(error)
 cases.append(case)
add('roles_unicode','data',text=data)
add('empty_data','data',text='header stats\n { unrelated=123 }')
add('duplicate_default','data',text='{m_animGraph2Refs=[{m_hGraph="a.vnmgraph"},{m_sIdentifier="default" m_hGraph="b.vnmgraph"}]}')
add('bad_skeleton_layout','data',text='{m_vecNmSkeletonRefs="bad"}')
add('missing_graph_path','data',text='{m_animGraph2Refs=[{}]}')
add('no_data','data',text='error')
add('recover_lists','reconcile',text=source,refs=refs,recovered=nodes)
recovered=ag2.reconcile(source,refs,nodes)[0]
add('exact_unchanged','reconcile',text=recovered,refs=refs,recovered=nodes)
partial=ag2.reconcile(source,refs[:1],nodes)[0]
add('existing_lists','reconcile',text=partial,refs=refs,recovered=nodes)
add('stale_source','reconcile',text=recovered,refs=refs[:1],recovered=nodes)
add('no_roles_guess','reconcile',text=source,refs=refs,recovered=[])
add('unknown_original','reconcile',text=recovered,refs=None,recovered=[])
add('remove_preview','preview',text=recovered)
add('preserve_compiler','compiler',text=recovered,policy='preserve',target=41)
add('disable_compiler','compiler',text=recovered,policy='disable_legacy',target=41)
add('bad_header','compiler',text=source.split('\n',1)[1],policy='preserve',target=41)
add('wrong_extension','source',text='{children=[{_class="AnimGraph2" filename="a.vnmskel"}]}')
add('typed_source','source',text='{children=[{_class="AnimGraph2" filename=resource:"models/u.vnmgraph_c" name="ui"}]}')
Path(__file__).with_name('fixtures').joinpath('ag2.json').write_text(json.dumps(cases,ensure_ascii=False,indent=2),encoding='utf8')
print(len(cases),'AG2 cases')
