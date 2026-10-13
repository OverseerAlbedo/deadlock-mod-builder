"""Recover image references from materials; never bake or rewrite shader graphs.

Direct Principled links, passthrough groups/reroutes and connected MMD base
textures are preferred. Without a Principled BSDF, an image feeding the
material output or another shader's color (Diffuse, Emission, Toon ...) is the
base color, also through a color mix/tint with a single image input. Complex
graphs use explicit image channel suffixes; a single remaining unsuffixed image
is the base color. Multiple images remain candidates for review rather than an
arbitrary choice.
"""
import hashlib
from pathlib import Path
import re


# Readable characters of a prepared material's name. The native compiler names
# the textures it makes after the material (about 40 characters more) and
# Windows paths end at 259 characters, so the readable part stays short; the
# hash of the full name is the material's identity (earlier choices follow it).
NAME_CHARS = 24

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
    if node.type=='TEX_IMAGE':return node.image
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


def recover_material(mat, folder, slug):
    cfg={'preset':'generic','mode':'generate','confirmed':False,'textures':{},
         'color':list(mat.diffuse_color[:3]),'source_material_name':mat.name}
    warnings=[];direct={};fallback={};mmd=[]
    if not mat.use_nodes or not mat.node_tree:return cfg,warnings
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
        if name.startswith(('mmd_toon_tex','mmd_sphere_tex')):continue
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
        paths=set()
        for image in {i.as_pointer():i for i in images}.values():
            path=copy_image(image,folder,slug,channel)
            if path:paths.add(path)
            elif EXTENSIONS.get(image.file_format):warnings.append(mat.name+': texture '+image.name+' is missing or empty; choose an image for this material in Appearance.')
            else:warnings.append(mat.name+': unsupported texture format '+image.file_format)
        candidates[channel]=sorted(paths)
        if len(paths)==1:
            cfg['textures'][channel]=next(iter(paths))
        elif len(paths)>1:
            warnings.append(mat.name+': multiple '+channel+' images; choose the intended image in Materials.')
        sources[channel]='Blender image connection' if channel in direct else 'Image filename in connected Blender material'
    cfg['candidates']=candidates
    # Blender ignores a connected Base Color's own value (often the material's
    # earlier flat colour), so a base-color image is not tinted with it.
    if len(principled)==1 and principled[0][0].inputs['Base Color'].is_linked and candidates.get('color'):
        cfg['color']=[1.0,1.0,1.0]
    cfg['ambiguous']=[channel for channel,paths in candidates.items() if len(paths)>1]
    cfg['texture_match_sources']=sources
    if len(principled)>1 or any(channel not in direct for channel in channels):
        warnings.append(mat.name+': recovered image references where available. Complex Blender shader mixing/toon effects are not baked; review the material appearance.')
    return cfg,warnings


def texture_and_materials(meshes,folder,hero_root):
    folder.mkdir(parents=True,exist_ok=True);result={};lookup={};warnings=[]
    materials={s.material.name:s.material for obj in meshes for s in obj.material_slots if s.material}
    for name,mat in materials.items():
        # The hash keeps the identity stable when the readable part is cut.
        slug=re.sub('[^a-zA-Z0-9_-]','_',name).lower()[:NAME_CHARS]+'_'+hashlib.sha256(name.encode()).hexdigest()[:8]
        rel=hero_root+'/materials/prepared/'+slug+'.vmat';lookup[name]=rel
        cfg,notes=recover_material(mat,folder,slug)
        result[rel]=cfg;warnings.extend(notes)
    if any(not o.material_slots or any(not s.material for s in o.material_slots) for o in meshes):
        raise ValueError('Every polygon must have a named material slot; assign missing slots in a source copy.')
    return result,lookup,warnings
