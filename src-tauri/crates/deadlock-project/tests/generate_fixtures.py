"""Regenerate semantic schema/path goldens from the retained Python reference.

Development tool only; no Python is invoked by Cargo tests or application code.
"""
from dataclasses import asdict
from pathlib import Path
from tempfile import TemporaryDirectory
import json
import sys
sys.path.insert(0, str(Path(__file__).resolve().parents[4]))
import sys as _reference_sys
from pathlib import Path as _ReferencePath
_reference_sys.path.insert(0, str(next(p for p in _ReferencePath(__file__).resolve().parents if (p/'legacy/python/deadlock_builder').is_dir())/'legacy/python'))
from deadlock_builder.project import Project, Settings
from deadlock_builder import asset_paths
from deadlock_builder.safety import relative_asset
from deadlock_builder.customization.state import clear_targets

ROOT = '${ROOT}'
IDENTITY = '0123456789abcdef0123456789abcdef'
HERO = 'models/heroes_staging/hornet_v3/hornet.vmdl_c'

def map_strings(value, operation):
    if isinstance(value, str): return operation(value)
    if isinstance(value, list): return [map_strings(v, operation) for v in value]
    if isinstance(value, dict): return {k: map_strings(v, operation) for k,v in value.items()}
    return value

files = ['old/package/Collection.dmx','old/package/modèle with spaces.GLB',
         'old/package/source_materials/models/body/color.png','old/package/local.png','old/package/mesh.vmdl',
         'new/package/Collection.dmx','new/package/source_materials/models/body/color.png',
         'new/package/local.png','new/package/mesh.vmdl', 'profiles/case/profile.json']
base = dict(id=IDENTITY, original_model=HERO, name='vindicta', hero_name='Vindicta',
            dmx=ROOT+'/old/package/Collection.dmx', textures=ROOT+'/old/package/source_materials',
            mesh_template=ROOT+'/old/package/mesh.vmdl',
            meshes={'body':['body_01','body_02'],'gun':'gun','hat':'@keep','extra':'@disable'},
            materials={'models/replacement/body.vmat':{'mode':'generate','confirmed':True,
                'roughness':None,'metalness':0.4,'color':[0.5,0.2,1],
                'textures':{'color':ROOT+'/old/package/source_materials/models/body/color.png',
                            'normal':ROOT+'/old/package/local.png','opacity':''},
                'candidates':{'color':['old']},'ambiguous':[], 'existing':True,
                'future_material':{'x':None}}})
custom = {'schema':1,'enabled':True,'model_enabled':True,'hero_path':HERO,
    'icons':[{'enabled':True,'source':ROOT+'/old/package/local.png','target':'panorama/vindicta.vtex_c','fit':'contain','original_sha256':'f'*64}],
    'animations':[{'source':ROOT+'/old/package/pose.vnmclip','target':'animations/vindicta.vnmclip_c'}],
    'effects':[{'source':ROOT+'/old/package/shine.vpcf_c','target':'particles/vindicta.vpcf_c','allow_shared':True}],
    'audio':[{'source':ROOT+'/old/package/sound.wav','target':'sounds/vindicta.vsnd_c'}],
    'clothing':{'enabled':False,'mode':'simulated','source':ROOT+'/old/package/skirt.glb'},
    'physics':{'enabled':True,'source':ROOT+'/old/package/test.vmdl_c','hero_path':HERO,'sha256':'f'*64},
    'cloth_setup':{'meshes':['skirt'],'attachment':'top','collision':'legs'},
    'appearance':{'enabled':True,'materials':[{'target':'models/replacement/body.vmat','color':[0.2,0.3,0.4],'emission':2}]},
    'equipment':{'enabled':True,'meshes':[{'target':'gun','action':'replace','sources':['newgun'],'include_lods':False}],
        'props':[{'enabled':True,'source':ROOT+'/old/package/prop.glb','bone':'hand','translation':[1,2,3],'rotation':[0,90,0],'scale':0.25}],
        'variants':[{'enabled':False,'model':'models/alternate.vmdl_c','project':ROOT+'/old/package/alternate.dmb.json'}]},
    'face':{'enabled':True,'mode':'static','weights':{'smile':0.2}},'future_section':{'unset':None,'paths_are_opaque':'do/not/rebase'}}
