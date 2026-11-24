"""Recover image references from materials; never bake or rewrite shader graphs.

Direct Principled links, passthrough groups/reroutes and connected MMD base
textures are preferred. Without a Principled BSDF, an image feeding the
material output or another shader's color (Diffuse, Emission, Toon ...) is the
base color, also through a color mix/tint with a single image input. Complex
graphs use explicit image channel suffixes; a single remaining unsuffixed image
is the base color. Multiple images remain candidates for review rather than an
arbitrary choice.

An image the model names under another extension (staged_images.py stand-ins)
is used like any connected image, with a note; several such files are offered
as candidates. A material the model gives no base-color image takes an image
beside the model whose file name matches its name (chosen at staging,
texture_names.rs) only when that image fits no other such material; otherwise
the matches are candidates. These picks stay unconfirmed, like every other.
"""
import hashlib
from pathlib import Path
import re
from readable_names import readable
import staged_images


# Readable characters of a prepared material's name. The native compiler names
# the textures it makes after the material (about 40 characters more) and
# Windows paths end at 259 characters, so the readable part stays short; the
# hash of the full name is the material's identity (earlier choices follow it).
NAME_CHARS = 24


def prepared_slug(name):
    """File name of a prepared material: its readable name (non-English letters
    spelled in Latin letters), cut to NAME_CHARS, and the hash of the full name."""
    return re.sub('[^a-zA-Z0-9_-]','_',readable(name)).lower()[:NAME_CHARS]+'_'+hashlib.sha256(name.encode()).hexdigest()[:8]

CHANNELS = [('Base Color','color'),('Roughness','roughness'),('Metallic','metalness'),
            ('Normal','normal'),('Emission Color','emission'),('Alpha','opacity')]
EXTENSIONS = {'PNG':'.png','JPEG':'.jpg','TARGA':'.tga','TARGA_RAW':'.tga',
              'TIFF':'.tif','OPEN_EXR':'.exr'}
ALIASES = {'d':'color','diff':'color','diffuse':'color','albedo':'color','basecolor':'color','color':'color',
           'colour':'color','col':'color','bc':'color','base':'color',
           'n':'normal','normal':'normal','norm':'normal','nrm':'normal','nor':'normal','normalmap':'normal',
           'rough':'roughness','roughness':'roughness','rgh':'roughness',
           'metal':'metalness','metallic':'metalness','metalness':'metalness',
           'ao':'ao','occlusion':'ao','ambientocclusion':'ao',
           'emission':'emission','emissive':'emission','selfillum':'emission','emit':'emission','glow':'emission',
           'alpha':'opacity','opacity':'opacity'}
# Shaders whose 'Color' input is the surface color when no Principled BSDF exists.
COLOR_SHADERS = {'BSDF_DIFFUSE','EMISSION','BSDF_TOON','SUBSURFACE_SCATTERING','BSDF_TRANSLUCENT','BACKGROUND'}
# MMD (mmd_tools) toon ramps and sphere (shine) maps shade the base texture
# inside the MMD shader; they are never the base color. Followed through that
# shader's color mixes, the sphere map used to win over the base texture.
MMD_SHADING_NODE = re.compile(r'mmd_(?:toon|sphere)_tex(?:\.\d+)?')


# Face layers an MMD model keeps fully transparent until an expression fades
# them in (blush, tears ...): a game build has no expressions, so they are
# hidden unless the user shows them (Appearance > Face details). Recorded as
# the material's 'face_layer'. One model's 赤面/照れ blush and 涙 tears were drawn
# solid and turned the face red.
FACE_LAYER_WORDS = (('blush', ('赤面','照れ','頬','チーク','blush','cheek','紅潮')),
                    ('tears', ('涙','泪','tear','namida')))


