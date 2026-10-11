"""Python oracle for texture contracts and image metadata; application never calls this."""
from pathlib import Path
import sys, json, hashlib, struct
from io import BytesIO
ROOT=Path(__file__).resolve().parents[4]
sys.path.insert(0,str(ROOT))
from PIL import Image, ImageCms
import sys as _reference_sys
from pathlib import Path as _ReferencePath
_reference_sys.path.insert(0, str(next(p for p in _ReferencePath(__file__).resolve().parents if (p/'legacy/python/deadlock_builder').is_dir())/'legacy/python'))
from deadlock_builder.customization import icons
from tests.test_customization_icons import texture,png
from lz4.block import compress
OUT=Path(__file__).with_name('fixtures')/'icons';OUT.mkdir(parents=True,exist_ok=True)
display=struct.pack('<HHHh8f',0,50,40,0,*([0.0]*8))
compressed=compress(bytes(64*64*4),store_size=False)
cases={
 'embedded':texture(flags=11|128|256), 'gpu':texture(fmt=4),
 'padded':texture(extras=[(3,display)],payload=png(50,40)),
 'one_pixel':texture(4,4,extras=[(3,struct.pack('<HHHh8f',0,1,1,0,*([0.0]*8)))],payload=png(1,1)),
 'compressed':texture(fmt=4,extras=[(4,struct.pack('<4I',1,8,1,len(compressed)))],payload=compressed),
 'bad_lz4':texture(fmt=4,extras=[(4,struct.pack('<4I',1,8,1,len(compressed)))],payload=b'\xff'*len(compressed)),
 'gpu_truncated':texture(fmt=4)[:-1], 'cube':texture(flags=16), 'array':texture(depth=2),
 'sprite':texture(extras=[(2,b'sheet')]), 'custom_mips':texture(fmt=4,mips=2),
 'unknown_flags':texture(fmt=4,flags=128), 'unknown_format':texture(fmt=17), 'png_tail':texture()+b'unknown tail',
 'gpu_padded':texture(fmt=4,extras=[(3,display)]), 'nonfinite':texture(extras=[(3,struct.pack('<HHHh8f',0,64,64,0,float('nan'),*([0.0]*7)))]),
}
expected={}
for name,raw in cases.items():
 (OUT/(name+'.vtex_c')).write_bytes(raw)
 try:
  meta=icons.texture_metadata(raw);method=icons._method(meta,raw)
  expected[name]={'metadata':meta,'method':method}
 except Exception as ex: expected[name]={'rejected':True,'reason':str(ex)}
(OUT/'textures.json').write_text(json.dumps(expected,indent=2)+'\n')
images={}
def save_image(name,image,fmt='PNG',**options):
 stream=BytesIO();image.save(stream,fmt,**options);raw=stream.getvalue();(OUT/name).write_bytes(raw)
 try:
  decoded,info=icons._image(raw);info['first_pixel']=list(decoded.getpixel((0,0)));info['last_pixel']=list(decoded.getpixel((decoded.width-1,decoded.height-1)));images[name]=info
 except Exception as ex: images[name]={'rejected':True,'reason':str(ex)}
save_image('source.png',Image.new('RGBA',(100,50),(255,64,32,128)))
profile=ImageCms.ImageCmsProfile(ImageCms.createProfile('sRGB')).tobytes()
save_image('profile.png',Image.new('RGBA',(64,64),(40,80,120,90)),icc_profile=profile)
save_image('bad_profile.png',Image.new('RGB',(24,24)),icc_profile=b'broken color profile')
save_image('cmyk.jpg',Image.new('CMYK',(12,8),(20,40,80,10)),'JPEG')
for fmt,suffix in [('JPEG','jpg'),('PNG','png')]:
 exif=Image.Exif();exif[274]=6
 save_image('oriented.'+suffix,Image.new('RGB',(12,8),(24,50,80)),fmt,exif=exif)
save_image('gray.jpg',Image.new('L',(12,8),90),'JPEG')
save_image('animated.png',Image.new('RGBA',(16,16),'red'),save_all=True,append_images=[Image.new('RGBA',(16,16),'blue')],duration=100)
(OUT/'images.json').write_text(json.dumps(images,indent=2)+'\n')
meta=icons.texture_metadata(cases['gpu'])
(OUT/'recipe.vtex').write_text(icons.texture_source('panorama/images/dmb_sources/example.png',meta))
(OUT/'provenance.json').write_text(json.dumps({'python_source_sha256':hashlib.sha256((ROOT/'legacy/python/deadlock_builder/customization/icons.py').read_bytes()).hexdigest(),'generator':'generate_icon_fixtures.py','native_compiler_executed':False},indent=2)+'\n')
