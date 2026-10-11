"""Static shape-key baking, never runtime flex/animation export.

Works on a temporary imported scene only. Shape key coordinates remain in the
mesh's rest space; existing armature/bone transforms and vertex weights survive.
"""
import numpy as np


def apply_static_shapes(meshes, mode):
    """Bake one explicitly chosen source shape, without baking armature motion.

    Blender's native mix handles relative/absolute keys, masks and muted keys.
    All coordinates are collected and validated before keys are cleared. This
    operates only on the temporary imported scene, never the user's source.
    """
    if mode not in {'current_static', 'basis_static'}:
        raise ValueError('Choose Shape key handling in Conversion review before converting a source with shape keys.')
    import bpy
    bpy.context.view_layer.update()
    prepared=[]
    for obj in meshes:
        keys=obj.data.shape_keys
        if not keys:continue
        count=max(0,len(keys.key_blocks)-1)
        values={key.name:float(key.value) for key in list(keys.key_blocks)[1:]}
        muted=[key.name for key in list(keys.key_blocks)[1:] if key.mute]
        # from_mix evaluates only shape keys in local rest space. It does not
        # include the armature modifier or object/world transformations.
        temporary=None
        try:
            shape=keys.reference_key
            if mode=='current_static':
                temporary=obj.shape_key_add(name='Deadlock static shape',from_mix=True)
                shape=temporary
            points=np.asarray([list(vertex.co) for vertex in shape.data],dtype=float)
        finally:
            if temporary is not None:obj.shape_key_remove(temporary)
        if points.shape!=(len(obj.data.vertices),3) or not np.isfinite(points).all():
            raise ValueError('Shape keys produced invalid static geometry: '+obj.name)
        prepared.append((obj,points,{'mesh':obj.name,'mode':mode,'shape_keys_baked':count,
                                    'saved_values':values,'muted':muted,
                                    'source_frame':int(bpy.context.scene.frame_current),
                                    'runtime_morphs_exported':False,'normals':'recomputed'}))
    receipts=[]
    for obj,points,receipt in prepared:
        obj.shape_key_clear()
        for vertex,position in zip(obj.data.vertices,points):vertex.co=position
        obj.data.update()
        if obj.data.has_custom_normals and hasattr(obj.data,'normals_split_custom_set'):
            obj.data.normals_split_custom_set([(0.,0.,0.) for _ in obj.data.loops])
        receipts.append(receipt)
    return receipts


def bake_relative(basis, shapes, weights):
    points=np.asarray(basis,dtype=float)
    if points.ndim!=2 or points.shape[1]!=3 or not np.isfinite(points).all():raise ValueError('Invalid facial basis positions.')
    result=points.copy()
    for name,value in weights.items():
        if name not in shapes:raise ValueError('Unknown facial shape: '+name)
        if isinstance(value,bool) or not isinstance(value,(int,float)) or not np.isfinite(value) or not 0<=value<=1:
            raise ValueError('Static facial weights must be between 0 and 1.')
        shape,relative=shapes[name]
        shape=np.asarray(shape,dtype=float);relative=np.asarray(relative,dtype=float)
        if shape.shape!=points.shape or relative.shape!=points.shape or not np.isfinite(shape).all() or not np.isfinite(relative).all():
            raise ValueError('Facial shapes have incompatible geometry.')
        result += value*(shape-relative)
    if not np.isfinite(result).all():raise ValueError('Static facial result is not finite.')
    return result


def apply_static_face(meshes, selection):
    if not selection or not selection.get('enabled'):return []
    if selection.get('mode','static')!='static':raise ValueError('Runtime facial animation export is not implemented. Choose static expression baking.')
    weights=selection.get('weights',{});available=set();prepared=[]
    for obj in meshes:
        keys=obj.data.shape_keys
        if not keys or len(keys.key_blocks)<=1:continue
        if not keys.use_relative:raise ValueError('Absolute shape-key animation cannot be baked by the relative expression controls: '+obj.name)
        if keys.animation_data and (keys.animation_data.drivers or keys.animation_data.action):
            raise ValueError('Facial shape drivers/animation need a manually baked source copy: '+obj.name)
        shapes={};selected={}
        for key in list(keys.key_blocks)[1:]:
            name=obj.name+'/'+key.name;available.add(name)
            if key.vertex_group:raise ValueError('Masked facial shape keys need a manually baked source copy: '+name)
            shapes[name]=([list(v.co) for v in key.data],[list(v.co) for v in key.relative_key.data])
            if name in weights:selected[name]=weights[name]
        points=bake_relative([list(v.co) for v in keys.key_blocks[0].data],shapes,selected)
        prepared.append((obj,points,selected))
    missing=set(weights)-available
    if missing:raise ValueError('Selected facial shapes are absent from the source: '+', '.join(sorted(missing)))
    receipts=[]
    for obj,points,selected in prepared:
        obj.shape_key_clear()
        for vertex,position in zip(obj.data.vertices,points):vertex.co=position
        obj.data.update()
        if obj.data.has_custom_normals and hasattr(obj.data,'normals_split_custom_set'):
            # Shapes may change surface directions; zero requests Blender's
            # automatic normals instead of preserving stale imported normals.
            obj.data.normals_split_custom_set([(0.,0.,0.) for _ in obj.data.loops])
        receipts.append({'mesh':obj.name,'weights':selected,'mode':'static',
                         'runtime_morphs_exported':False,'normals':'recomputed'})
    return receipts