def source_hidden(mat, nodes):
    """Whether the source shows this material fully transparent: the MMD
    shader's Alpha (mmd_tools), else a single Principled BSDF's Alpha, else the
    material's viewport alpha; an Alpha fed by a link is not a constant."""
    for node,_ in nodes:
        if node.type=='GROUP' and node.name.startswith('mmd_shader') and node.inputs.get('Alpha') is not None:
            inp=node.inputs['Alpha']
            return not inp.is_linked and inp.default_value<=0.001
    principled=[n for n,_ in nodes if n.type=='BSDF_PRINCIPLED']
    if len(principled)==1 and principled[0].inputs.get('Alpha') is not None:
        inp=principled[0].inputs['Alpha']
        return not inp.is_linked and inp.default_value<=0.001
    return mat.diffuse_color[3]<=0.001


def face_layer(name):
    lowered=name.lower()
    return next((kind for kind,words in FACE_LAYER_WORDS if any(w in lowered for w in words)),'other')


def corresponding(sockets, source):
    return next((s for s in sockets if s.identifier == source.identifier),None) or sockets.get(source.name)


def upstream(socket, context):
    """Resolve group boundaries without mixing inputs from other instances."""
    node=socket.node
    if node.mute:return []
    if node.type=='GROUP' and node.node_tree:
        outputs=[n for n in node.node_tree.nodes if n.type=='GROUP_OUTPUT' and n.is_active_output]
        inputs=[corresponding(n.inputs,socket) for n in outputs]
        return [(link.from_socket,context+(node,)) for inp in inputs if inp for link in inp.links]
    if node.type=='GROUP_INPUT' and context:
        inp=corresponding(context[-1].inputs,socket)
        return [(link.from_socket,context[:-1]) for link in inp.links] if inp else []
    return [(link.from_socket,context) for inp in node.inputs for link in inp.links]


def connected_nodes(tree):
    outputs=[n for n in tree.nodes if n.type=='OUTPUT_MATERIAL' and n.is_active_output]
    pending=[(link.from_socket,(),0) for n in outputs for inp in [n.inputs.get('Surface')] if inp for link in inp.links]
    seen=set();nodes={}
    while pending:
        socket,context,depth=pending.pop()
        key=(socket.as_pointer(),tuple(n.as_pointer() for n in context))
        if key in seen or depth>64:continue
        if len(seen)>10000:raise ValueError('Material node graph exceeds the texture recovery limit.')
        seen.add(key)
        if socket.node.mute:continue
        nodes[(socket.node.as_pointer(),key[1])]=(socket.node,context)
        pending.extend((s,c,depth+1) for s,c in upstream(socket,context))
    return list(nodes.values())


def direct_image(inp, context, channel, seen=None):
    if not inp or len(inp.links)!=1:return None
    socket=inp.links[0].from_socket;node=socket.node
    seen=set() if seen is None else seen
    key=(socket.as_pointer(),tuple(n.as_pointer() for n in context))
    if key in seen or len(seen)>64 or node.mute:return None
    seen.add(key)
    if node.type=='TEX_IMAGE':
        return None if MMD_SHADING_NODE.fullmatch(node.name.lower()) else node.image
    if node.type=='REROUTE':return direct_image(node.inputs[0],context,channel,seen)
    if channel=='normal' and node.type=='NORMAL_MAP':
        return direct_image(node.inputs.get('Color'),context,channel,seen)
    if node.type=='GROUP' and node.node_tree:
        outputs=[n for n in node.node_tree.nodes if n.type=='GROUP_OUTPUT' and n.is_active_output]
        if len(outputs)==1:
            return direct_image(corresponding(outputs[0].inputs,socket),context+(node,),channel,seen)
    if node.type=='GROUP_INPUT' and context:
        return direct_image(corresponding(context[-1].inputs,socket),context[:-1],channel,seen)
    if channel=='color' and node.type in ('MIX','MIX_RGB'):
        # A tint or multiply over one image keeps that image as the base color.
        found={}
        for item in node.inputs:
            if item.type=='RGBA' and item.enabled and item.is_linked:
                image=direct_image(item,context,channel,seen)
                if image:found[image.as_pointer()]=image
        return next(iter(found.values())) if len(found)==1 else None
    return None