cases = [
 {'name':'empty_draft','operation':'save','project':{'id':IDENTITY},'path':'draft.json'},
 {'name':'vindicta_saved','operation':'save','project':base,'path':'old/package/project.dmb.json'},
 {'name':'all_sources_and_customization','operation':'save','project':dict(base,customization=custom,preparation_source=ROOT+'/old/package/modèle with spaces.GLB',preparation_options={'rotation':[0,0,90],'roles':{'head':'Head'},'landmarks':{'head':[]},'reviewed':False,'unknown':None}),'path':'old/package/project.dmb.json'},
 {'name':'changed_model_rebases_existing_old','operation':'rebase','project':base,'initial_rebase':True,'changes':{'dmx':ROOT+'/new/package/Collection.dmx'}},
 {'name':'changed_textures_missing_new_remains_new','operation':'rebase','project':base,'initial_rebase':True,'changes':{'textures':ROOT+'/custom-textures'}},
 {'name':'windows_absolute_recovered','operation':'rebase','project':dict(base,dmx=ROOT+'/new/package/Collection.dmx',textures=ROOT+'/new/package/source_materials',materials={'a.vmat':{'textures':{'color':r'C:\Users\name\package\source_materials\models\body\color.png'}}})},
 {'name':'missing_basename_no_recovery','operation':'rebase','project':dict(base,materials={'a.vmat':{'textures':{'color':'/unrelated/color.png'}}})},
 {'name':'legacy_top_paths_recovery','operation':'load','project':dict(base,dmx='/gone/package/Collection.dmx',textures='/gone/package/source_materials',mesh_template='/gone/package/mesh.vmdl',extra_root_key={'ignored':True}),'path':'new/package/profile.json'},
 {'name':'relative_load_sources','operation':'load','project':dict(base,dmx='Collection.dmx',textures='source_materials',source_root='.',mesh_template='mesh.vmdl',materials={'a.vmat':{'textures':{'color':'@textures/models/body/color.png','normal':'@source/local.png','opacity':''}}},customization=map_strings(custom, lambda v:v.replace(ROOT+'/old/package/',''))),'path':'new/package/profile.json'},
 {'name':'relative_dmx_parent_dot','operation':'rebase','project':{'id':IDENTITY,'dmx':'Collection.dmx'}},
 {'name':'clear_hero_targets','operation':'clear_targets','customization':custom},
 {'name':'settings_legacy_unknown','operation':'settings','settings':{'cswin':'C:/Tools/CSWin64','resourcecompiler':'C:/Tools/材料/compiler.exe','unknown':'discard'}},
]
invalid = [
 {'name':'empty_action','project':{'id':IDENTITY},'action':True},
 *[{'name':'unsafe_'+str(i),'project':{'id':IDENTITY,'original_model':s}} for i,s in enumerate(['../escape.vmdl','/absolute.vmdl','C:/absolute.vmdl',r'models\..\escape.vmdl_c','models//hero.vmdl','models/./hero.vmdl','models/hero\n.vmdl','models/hero.vmat',' '])],
 *[{'name':'field_'+key,'project':{'id':IDENTITY,key:value}} for key,value in {'schema':2,'name':'invalid name','model_input_mode':'unknown','vpk_number':0,'lod_strategy':'unknown','ag2_mode':'unknown','animation_policy':'unknown','split_target':524288,'auto_split':'yes','meshes':{'body':[]}}.items()],
 {'name':'split_name_collision','project':{'id':IDENTITY,'meshes':{'body':['one','two'],'body__part01':'three'}}},
 {'name':'duplicate_unicode_targets','project':{'id':IDENTITY,'customization':{'icons':[{'target':'icons/Straße.vtex_c'},{'target':'icons/STRASSE.vtex_c'}]}}},
 {'name':'bad_bool','project':{'id':IDENTITY,'customization':{'enabled':'yes'}}},
 {'name':'bad_appearance_number','project':{'id':IDENTITY,'customization':{'appearance':{'materials':[{'roughness':True}]}}}},
 {'name':'bad_prop_scale','project':{'id':IDENTITY,'customization':{'equipment':{'props':[{'scale':0}]}}}},
 {'name':'token_traversal','operation':'rebase','project':dict(base,materials={'a.vmat':{'textures':{'color':'@textures/../secret.png'}}})},
]
with TemporaryDirectory() as temp:
    root=Path(temp).resolve()
    for f in files:
        p=root/f;p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes(b'fixture')
    decode=lambda v:map_strings(v,lambda s:s.replace(ROOT,str(root)))
    normalize=lambda v:map_strings(v,lambda s:s.replace(str(root),ROOT))
    for c in cases:
        data=decode(c)
        if c['operation']=='clear_targets': output=clear_targets(data['customization'])
        elif c['operation']=='settings':
            path=root/'settings.json';path.write_text(json.dumps(data['settings']));output=asdict(Settings.load(path))
        else:
            p=Project(**{k:v for k,v in data['project'].items() if k in Project.__dataclass_fields__})
            if c['operation']=='save':
                path=root/data['path'];p.save(path);output=json.loads(path.read_text())
            elif c['operation']=='load':
                path=root/data['path'];path.parent.mkdir(parents=True,exist_ok=True);path.write_text(json.dumps(data['project']));output=asdict(Project.load(path))
            else:
                if c.get('initial_rebase'):p.rebase_assets()
                for k,v in data.get('changes',{}).items():setattr(p,k,v)
                p.rebase_assets();output=asdict(p)
        c['expected']=normalize(output)
    for c in invalid:
        try:
            data=decode(c);p=Project(**data['project'])
            p.rebase_assets() if c.get('operation')=='rebase' else p.validate(require_original=c.get('action',False))
        except Exception as error:c['error']=str(error)
        else:raise AssertionError('Expected invalid: '+c['name'])
path_cases=[]
for op,args in [('relative_asset',[r'models\hero.vmdl']),('relative_to',[r'C:\TOOLS\package\Color.png',r'c:\tools\package']),('relative_to',['C:/straße/body.png','C:/STRASSE']),('relative_to',['C:/same','C:/same']),('absolute',[r'C:\Tools\hero.vmdl']),('absolute',['C:relative']),('absolute',[r'\\server\share\file']),('relative_to',['/same','/same'])]:
    path_cases.append({'operation':op,'arguments':args,'expected':(relative_asset if op == 'relative_asset' else getattr(asset_paths,op))(*args)})
output={'reference':'deadlock_builder.project + asset_paths + customization.state, schema 1','platform':'posix','files':files,'cases':cases,'invalid':invalid,'path_cases':path_cases}
Path(__file__).with_name('fixtures').joinpath('reference.json').write_text(json.dumps(output,ensure_ascii=False,indent=2)+'\n')
print(f'Wrote {len(cases)} reference cases, {len(invalid)} invalid cases, {len(path_cases)} path cases')
