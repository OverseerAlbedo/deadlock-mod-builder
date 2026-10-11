"""Development-only goldens from the reviewed Python implementation."""
from pathlib import Path
import sys,json,copy
ROOT=Path(__file__).resolve().parents[4]
sys.path.insert(0,str(ROOT))
import sys as _reference_sys
from pathlib import Path as _ReferencePath
_reference_sys.path.insert(0, str(next(p for p in _ReferencePath(__file__).resolve().parents if (p/'legacy/python/deadlock_builder').is_dir())/'legacy/python'))
from tests.test_customization_resources import sound,wave_data,particle,MODEL,SOUND,PARTICLE
from tests.test_customization_motion import clip_data,compiled as clip,SKELETON,TARGET,MODEL as CLIPMODEL
from tests.fixtures import vpk,resource
from deadlock_builder.customization import audio,resources,motion,icons,variants
from deadlock_builder.vpk import Vpk
HERE=Path(__file__).parent/'fixtures'
HERE.mkdir(exist_ok=True)
def save(name,data):
    (HERE/name).write_bytes(data)
def report(name,value):
    (HERE/name).write_text(json.dumps(value,ensure_ascii=False,sort_keys=True,indent=2)+'\n')
for ctrl in [False,True]:
    original=sound(ctrl=ctrl);name='sound-ctrl' if ctrl else 'sound-data'
    save(name+'.bin',original);report(name+'.json',audio.metadata(original))
    save(name+'-replaced.bin',audio.wav_replacement(original,wave_data()))
save('samples.wav',wave_data());save('bad-rate.wav',wave_data(rate=16000));save('bad-loop.bin',sound(loop=2))
old=particle();new=resources._particle(old)[3];new['m_ConstantColor']=[255,0,0,255]
save('particle.bin',old);save('particle-replaced.bin',particle(new));report('particle.json',resources.validate_effect(old,particle(new)))
bad=copy.deepcopy(new);bad['m_Initializers'][0]['m_nControlPointNumber']=2;save('particle-bad-control.bin',particle(bad))
save('clip.bin',clip());new=clip_data();new['m_compressedPoseData']=bytes([1,0]*6);save('clip-replaced.bin',clip(new));report('clip.json',motion.validate_clip(clip(),clip(new)))
bad=copy.deepcopy(new);bad['m_rootMotion']={'m_transforms':[1]};save('clip-bad-root.bin',clip(bad))
bad=copy.deepcopy(new);bad['m_compressedPoseOffsets']=[0,1];save('clip-bad-offset.bin',clip(bad))
shared='sounds/shared/gun.vsnd_c'
files={MODEL+'_c':resource([shared[:-2]]),SOUND:sound(),PARTICLE:old,shared:sound(),CLIPMODEL:resource([SKELETON]),TARGET:clip(),SKELETON+'_c':resource()}
vpk(HERE/'components_dir.vpk',files)
archive=Vpk(HERE/'components_dir.vpk')
report('audio-catalog.json',resources.discover(archive,MODEL,kind='audio'))
report('hero-tokens.json',sorted(icons._hero_tokens(MODEL)))
report('animations.json',motion.discover(archive,CLIPMODEL))
from tests.fixtures import dmx
save('animation.dmx',dmx().replace(b'DmeJoint\0',b'DmeChannelsClip\0'))
save('action.vnmclip',('{ m_sourceFilename = "animation.dmx" m_animationSkeletonName = "'+SKELETON+'" m_eventTracks = [] }').encode())
import struct
rot=clip_data();rot['m_trackCompressionSettings'][0]['m_bIsRotationStatic']=False;rot['m_trackCompressionSettings'][0]['m_bIsTranslationStatic']=True
rot['m_compressedPoseData']=struct.pack('<6H',*([16383]*6));save('clip-rotation.bin',clip(rot))
rot['m_compressedPoseData']=struct.pack('<6H',*([32767]*6));save('clip-bad-rotation.bin',clip(rot))
variant_files={'models/heroes_wip/werewolf/werewolf.vmdl_c':resource(),'models/heroes_wip/werewolf/transform/werewolf_transform.vmdl_c':resource(),'models/heroes_wip/ratking/ratking.vmdl_c':resource()}
vpk(HERE/'variants_dir.vpk',variant_files)
report('variants.json',variants.discover_variants(Vpk(HERE/'variants_dir.vpk'),'models/heroes_wip/werewolf/werewolf.vmdl'))