def output_surfaces(tree):
    return [n.inputs.get('Surface') for n in tree.nodes if n.type=='OUTPUT_MATERIAL' and n.is_active_output]


def image_stem(image):
    return Path(image.filepath.replace('\\','/')).stem if image.filepath else Path(image.name).stem


def image_channel(image):
    suffix=re.split(r'[_ .-]+',image_stem(image).lower())[-1]
    return ALIASES.get(suffix)


def copy_image(image, folder, slug, channel):
    import bpy
    ext=EXTENSIONS.get(image.file_format)
    if not ext:return None
    if image.packed_file:
        data=bytes(image.packed_file.data)
    else:
        source=Path(bpy.path.abspath(image.filepath,library=image.library))
        if not source.is_file():return None
        data=source.read_bytes()
    if not data:return None
    # Preserve exact image bytes, including alpha; no color-space/render conversion.
    digest=hashlib.sha256(data).hexdigest()[:12]
    dest=folder/(slug+'_'+channel+'_'+digest+ext)
    dest.write_bytes(data)
    if not image.packed_file:image.pack()
    return str(dest)


FILE_EXTENSIONS = {'.png':'.png','.jpg':'.jpg','.jpeg':'.jpg','.tga':'.tga','.tif':'.tif','.tiff':'.tif'}
# texture_match_sources label of a base color picked by file name.
NAME_MATCH = 'Image file name beside the model'


def copy_file(source, folder, slug, channel):
    """A staged image file copied like copy_image, for images no Blender
    material holds (stand-in choices, name matches)."""
    ext=FILE_EXTENSIONS.get(Path(source).suffix.lower())
    if not ext:return None
    try:data=Path(source).read_bytes()
    except OSError:return None
    if not data:return None
    dest=folder/(slug+'_'+channel+'_'+hashlib.sha256(data).hexdigest()[:12]+ext)
    dest.write_bytes(data)
    return str(dest)


SUFFIXES = {'PNG':('.png',),'JPEG':('.jpg','.jpeg'),'TARGA':('.tga',),'TARGA_RAW':('.tga',),
            'TIFF':('.tif','.tiff'),'OPEN_EXR':('.exr',)}


def packed_stand_in(image):
    """Whether a packed image asks for a file of another format: a stand-in
    as a prepared .blend keeps it (Analyze materials reads that file, where
    staged_images has no record of the stand-ins)."""
    if not (image.packed_file and image.filepath):return False
    suffix=Path(image.filepath.replace('\\','/')).suffix.lower()
    return bool(suffix) and image.file_format in SUFFIXES and suffix not in SUFFIXES[image.file_format]


def asked_name(image):
    """The file name the model asks for, and Blender's image name when it differs."""
    if not image.filepath:return image.name
    name=image.filepath.replace('\\','/').rsplit('/',1)[-1]
    return name if name==image.name else name+' ('+image.name+')'


