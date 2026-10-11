"""Development-only differential fixtures for outfit choices."""
from dataclasses import asdict
from pathlib import Path
import json,sys,copy
sys.path.insert(0,str(Path(__file__).resolve().parents[4]))
import sys as _reference_sys
from pathlib import Path as _ReferencePath
_reference_sys.path.insert(0, str(next(p for p in _ReferencePath(__file__).resolve().parents if (p/'legacy/python/deadlock_builder').is_dir())/'legacy/python'))
from deadlock_builder.project import Project
from deadlock_builder.customization import presets
p=Project(id='a'*32,original_model='models/hero.vmdl_c',dmx='/source/body.dmx',preparation_report='/source/report.json',preparation_blend='/source/prepared.blend',preparation_options={'reviewed':True},review_lods=True,install=False,vpk_number=9,meshes={'body':'body'},customization={'enabled':True,'appearance':{'enabled':True,'materials':[{'target':'materials/body.vmat','roughness':0.5}]},'equipment':{'enabled':True,'meshes':[],'props':[]}})
cases=[]
for label,mutate in [('same',lambda c:None),('appearance',lambda c:c['customization']['appearance']['materials'][0].update(roughness=.2)),('dmx',lambda c:c.update(dmx='/new/body.dmx')),('clothing',lambda c:c['customization'].update(clothing={'enabled':True,'source':'/source/skirt.glb','mode':'weighted'})),('prop',lambda c:c['customization']['equipment']['props'].append({'source':'/source/hat.glb','bone':'head'})),('disabled_prop',lambda c:c['customization']['equipment']['props'].append({'enabled':False,'source':'/source/hat.glb'})),('face',lambda c:c['customization'].update(face={'enabled':True,'weights':{'smile':.8}})),('equipment',lambda c:c['customization']['equipment']['meshes'].append({'target':'hat','action':'hide'}))]:
    preset=presets.capture(p);mutate(preset['choices']);cases.append({'name':label,'project':asdict(p),'preset':preset,'expected':asdict(presets.apply(p,preset))})
preset=presets.capture(p)
invalid=[]
for label,mutate in [('wrong_hero',lambda v:v.update(original_model='models/other.vmdl')),('unknown_execution',lambda v:v['choices'].update(install=True)),('missing_choice',lambda v:v['choices'].pop('dmx')),('unknown_root',lambda v:v.update(command='do something')),('schema',lambda v:v.update(schema=2)),('name',lambda v:v.update(name=''))]:
    value=copy.deepcopy(preset);mutate(value)
    try:presets.apply(p,value)
    except Exception as e:invalid.append({'name':label,'project':asdict(p),'preset':value,'error':str(e)})
    else:raise AssertionError(label)
Path(__file__).with_name('fixtures').joinpath('presets.json').write_text(json.dumps({'cases':cases,'invalid':invalid},indent=2)+'\n')
print(f'{len(cases)} outfit reference applications; {len(invalid)} rejections')