def recover_material(mat, folder, slug):
    cfg={'preset':'generic','mode':'generate','confirmed':False,'textures':{},
         'color':list(mat.diffuse_color[:3]),'source_material_name':mat.name}
    warnings=[];direct={};fallback={};mmd=[];mmd_shading=False
    if not mat.use_nodes or not mat.node_tree:
        if source_hidden(mat,[]):cfg['face_layer']=face_layer(mat.name)
        return cfg,warnings
    nodes=connected_nodes(mat.node_tree)
    principled=[(n,c) for n,c in nodes if n.type=='BSDF_PRINCIPLED']
    if len(principled)==1:
        node,_=principled[0]
        cfg['color']=list(node.inputs['Base Color'].default_value[:3])
        cfg['roughness']=float(node.inputs['Roughness'].default_value)
        cfg['metalness']=float(node.inputs['Metallic'].default_value)
    for node,context in principled:
        for label,channel in CHANNELS:
            image=direct_image(node.inputs.get(label),context,channel)
            if image:direct.setdefault(channel,[]).append(image)
    if not principled:
        # Unlit/toon setups: an image straight into the output or a shader's color.
        for surface in output_surfaces(mat.node_tree):
            image=direct_image(surface,(),'color')
            if image:direct.setdefault('color',[]).append(image)
        for node,context in nodes:
            if node.type in COLOR_SHADERS:
                image=direct_image(node.inputs.get('Color'),context,'color')
                if image:direct.setdefault('color',[]).append(image)
    assigned_images={image.as_pointer() for images in direct.values() for image in images}
    unsuffixed={}
    for node,_ in nodes:
        if node.type!='TEX_IMAGE' or not node.image:continue
        name=node.name.lower()
        if re.fullmatch(r'mmd_base_tex(?:\.\d+)?',name):
            mmd.append(node.image);continue
        if MMD_SHADING_NODE.fullmatch(name):
            mmd_shading=True;continue
        channel=image_channel(node.image)
        if channel and node.image.as_pointer() not in assigned_images:
            fallback.setdefault(channel,[]).append(node.image)
        elif not channel and node.image.as_pointer() not in assigned_images:
            unsuffixed[node.image.as_pointer()]=node.image
    if mmd and not direct.get('color'):direct['color']=mmd
    if not direct.get('color') and not fallback.get('color') and unsuffixed:
        # One unsuffixed image is the base color; with several (Body, Body_M,
        # Body_S ...) only the shared base name qualifies.
        images=list(unsuffixed.values())
        stems={i.as_pointer():image_stem(i).lower() for i in images}
        base=[i for i in images if all(stems[o.as_pointer()].startswith(stems[i.as_pointer()]) for o in images)]
        if len(images)==1 or len(base)==1:
            fallback['color']=[images[0] if len(images)==1 else base[0]]
    channels=sorted(set(direct)|set(fallback))
    candidates={};sources={}
    for channel in channels:
        images=direct.get(channel) or fallback[channel]
        paths=set();offered=False
        for image in {i.as_pointer():i for i in images}.values():
            used=staged_images.stand_in(image)
            if channel=='opacity' and image.file_format=='JPEG' and (used or packed_stand_in(image)):
                # A JPEG has no transparency to stand in for the image asked for.
                warnings.append(mat.name+': the model asks for '+(used[0] if used else asked_name(image))+' for its opacity; '+(used[1] if used else 'the JPEG used for it')+' has no transparency, so no opacity image was taken.')
                continue
            path=copy_image(image,folder,slug,channel)
            if path:
                paths.add(path)
                if used:warnings.append('The model asks for '+used[0]+'; '+used[1]+' was used.')
                continue
            options=[p for p in (copy_file(c,folder,slug,channel) for c in staged_images.choices(image)) if p]
            if options:
                paths.update(options);offered=True
                names=sorted({staged_images.found_name(c) for c in staged_images.choices(image)})
                warnings.append(mat.name+': the model asks for '+asked_name(image)+', which is missing; '+str(len(names))+' files have its name with another extension ('+', '.join(names)+'). Choose one in Materials.')
            elif EXTENSIONS.get(image.file_format):warnings.append(mat.name+': texture '+asked_name(image)+' is missing or empty; choose an image for this material in Appearance.')
            else:warnings.append(mat.name+': unsupported texture format '+image.file_format)
        candidates[channel]=sorted(paths)
        if len(paths)==1:
            cfg['textures'][channel]=next(iter(paths))
        elif len(paths)>1 and not offered:
            warnings.append(mat.name+': multiple '+channel+' images; choose the intended image in Materials.')
        # Only an image found is labelled; the page shows the label beside
        # whatever image the user picks for the channel later.
        if paths:sources[channel]='Blender image connection' if channel in direct else 'Image filename in connected Blender material'
    cfg['candidates']=candidates
    # Blender ignores a connected Base Color's own value (often the material's
    # earlier flat colour), so a base-color image is not tinted with it.
    if len(principled)==1 and principled[0][0].inputs['Base Color'].is_linked and candidates.get('color'):
        cfg['color']=[1.0,1.0,1.0]
    cfg['ambiguous']=[channel for channel,paths in candidates.items() if len(paths)>1]
    cfg['texture_match_sources']=sources
    if source_hidden(mat,nodes):
        cfg['face_layer']=face_layer(mat.name)
        warnings.append(mat.name+': fully transparent in the source (an expression layer); hidden in the game unless shown in Appearance > Face details.')
    if len(principled)>1 or mmd_shading or any(channel not in direct for channel in channels):
        warnings.append(mat.name+': recovered image references where available. Complex Blender shader mixing/toon effects are not baked; review the material appearance.')
    return cfg,warnings


def texture_and_materials(meshes,folder,hero_root):
    folder.mkdir(parents=True,exist_ok=True);result={};lookup={};warnings=[]
    materials={s.material.name:s.material for obj in meshes for s in obj.material_slots if s.material}
    for name,mat in materials.items():
        # The hash keeps the identity stable when the readable part is cut.
        slug=prepared_slug(name)
        rel=hero_root+'/materials/prepared/'+slug+'.vmat';lookup[name]=rel
        cfg,notes=recover_material(mat,folder,slug)
        result[rel]=cfg;warnings.extend(notes)
    if any(not o.material_slots or any(not s.material for s in o.material_slots) for o in meshes):
        raise ValueError('Every polygon must have a named material slot; assign missing slots in a source copy.')
    warnings.extend(name_matched(materials,result,lookup,folder))
    # One note per image, however many materials use it.
    return result,lookup,list(dict.fromkeys(warnings))


def name_matched(materials, result, lookup, folder):
    """Base colors for materials without one, from the images staging matched
    to them by file name. A material with no base-color image link takes its
    image when exactly one fits it and that image fits no other such
    material; a material whose linked image is missing, or with several
    fits, gets them as candidates only. A link that exists is never replaced."""
    matches=staged_images.name_matches();warnings=[];wanting={}
    for name in materials:
        cfg=result[lookup[name]]
        if cfg['textures'].get('color') or cfg.get('candidates',{}).get('color'):continue
        fits=sorted({p for p in matches.get(name,[]) if Path(p).is_file()})
        if fits:wanting[name]=(cfg,fits,'color' not in cfg.get('candidates',{}))
    uses={}
    for _,fits,_ in wanting.values():
        for path in fits:uses[path]=uses.get(path,0)+1
    for name,(cfg,fits,unlinked) in wanting.items():
        copies=sorted({c for c in (copy_file(p,folder,prepared_slug(name),'color') for p in fits) if c})
        if not copies:continue
        cfg.setdefault('candidates',{})['color']=copies
        cfg.setdefault('texture_match_sources',{})['color']=NAME_MATCH
        shown=', '.join(sorted({Path(p).name for p in fits}))
        if unlinked and len(copies)==1 and all(uses[p]==1 for p in fits):
            cfg['textures']['color']=copies[0]
            # The image is the base color; the flat colour was the stand-in for it.
            cfg['color']=[1.0,1.0,1.0]
            warnings.append(name+': the model links no image to it; '+shown+' beside the model has a matching file name and was picked. Check it in Materials.')
        else:
            if len(copies)>1 and 'color' not in cfg.setdefault('ambiguous',[]):cfg['ambiguous'].append('color')
            why='the model links no image to it' if unlinked else 'the image the model links to it is missing'
            warnings.append(name+': '+why+'; '+shown+' beside the model '+('has' if len(fits)==1 else 'have')+' a matching file name. Choose an image in Materials.')
    return warnings
