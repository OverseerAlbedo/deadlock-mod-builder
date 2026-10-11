"""Executed ONLY by external Blender 4.2–4.5 / 5.0 / 5.2, with autoexec disabled."""
import sys
import json
import math
import re
import hashlib
import shutil
from pathlib import Path
sys.path.insert(0, str(Path(__file__).parent))
from algorithms import identify, estimate_landmarks, normalize_weights, deformation_checks, component_count
from retarget import rest_maps, anatomical_frames, rigid_attachment_map, NEXT, FIT_MODES
from exact_bind import rigid_display_matrix, store_bind_metadata, load_correction_bind, METADATA_KEY
from blend_source import snapshot_blend
from clothing import validate_garment_binding
from static_face import apply_static_face, apply_static_shapes
from material_textures import texture_and_materials
from source_helpers import mmd_helpers, helper_receipt, remove_workspace_helpers, uses_linked_libraries, physics_bone_bindings, helper_diagnostic
from source_rigs import resolve_modular_rigs, append_saved_notice
from source_compatibility import normalize as normalize_source_rig
from secondary_bones import selection as secondary_selection, extend_target
from arm_skinning import plan as arm_skin_plan, redistribute as arm_skin_weights, MODES as ARM_SKIN_MODES, POLICY as ARM_SKIN_POLICY
from scene_isolation import isolate, last_receipt, apply_pending_modifiers
import weapon_fit
import bpy
import numpy as np
from mathutils import Matrix, Vector, Quaternion


def matrix_list(matrix): return [list(row) for row in matrix]

def select_source_rig(meshes, rigs, reports=None):
    """Resolve render bindings, not scene armature count or largest-object guesses.

    Unused authoring/control rigs do not make a second character. Multiple
    render-bound rigs may share one exact rest skeleton (separate outfit parts).
    Different bound skeletons remain explicit rather than losing any meshes.
    Only this in-memory import is adjusted; the source is never saved.
    """
    if len(rigs) <= 1:
        return rigs
    active = set()
    owners = {}
    for mesh in meshes:
        bound = {m.object for m in mesh.modifiers if m.type == 'ARMATURE' and m.object is not None}
        parent = mesh.parent
        while parent is not None:
            if parent.type == 'ARMATURE':
                bound.add(parent)
                break
            parent = parent.parent
        if any(r not in rigs for r in bound):
            raise ValueError('Character mesh references a rig outside its imported scene: '+mesh.name)
        if not bound:
            used = {mesh.vertex_groups[g.group].name for v in mesh.data.vertices for g in v.groups if g.weight > 0}
            matches = [r for r in rigs if used.intersection(b.name for b in r.data.bones if b.use_deform)]
            if len(matches) > 1:
                raise ValueError('Mesh '+mesh.name+' has weights matching multiple armatures but no explicit armature binding. Bind it to its intended rig before importing.')
            bound.update(matches)
        owners[mesh.name] = sorted(r.name for r in bound)
        active.update(bound)
    selected = sorted(active, key=lambda r: r.name)
    merged = []
    if len(selected) > 1:
        primary = selected[0]
        def equivalent(other):
            if set(primary.data.bones.keys()) != set(other.data.bones.keys()):
                return False
            for a in primary.data.bones:
                b = other.data.bones[a.name]
                if (a.parent.name if a.parent else None) != (b.parent.name if b.parent else None) or a.use_deform != b.use_deform:
                    return False
                if not np.allclose(np.asarray(primary.matrix_world @ a.matrix_local),
                                   np.asarray(other.matrix_world @ b.matrix_local), rtol=1e-7, atol=1e-6):
                    return False
            return True
        if not all(equivalent(r) for r in selected[1:]):
            return resolve_modular_rigs(meshes, rigs, selected, owners, reports, rest_rig_controls)
        merged = [r.name for r in selected[1:]]
        for mesh in meshes:
            world = mesh.matrix_world.copy()
            for m in mesh.modifiers:
                if m.type == 'ARMATURE' and m.object in active:
                    m.object = primary
            if mesh.parent in active:
                mesh.parent = primary
                mesh.matrix_world = world
        selected = [primary]
    ignored = [r.name for r in rigs if r not in active]
    if reports is not None:
        reports.append({'policy':'render-bound-source-rig-v1','selected_armatures':[r.name for r in selected],
            'ignored_armatures':sorted(ignored),'equivalent_armatures':merged,'mesh_bindings':owners,
            'rigid_body_meshes':0,'joint_empties':0,'objects':[],
            'warning':'Resolved character rigs from mesh bindings; ignored '+str(len(ignored))+
                      ' unused armatures and combined '+str(len(merged))+' equivalent rest rigs. All character meshes are retained.'})
    return selected

def import_source(path,clear=True,allow_shape_keys=False,helper_reports=None,scene=None):
    if clear:bpy.ops.wm.read_factory_settings(use_empty=True)
    prior=set(bpy.context.scene.objects)
    suffix=Path(path).suffix.lower()
    if suffix in {'.glb','.gltf'}:
        properties = bpy.ops.import_scene.gltf.get_rna_type().properties
        optional = {'disable_bone_shape': True} if 'disable_bone_shape' in properties else {}
        bpy.ops.import_scene.gltf(filepath=path, merge_vertices=False, **optional)
    elif suffix=='.fbx':
        bpy.ops.import_scene.fbx(filepath=path, use_anim=False, automatic_bone_orientation=False)
    elif suffix=='.blend':
        # Both packed source snapshots and workspace corrections disable scripts.
        if clear:
            bpy.ops.wm.open_mainfile(filepath=path, load_ui=False, use_scripts=False)
            prior=set()
        else:
            # Append the single source scene as local data; use only its actual
            # objects (not unrelated orphan datablocks). No UI or scripts run.
            # The intake snapshot already checked the scene and packed images.
            with bpy.data.libraries.load(path,link=False) as (available,loaded):
                if len(available.scenes)!=1:
                    raise ValueError('The fitted clothing Blender file must contain one scene.')
                loaded.scenes=list(available.scenes)
            imported_scene=loaded.scenes[0]
            for obj in list(imported_scene.objects):
                bpy.context.scene.collection.objects.link(obj)
            bpy.data.scenes.remove(imported_scene)
            # Blender 5.2 defers evaluation of appended object transforms.
            # Resolve them before reading world-space geometry or reparenting.
            bpy.context.view_layer.update()
        if uses_linked_libraries(bpy):
            raise ValueError('This Blender file uses linked libraries. In a separate source copy, make the character and materials local, then import again.')
    else: raise ValueError('Unsupported format. The experimental bridge accepts GLB/glTF or FBX; native import validation is still required.')
    if bpy.context.object and bpy.context.object.mode != 'OBJECT':
        bpy.ops.object.mode_set(mode='OBJECT')
    imported=[o for o in bpy.context.scene.objects if o not in prior]
    physics_helpers = mmd_helpers(imported)
    if helper_reports is not None and physics_helpers:
        helper_reports.append(helper_receipt(physics_helpers))
    # Keep the character; set aside cameras, lights, hidden alternates, other
    # characters and scenery; repair modifiers, transforms and missing weights.
    # Only this temporary in-memory scene changes, never the user's file.
    meshes,rigs=isolate(bpy,imported,scene,allow_shape_keys,helper_reports,
                        select_source_rig if clear else None,physics_helpers,rig_logic=clear)
    if clear:
        receipt=last_receipt()
        if helper_reports is not None and receipt and receipt['warning']:
            helper_reports.append({'policy':receipt['policy'],'warning':receipt['warning'],'objects':[],
                                   'rigid_body_meshes':0,'joint_empties':0})
    else:
        rigs=[o for o in imported if o.type=='ARMATURE']
        rigs=select_source_rig(meshes,rigs,helper_reports)
    if sum(len(o.data.vertices) for o in meshes)>5_000_000:
        raise ValueError('Preparation is limited to five million imported vertices; use Prepared mode for larger validated assets.')
    if clear:
        append_saved_notice(meshes,rigs,helper_reports)
    for mesh in meshes:
        if not allow_shape_keys and mesh.data.shape_keys and len(mesh.data.shape_keys.key_blocks)>1:
            raise ValueError('Morph/flex conversion needs an explicit static choice. In Conversion review, choose Shape key handling: Keep the saved shape (static) or Use the base shape (static). Preparation stopped without dropping shape keys.')
        if len([m for m in mesh.modifiers if m.type=='ARMATURE'])>1:
            raise ValueError('Multiple armature modifiers are unsupported: '+mesh.name)
        if mesh.matrix_world.to_3x3().determinant()<=0:
            raise ValueError('Mirrored/singular object transform: apply transforms in a source copy first.')
    if len(rigs) == 1:
        normalize_source_rig(rigs[0], meshes, helper_reports)
    return meshes,rigs


def import_garment(path,meshes,rigs,helper_reports=None):
    prior=set(bpy.context.scene.objects)
    added,garment_rigs=import_source(path,clear=False,helper_reports=helper_reports)
    garment_helpers=mmd_helpers([obj for obj in bpy.context.scene.objects if obj not in prior])
    for obj in added:
        for modifier in obj.modifiers:
            if modifier.type=='ARMATURE' and modifier.object is not None and modifier.object not in garment_rigs:
                raise ValueError('Clothing references an armature outside its imported scene. Include its matching rest rig in the clothing source: '+obj.name)
    if sum(len(o.data.vertices) for o in meshes+added)>5_000_000:
        raise ValueError('Character and clothing together exceed the five million preparation vertex limit.')
    if rigs:
        source_info=describe(meshes,rigs)
        garment_info=describe(added,garment_rigs)
        validate_garment_binding(source_info['bones'],garment_info['bones'],garment_info['weighted_bones'])
        for obj in added:
            if any(not row for row in skin_rows(obj,skin_group_names([obj],garment_rigs)[0])):
                raise ValueError('Clothing has unweighted vertices: '+obj.name+
                                 '. Weight the fitted garment to the same source rig in Blender before importing.')
            world=obj.matrix_world.copy()
            obj.parent=rigs[0];obj.matrix_world=world
            modifiers=[modifier for modifier in obj.modifiers if modifier.type=='ARMATURE']
            modifier=modifiers[0] if modifiers else obj.modifiers.new('Character clothing','ARMATURE')
            modifier.object=rigs[0]
        remove_workspace_helpers(bpy,garment_helpers)
        for rig in garment_rigs:bpy.data.objects.remove(rig,do_unlink=True)
    elif garment_rigs:
        raise ValueError('The character is unrigged but clothing has an armature. Use an unrigged fitted garment for automatic weighting, or import a rigged character with its matching weighted garment.')
    return meshes+added,rigs


def skin_group_names(meshes,rigs):
    """Blender skin groups must name deform bones, not arbitrary mesh metadata."""
    deform={bone.name for rig in rigs for bone in rig.data.bones if bone.use_deform}
    used={obj.vertex_groups[group.group].name for obj in meshes for vertex in obj.data.vertices
          for group in vertex.groups if group.weight>0}
    return deform,sorted(used-deform)


def skin_rows(obj,deform):
    return [[(obj.vertex_groups[group.group].name,float(group.weight)) for group in vertex.groups
             if group.weight>0 and obj.vertex_groups[group.group].name in deform]
            for vertex in obj.data.vertices]


def non_skin_notice(names):
    return ('Excluded non-deforming vertex groups from skeletal mapping and skin weights: '+', '.join(names)+
            '. These groups do not drive deform bones; the original source file is unchanged.')


def rest_rig_controls(rig, meshes):
    """Qualify pose-only authoring controls for the existing rest-data export.

    This exporter does not request pose baking. Conversion reads data.bones
    and mesh.data directly and replaces the source armature, so pose controls
    are not exported. Controls that would move the armature object or its rest
    data are baked into the temporary scene (never the user's file): the
    current object placement is kept and the controls are dropped.
    """
    baked=[]
    if rig.constraints:
        world=rig.matrix_world.copy()
        for constraint in list(rig.constraints):
            rig.constraints.remove(constraint)
        rig.matrix_world=world
        baked.append('armature object constraints')
    if rig.data.animation_data and rig.data.animation_data.drivers:
        rig.data.animation_data_clear()
        baked.append('armature data drivers')
    drivers=list(rig.animation_data.drivers) if rig.animation_data else []
    quoted=r'"(?:[^"\\]|\\.)*"'
    pose=re.compile(r'^pose\.bones\[('+quoted+r')\]\.(.+)$')
    transform={'location','rotation_euler','rotation_quaternion','rotation_axis_angle','scale',
               'custom_shape_scale_xyz','custom_shape_translation','custom_shape_rotation_euler'}
    constraint=re.compile(r'^constraints\[('+quoted+r')\]\.[A-Za-z_][A-Za-z_0-9]*$')
    pose_drivers=0
    for driver in drivers:
        match=pose.fullmatch(driver.data_path)
        if not match or (match[2] not in transform and not constraint.fullmatch(match[2])):
            world=rig.matrix_world.copy()
            rig.animation_data.drivers.remove(driver)
            rig.matrix_world=world
            baked.append('object driver '+driver.data_path)
            continue
        pose_drivers+=1
    constraints=sum(len(b.constraints) for b in rig.pose.bones)
    return {'policy':'pose-only-rest-rig-v1','pose_drivers':pose_drivers,'pose_constraints':constraints,
            'rest_bones_and_raw_geometry':True,'source_animation_baked':False,'pose_controls_exported':False,
            'baked_object_controls':baked}


def describe(meshes,rigs):
    bones=[]
    if rigs:
        rig=rigs[0]
        physics = physics_bone_bindings(bpy.context.scene.objects, rig)
        controls=rest_rig_controls(rig,meshes)
        for b in rig.data.bones:
            world=rig.matrix_world@b.matrix_local
            bones.append({'name':b.name,'parent':b.parent.name if b.parent else None,'position':list(world.translation),
                          'matrix':matrix_list(world),'deform':b.use_deform})
            if b.name in physics: bones[-1]['physics'] = physics[b.name]
    points=[list(o.matrix_world@v.co) for o in meshes for v in o.data.vertices]
    p=np.array(points)
    deform,ignored=skin_group_names(meshes,rigs)
    used=set();skin={}
    for o in meshes:
        for v in o.data.vertices:
            point=np.asarray(o.matrix_world@v.co,dtype=float)
            for g in v.groups:
                name=o.vertex_groups[g.group].name
                if g.weight<=0 or name not in deform:continue
                used.add(name)
                row=skin.setdefault(name,{'vertices':0,'weight_sum':0.,'sum':np.zeros(3),'lo':point.copy(),'hi':point.copy()})
                row['vertices']+=1;row['weight_sum']+=g.weight;row['sum']+=point*g.weight
                row['lo']=np.minimum(row['lo'],point);row['hi']=np.maximum(row['hi'],point)
    for bone in bones:
        if bone['name'] in skin:
            row=skin[bone['name']]
            bone['skin']={'vertices':row['vertices'],'weight_sum':row['weight_sum'],
                          'centroid':(row['sum']/row['weight_sum']).tolist(),'bounds':[row['lo'].tolist(),row['hi'].tolist()]}
    result={'armatures':len(rigs),'bones':bones,'vertices':len(points), 'bounds':[p.min(axis=0).tolist(),p.max(axis=0).tolist()],
            'meshes':[{'name':o.name,'vertices':len(o.data.vertices),'polygons':len(o.data.polygons),
                       'materials':[s.material.name if s.material else '(empty)' for s in o.material_slots],
                       'disconnected_components':component_count(len(o.data.vertices),[tuple(e.vertices) for e in o.data.edges])} for o in meshes],
            'morphs':[{'name':o.name+'/'+key.name,'mesh':o.name,'shape':key.name}
                      for o in meshes if o.data.shape_keys for key in list(o.data.shape_keys.key_blocks)[1:]],
            'weighted_bones':sorted(used),'suggested_mode':'rigged_humanoid' if rigs else 'unrigged_humanoid'}
    if ignored:result['warnings']=[non_skin_notice(ignored)]
    if rigs:
        result['rest_rig_controls']=controls
        if controls.get('baked_object_controls'):
            result.setdefault('warnings',[]).append('Kept the armature at its current placement and dropped controls that would move it: '+
                ', '.join(controls['baked_object_controls'][:6])+'. The source file is unchanged.')
        if controls['pose_drivers'] or controls['pose_constraints']:
            result.setdefault('warnings',[]).append('Imported the source rest skeleton and undeformed geometry. '+
                str(controls['pose_drivers'])+' pose drivers and '+str(controls['pose_constraints'])+
                ' pose constraints are authoring controls and will not be exported. Source animation is not transferred; inspect the converted rest pose.')
    receipt=last_receipt()
    if receipt:
        result['scene']=receipt
    if len(p):
        height=float(p[:,2].max()-p[:,2].min()) or 1.
        result['weapons']=weapon_fit.candidates(meshes,rigs,height)
    if bones:
        recovered = normalize_source_rig(rigs[0], meshes)
        hints = recovered['roles'] if recovered else {}
        if recovered:
            for bone in bones:
                bone['parent'] = recovered['parents'].get(bone['name'], bone['parent'])
            result['rest_rig_recovery'] = recovered
        for bone in bones:
            for role,name in hints.items():
                if name == bone['name']:
                    bone['humanoid_role_hint'] = role
        result['humanoid']=identify(bones,hints)
    else:
        try:result['humanoid']=estimate_landmarks(points)
        except ValueError as ex:result['humanoid']={'status':'Unsupported','errors':[str(ex)]}
    return result


def make_rig(target, fitted=None, fitted_tails=None):
    data=bpy.data.armatures.new('Deadlock target');obj=bpy.data.objects.new('Deadlock target',data)
    bpy.context.collection.objects.link(obj);bpy.context.view_layer.objects.active=obj;obj.select_set(True)
    bpy.ops.object.mode_set(mode='EDIT')
    for b in target:
        # EditBone strips scale. Its display frame deliberately keeps the exact
        # scaled world joint position; canonical T*R*S binds are stored separately.
        eb=data.edit_bones.new(b['name']);mat=Matrix(rigid_display_matrix(b['matrix']).tolist())
        # A fresh EditBone has zero length. Assigning its matrix before a tail
        # exists silently loses the requested orientation in Blender.
        eb.head=(0.,0.,0.);eb.tail=(0.,max(.01,float(b.get('length',1))),0.)
        eb.matrix=mat
        if fitted and b['name'] in fitted:eb.head=Vector(fitted[b['name']]);eb.tail=eb.head+mat.to_3x3()@Vector((0,eb.length,0))
        eb.use_deform=b.get('deform',True)
    for b in target:
        if b.get('parent'):data.edit_bones[b['name']].parent=data.edit_bones[b['parent']]
    if fitted:
        for b in target:
            children=[c for c in target if c.get('parent')==b['name'] and c['name'] in fitted]
            if children:
                tail=Vector(fitted[children[0]['name']]);bone=data.edit_bones[b['name']]
                if (tail-bone.head).length>.001:bone.tail=tail
    if fitted_tails:
        for name, tail in fitted_tails.items():
            data.edit_bones[name].tail = Vector(tail)
    bpy.ops.object.mode_set(mode='OBJECT');obj.select_set(False)
    return obj


def auto_weight(meshes,target,roles,landmarks):
    if set(roles)!=set(landmarks):raise ValueError('Confirm every target role and anatomical landmark before auto-weighting.')
    fitted={roles[r]:landmarks[r] for r in roles}
    byname={b['name']:b for b in target}
    target_height = (Vector(byname[roles['head']]['position']) - Vector(byname[roles['pelvis']]['position'])).length
    fitted_height = (Vector(landmarks['head']) - Vector(landmarks['pelvis'])).length
    if min(target_height, fitted_height) <= 1e-8: raise ValueError('Invalid fitted humanoid height.')
    fit_scale = fitted_height / target_height
    frames = anatomical_frames(fitted, roles)
    tails = {}
    for role, name in roles.items():
        if role in NEXT:
            tails[name] = landmarks[NEXT[role]]
        else:
            previous = 'neck' if role == 'head' else ('forearm_' if role.startswith('hand_') else 'shin_') + role[-1]
            fraction = .6 if role == 'head' else .35 if role.startswith('hand_') else .3
            length = (Vector(landmarks[role]) - Vector(landmarks[previous])).length * fraction
            tails[name] = (np.asarray(landmarks[role]) + frames[role][:3, 1] * length).tolist()
    # Non-primary controls follow their closest fitted ancestor, never become new generic bones.
    pending=list(target)
    while pending:
        progress=False
        for b in pending[:]:
            parent=b.get('parent')
            if b['name'] in fitted or parent is None or parent in fitted:
                if b['name'] not in fitted:
                    p=Vector(b['position']) * fit_scale
                    if parent:p=Vector(fitted[parent])+(Vector(b['position'])-Vector(byname[parent]['position']))*fit_scale
                    fitted[b['name']]=list(p)
                pending.remove(b);progress=True
        if not progress:raise ValueError('Invalid target hierarchy.')
    weighted=[dict(b,deform=b['name'] in roles.values(), length=float(b.get('length',1))*fit_scale) for b in target]
    rig=make_rig(weighted,fitted,tails)
    for obj in meshes:
        obj.vertex_groups.clear()
        bpy.ops.object.select_all(action='DESELECT');obj.select_set(True);rig.select_set(True)
        bpy.context.view_layer.objects.active=rig
        bpy.ops.object.parent_set(type='ARMATURE_AUTO',keep_transform=True)
    return rig



def import_props(job, meshes):
    """Attach local prop geometry with the exact canonical scaled target bind."""
    from rigid_props import attach_mesh
    exported=[];materials={};warnings=[]
    for index,prop in enumerate(job.get('props',[])):
        added,rigs=import_source(prop['source'],clear=False)
        if rigs:raise ValueError('An attached prop must be a rigid mesh without an armature. Its vertices will follow the selected existing target bone.')
        if sum(len(o.data.vertices) for o in meshes+added)>5_000_000:
            raise ValueError('Character and attached props exceed the five million preparation vertex limit.')
        for obj in added:
            if obj.parent and obj.parent.type=='ARMATURE':raise ValueError('Attached props cannot reference another rig.')
            obj.name='prop_'+str(index+1).zfill(2)+'_'+obj.name
        mats,mat_paths,mw=texture_and_materials(added,Path(job['workspace'])/'textures',job['hero_root'])
        materials.update(mats);warnings+=mw
        for obj in added:
            mesh=obj.data
            if hasattr(mesh,'calc_normals_split'):mesh.calc_normals_split()
            corners=mesh.corner_normals if hasattr(mesh,'corner_normals') else None
            normal_matrix=np.linalg.inv(np.asarray(obj.matrix_world.to_3x3())).T
            normals=[]
            for loop in mesh.loops:
                normal=normal_matrix@np.asarray(corners[loop.index].vector if corners else loop.normal)
                length=np.linalg.norm(normal)
                if length<=1e-12:raise ValueError('Invalid attached prop normal.')
                normals.append((normal/length).tolist())
            colors=[]
            for layer in mesh.color_attributes:
                if layer.domain not in {'POINT','CORNER'}:raise ValueError('Unsupported prop color domain: '+layer.domain)
                colors.append([list(layer.data[loop.vertex_index if layer.domain=='POINT' else loop.index].color) for loop in mesh.loops])
            raw={'name':obj.name,'positions':[list(obj.matrix_world@v.co) for v in mesh.vertices],
                 'normals':normals,'uvs':[[list(value.uv) for value in layer.data] for layer in mesh.uv_layers],
                 'colors':colors,'faces':[{'vertices':list(poly.vertices),'corners':list(poly.loop_indices),
                      'material':mat_paths[obj.material_slots[poly.material_index].material.name]} for poly in mesh.polygons]}
            output=attach_mesh(raw,job['target'],prop)
            for vertex,position in zip(mesh.vertices,output['positions']):vertex.co=position
            obj.parent=None;obj.matrix_parent_inverse=Matrix.Identity(4);obj.matrix_world=Matrix.Identity(4)
            obj.modifiers.clear();obj.vertex_groups.clear()
            group=obj.vertex_groups.new(name=prop['bone']);group.add(list(range(len(mesh.vertices))),1.,'REPLACE')
            if hasattr(mesh,'normals_split_custom_set'):mesh.normals_split_custom_set(output['normals'])
            if mesh.uv_layers:
                mesh.calc_tangents(uvmap=mesh.uv_layers[0].name)
                output['tangents']=[list(loop.tangent)+[loop.bitangent_sign] for loop in mesh.loops]
            output['deformation']=[];exported.append(output)
        meshes.extend(added)
    return exported,materials,warnings


def prepare(job,meshes,rigs):
    physics_helpers = mmd_helpers(bpy.context.scene.objects)
    mode=job['mode'];target=job['target'];options=job.get('options',{})
    weapon_plan=None if job.get('correction') else job.get('weapon')
    meshes,weapon_meshes=weapon_fit.split(meshes,weapon_plan)
    if weapon_plan and weapon_plan.get('action')=='remove':
        for obj in weapon_meshes:bpy.data.objects.remove(obj,do_unlink=True)
        weapon_meshes=[]
    if not meshes:raise ValueError('Only the gun is left after setting it aside. Choose the character objects in Scene objects.')
    fit_mode=options.get('fit_mode','match_target')
    fit_arm_joints=options.get('fit_arm_joints',True)
    arm_skinning=options.get('arm_skinning','source')
    if not isinstance(arm_skinning,str) or arm_skinning not in ARM_SKIN_MODES:
        raise ValueError('Choose a supported Arm skinning option before conversion.')
    if not isinstance(fit_arm_joints,bool):
        raise ValueError('Align arms to hero joints must be enabled or disabled.')
    if not isinstance(fit_mode,str) or fit_mode not in FIT_MODES:
        raise ValueError('Choose a supported Model fitting option before conversion.')
    if mode=='rigged_humanoid' and not rigs:raise ValueError('No armature: switch to Unrigged Humanoid.')
    if mode=='unrigged_humanoid' and rigs:raise ValueError('Armature found: switch to Rigged Humanoid.')
    if not options.get('reviewed'):raise ValueError('Confirm bone mapping/landmarks and orientation before preparation.')
    target_check=identify(target,options.get('target_roles'))
    if target_check['errors']:raise ValueError('Target hero needs humanoid role review: '+'; '.join(target_check['errors']))
    if mode=='unrigged_humanoid':
        points=[list(o.matrix_world@v.co) for o in meshes for v in o.data.vertices]
        estimate_landmarks(points) # unsupported geometry never bypassed just by checking a box
        rig=auto_weight(meshes,target,target_check['roles'],options['landmarks'])
        mapping={b['name']:b['name'] for b in target}
    else:
        rig=rigs[0];mapping=dict(options['mapping'])
        source_info=describe(meshes,[rig])
        source_check=identify(source_info['bones'],options.get('source_roles'))
        if source_check['errors']:raise ValueError('Source skeleton needs review: '+'; '.join(source_check['errors']))
        # Prevent a user mapping left arm to right foot while satisfying names alone.
        for role,name in source_check['roles'].items():
            if mapping.get(name)!=target_check['roles'][role]:raise ValueError('Mapping contradicts reviewed humanoid role: '+role)
    correction_bind = None
    if job.get('correction', False):
        restored, correction_bind = load_correction_bind(rig, target)
        source = {name: Matrix(matrix.tolist()) for name, matrix in restored.items()}
    else:
        source={b.name:rig.matrix_world@b.matrix_local for b in rig.data.bones}
    target_matrices={b['name']:Matrix(b['matrix']) for b in target}
    source_roles = target_check['roles'] if mode=='unrigged_humanoid' else source_check['roles']
    target_roles = target_check['roles']
    source_height=source[source_roles['head']].translation.z-min(source[source_roles['foot_L']].translation.z,source[source_roles['foot_R']].translation.z)
    target_height=target_matrices[target_roles['head']].translation.z-min(target_matrices[target_roles['foot_L']].translation.z,target_matrices[target_roles['foot_R']].translation.z)
    if min(source_height,target_height)<=1e-8:raise ValueError('Invalid upright height for unit normalization.')
    unit_scale=target_height/source_height
    if correction_bind is None:
        # Existing foreign-source unit conversion only. A prepared correction's
        # exact target bind is reconstructed from metadata and never normalized.
        for name,matrix in source.items():
            scales=matrix.to_scale()
            if min(scales)<=0 or max(scales)/min(scales)>1.0001:raise ValueError('Nonuniform source bone scale requires manual preparation: '+name)
            normalized=matrix.to_quaternion().to_matrix().to_4x4()
            normalized.translation=matrix.translation*unit_scale
            source[name]=normalized
    deform,ignored=skin_group_names(meshes,[rig])
    rows_by_mesh={obj.name:skin_rows(obj,deform) for obj in meshes}
    weighted_names = {name for rows in rows_by_mesh.values() for row in rows for name,_ in row}
    following = dict(options.get('mapping_anchors',{})) if not job.get('correction',False) else {}
    parents = {b['name']:b.get('parent') for b in source_info['bones']} if mode == 'rigged_humanoid' else {b.name:b.parent.name if b.parent else None for b in rig.data.bones}
    kept = None
    if options.get('preserve_physics_bones') and mode != 'rigged_humanoid':
        raise ValueError('Keeping existing physics bones requires an already rigged source.')
    if mode == 'rigged_humanoid' and not job.get('correction', False):
        kept = secondary_selection(options, source_info['bones'], weighted_names, source_roles, target_roles, target)
    anchor_transforms = {}
    source_bones = source_info['bones'] if mode == 'rigged_humanoid' else None
    if kept:
        for group in kept['groups']:
            anchor, destination = group['source_anchor'], group['target_anchor']
            anchor_transforms.update(rest_maps(source, target_matrices, {anchor: destination}, source_roles, target_roles, parents, fit_mode=fit_mode, source_bones=source_bones, target_bones=target, fit_arm_joints=fit_arm_joints)[0])
            for name in group['source_bones']:
                following.pop(name, None)
                mapping.pop(name, None)
    if following and options.get('follow_motion_reviewed') is not True:
        raise ValueError('Review body-following mappings and their loss of independent motion before preparation.')
    maps, alignment_audit = rest_maps(source, target_matrices,
                                      {n: d for n, d in mapping.items() if n in weighted_names}, source_roles, target_roles,
                                      parents, following, mapping, fit_mode=fit_mode, source_bones=source_bones, target_bones=target, fit_arm_joints=fit_arm_joints)
    if kept:
        group_transforms = {}
        for group in kept['groups']:
            transform = anchor_transforms[group['source_anchor']]
            if fit_mode == 'preserve_proportions' and fit_arm_joints:
                transform = rigid_attachment_map(transform, np.asarray(source[group['root']])[:3,3])
            group_transforms[group['root']] = transform
            for name in group['source_bones']:
                maps[name] = transform
                mapping[name] = kept['exported_bones'][name]
        target = extend_target(target, source, parents, kept, anchor_transforms, group_transforms)
        target_matrices = {b['name']: Matrix(b['matrix']) for b in target}
    exported=[];totals=[];warnings=['Secondary-motion geometry may require manual setup. Existing hero cloth preservation is unchanged.',
                               'Automatic pose probes do not establish artistic quality or in-game acceptance.']
    if mode=='rigged_humanoid':warnings.extend(source_info.get('warnings',[]))
    if fit_mode=='preserve_proportions':
        warnings.append('Source torso, head and leg proportions retained after overall scale and alignment. '+
                        ('Arms fitted to the hero shoulder, elbow and wrist pivots while retaining limb width. Arm lengths can change. ' if fit_arm_joints else
                         'Arm lengths retained; source elbows and wrists can differ from hero animation pivots. ')+
                        'Check joint movement, weapon grip and clothing clearance in game.')
    if ignored:warnings.append(non_skin_notice(ignored))
    if following:warnings.append(f'{len(following)} source bones follow mapped body joints with their rest offsets preserved. Their independent source twist, corrective, facial or physics motion is not transferred.')
    if kept:warnings.append(f"Kept {len(kept['exported_bones'])} existing physics bones in {len(kept['groups'])} groups with their skin weights. This preserves independent rigging only; Source 2 simulation, gravity, spring motion and collisions are not generated.")
    mats,mat_paths,mw=texture_and_materials(meshes+weapon_meshes,Path(job['workspace'])/'textures',job['hero_root']);warnings+=mw
    skin_plan = arm_skin_plan(target, target_roles, source_roles, mapping, alignment_audit.get('arm_deformation',{})) if arm_skinning=='smooth_twist' and not job.get('correction',False) else []
    arm_skin_audit={'policy':ARM_SKIN_POLICY,'mode':arm_skinning,'correction_weights_preserved':bool(job.get('correction',False)),
                   'segments':[{'role':s['role'],'twist_joints':[n for _,n in s['helpers']]} for s in skin_plan],'meshes':[]}
    for obj in meshes:
        mesh=obj.data
        rows=rows_by_mesh[obj.name]
        weights,stats=normalize_weights(rows,mapping,target_matrices,options.get('influence_limit',4))
        points=[];normal_maps=[]
        for vertex,row in zip(mesh.vertices,rows):
            total=sum(w for _,w in row)
            if total<=0:raise ValueError('Unweighted vertex in '+obj.name)
            # Geometry follows original weights; normalization/pruning for the final skin is separately reported.
            blend=sum((np.array(maps[n])*w/total for n,w in row if w>0),np.zeros((4,4)))
            if np.linalg.det(blend[:3,:3])<1e-8:raise ValueError('Retarget collapsed a transform in '+obj.name)
            p=blend@np.r_[np.array(obj.matrix_world@vertex.co)*unit_scale,1.]
            points.append(p[:3].tolist());normal_maps.append(np.linalg.inv(blend[:3,:3]@np.array(obj.matrix_world.to_3x3())*unit_scale).T)
        if skin_plan:
            fitted_rows,skin_audit=arm_skin_weights(rows,points,mapping,skin_plan,options.get('influence_limit',4),kept['exported_bones'] if kept else ())
            weights,stats=normalize_weights(fitted_rows,{n:n for n in target_matrices},target_matrices,options.get('influence_limit',4))
            arm_skin_audit['meshes'].append({'mesh':obj.name,**skin_audit})
        totals.append(stats)
        if hasattr(mesh,'calc_normals_split'):mesh.calc_normals_split()
        normals=[]
        corner_normals=mesh.corner_normals if hasattr(mesh,'corner_normals') else None
        for loop in mesh.loops:
            n=normal_maps[loop.vertex_index]@np.array(corner_normals[loop.index].vector if corner_normals else loop.normal)
            norm=np.linalg.norm(n)
            if norm<1e-8:raise ValueError('Invalid normal.')
            normals.append((n/norm).tolist())
        uvs=[[list(x.uv) for x in layer.data] for layer in mesh.uv_layers]
        colors=[]
        for layer in mesh.color_attributes:
            if layer.domain not in {'POINT','CORNER'}:raise ValueError('Unsupported color domain: '+layer.domain)
            colors.append([list(layer.data[l.vertex_index if layer.domain=='POINT' else l.index].color) for l in mesh.loops])
        polygons=[{'vertices':list(poly.vertices),'corners':list(poly.loop_indices),
                   'material':mat_paths[obj.material_slots[poly.material_index].material.name]} for poly in mesh.polygons]
        # Bake the converted rest shape and replace the foreign rig, preserving topology and slots.
        for v,p in zip(mesh.vertices,points):v.co=p
        obj.parent=None;obj.matrix_parent_inverse=Matrix.Identity(4);obj.matrix_world=Matrix.Identity(4);obj.modifiers.clear();obj.vertex_groups.clear()
        for name in target_matrices:obj.vertex_groups.new(name=name)
        for i,row in enumerate(weights):
            for name,w in row:obj.vertex_groups[name].add([i],w,'REPLACE')
        if hasattr(mesh,'normals_split_custom_set'):mesh.normals_split_custom_set(normals)
        tangents=[]
        if mesh.uv_layers:
            mesh.calc_tangents(uvmap=mesh.uv_layers[0].name)
            tangents=[list(loop.tangent)+[loop.bitangent_sign] for loop in mesh.loops]
        probes={}
        for role in ('forearm_L','upperarm_L','shin_L','thigh_L','head','hand_L','forearm_R','shin_R'):
            name=target_check['roles'][role];m=target_matrices[name]
            branch={name}
            while True:
                expanded=branch|{b['name'] for b in target if b.get('parent') in branch}
                if expanded==branch:break
                branch=expanded
            delta=matrix_list(m@Matrix.Rotation(math.radians(35),4,'X')@m.inverted())
            probes[name]={child:delta for child in branch}
        checks=deformation_checks(points,[p['vertices'] for p in polygons],weights,probes)
        if any(not x['finite'] for x in checks):raise ValueError('Non-finite deformation.')
        severe=sum(x['severe_edge_strain'] for x in checks)
        if severe:warnings.append(f'{obj.name}: movement probes found {severe} severe edge-strain observations. Review the converted model before accepting; these are diagnostics, not proof of a broken mesh.')
        exported.append({'name':obj.name,'positions':points,'normals':normals,'uvs':uvs,'colors':colors,
                         'weights':weights,'faces':polygons,'deformation':checks,'tangents':tangents})
    weapon_receipt=None
    if weapon_meshes:
        fitted=weapon_fit.apply(bpy,weapon_meshes,weapon_plan,[b['name'] for b in target],mat_paths)
        for mesh in fitted:
            weights,stats=normalize_weights(mesh['weights'],{n:n for n in target_matrices},target_matrices,options.get('influence_limit',4))
            mesh['weights']=weights;totals.append(stats)
        exported.extend(fitted)
        meshes=meshes+weapon_meshes
        weapon_receipt={'policy':weapon_fit.POLICY,'meshes':[m['name'] for m in fitted],'objects':weapon_plan['objects'],
                        'action':weapon_plan.get('action','replace'),'skin':weapon_plan['skin']['kind']}
        warnings.append('The gun ('+', '.join(weapon_plan['objects'])+') was fitted to the hero weapon and skinned to '+
                        (weapon_plan['skin'].get('bone') or 'the original weapon bones')+'. Check grip, aim and muzzle position in game.')
    elif weapon_plan and weapon_plan.get('action')=='remove':
        weapon_receipt={'policy':weapon_fit.POLICY,'meshes':[],'objects':weapon_plan['objects'],'action':'remove'}
        warnings.append('The gun ('+', '.join(weapon_plan['objects'])+') was left out of the converted character.')
    remove_workspace_helpers(bpy, physics_helpers)
    bpy.data.objects.remove(rig,do_unlink=True)
    if job.get('props'):
        props,prop_materials,prop_warnings=import_props(job,meshes)
        exported.extend(props);mats.update(prop_materials);warnings.extend(prop_warnings)
        warnings.append('Attached props follow existing bones rigidly. Their offsets are bone-local Source 2 model units; inspect fit before building.')
    final=make_rig(target)
    for obj in meshes:
        mod=obj.modifiers.new('Deadlock skin','ARMATURE');mod.object=final;obj.parent=final
        obj.matrix_parent_inverse=Matrix.Identity(4)
    exact_bind = store_bind_metadata(final, target)
    bpy.context.scene.unit_settings.system='METRIC'
    bpy.context.scene.unit_settings.scale_length=.0254  # Source 2 skeletal translations are inches.
    bpy.ops.wm.save_as_mainfile(filepath=str(Path(job['workspace'])/'prepared.blend'))
    return {'status':'ok','meshes':exported,'materials':mats,'weights':totals,'warnings':warnings,
            'blend':str(Path(job['workspace'])/'prepared.blend'),'target':target,'review_required':True,
            'unit_scale':unit_scale,'rest_alignment':alignment_audit,'arm_skinning':arm_skin_audit,'preserved_physics_bones':kept,'bone_mapping':{'mapped_source_bones':len(mapping),'unused_source_bones':sorted(set(source)-set(mapping)),
                            'final_target_bones':len(target)},'exact_bind':exact_bind,'correction_bind':correction_bind,
            'deformation_status':'WARNING' if any(c['status']=='WARNING' for m in exported for c in m['deformation']) else 'PASS',
            'weapon':weapon_receipt}


def prepare_weapon_model(job,meshes,rigs):
    """A separate weapon model: export only the fitted gun against its own skeleton."""
    physics_helpers = mmd_helpers(bpy.context.scene.objects)
    target=job['target'];options=job.get('options',{})
    if not options.get('reviewed'):raise ValueError('Confirm the weapon fit before preparation.')
    target_matrices={b['name']:Matrix(b['matrix']) for b in target}
    names=[b['name'] for b in target]
    warnings=[]
    if job.get('correction'):
        # The prepared scene already holds the gun in weapon-model space.
        weapon_meshes=list(meshes)
        mats,mat_paths,mw=texture_and_materials(weapon_meshes,Path(job['workspace'])/'textures',job['hero_root']);warnings+=mw
        exported=[]
        for obj in weapon_meshes:
            mesh=obj.data
            rows=skin_rows(obj,set(names))
            if any(not row for row in rows):raise ValueError('Every corrected weapon vertex must stay weighted to a weapon bone: '+obj.name)
            if hasattr(mesh,'calc_normals_split'):mesh.calc_normals_split()
            corners=mesh.corner_normals if hasattr(mesh,'corner_normals') else None
            to_world=np.linalg.inv(np.asarray(obj.matrix_world.to_3x3())).T
            normals=[]
            for loop in mesh.loops:
                n=to_world@np.asarray(corners[loop.index].vector if corners else loop.normal);normals.append((n/np.linalg.norm(n)).tolist())
            tangents=[]
            if mesh.uv_layers:
                mesh.calc_tangents(uvmap=mesh.uv_layers[0].name)
                tangents=[list(loop.tangent)+[loop.bitangent_sign] for loop in mesh.loops]
            exported.append({'name':obj.name,'positions':[list(obj.matrix_world@v.co) for v in mesh.vertices],'normals':normals,
                             'uvs':[[list(x.uv) for x in layer.data] for layer in mesh.uv_layers],
                             'colors':[[list(layer.data[l.vertex_index if layer.domain=='POINT' else l.index].color) for l in mesh.loops] for layer in mesh.color_attributes],
                             'weights':rows,'faces':[{'vertices':list(p.vertices),'corners':list(p.loop_indices),
                                 'material':mat_paths[obj.material_slots[p.material_index].material.name]} for p in mesh.polygons],
                             'deformation':[],'tangents':tangents})
        plan_objects=[m.name for m in weapon_meshes]
    else:
        plan=job.get('weapon')
        if not plan:raise ValueError('Choose the gun in the Weapon tab before converting.')
        body,weapon_meshes=weapon_fit.split(meshes,plan)
        for obj in body:bpy.data.objects.remove(obj,do_unlink=True)
        mats,mat_paths,mw=texture_and_materials(weapon_meshes,Path(job['workspace'])/'textures',job['hero_root']);warnings+=mw
        exported=weapon_fit.apply(bpy,weapon_meshes,plan,names,mat_paths)
        plan_objects=plan['objects']
        warnings.append('Only the gun ('+', '.join(plan_objects)+') is exported for this weapon model; the character is ignored here. Check grip, aim and muzzle position in game.')
    totals=[]
    for mesh in exported:
        weights,stats=normalize_weights(mesh['weights'],{n:n for n in names},target_matrices,options.get('influence_limit',4))
        mesh['weights']=weights;totals.append(stats)
    remove_workspace_helpers(bpy, physics_helpers)
    for rig in rigs:
        if rig.name in bpy.context.scene.objects:bpy.data.objects.remove(rig,do_unlink=True)
    final=make_rig(target)
    for obj in weapon_meshes:
        for m in [m for m in obj.modifiers if m.type=='ARMATURE']:obj.modifiers.remove(m)
        mod=obj.modifiers.new('Deadlock skin','ARMATURE');mod.object=final;obj.parent=final
        obj.matrix_parent_inverse=Matrix.Identity(4)
    exact_bind = store_bind_metadata(final, target)
    bpy.context.scene.unit_settings.system='METRIC'
    bpy.context.scene.unit_settings.scale_length=.0254
    bpy.ops.wm.save_as_mainfile(filepath=str(Path(job['workspace'])/'prepared.blend'))
    return {'status':'ok','meshes':exported,'materials':mats,'weights':totals,'warnings':warnings,
            'blend':str(Path(job['workspace'])/'prepared.blend'),'target':target,'review_required':True,'unit_scale':1.,
            'rest_alignment':{'policy':weapon_fit.POLICY,'weapon_model':True,'target_bind_matrices_modified':False},
            'bone_mapping':{'mapped_source_bones':0,'unused_source_bones':[],'final_target_bones':len(target)},
            'exact_bind':exact_bind,'correction_bind':None,'deformation_status':'PASS',
            'weapon':{'policy':weapon_fit.POLICY,'meshes':[m['name'] for m in exported],'objects':plan_objects,'action':'weapon_model'}}


def main():
    job=json.loads(Path(sys.argv[sys.argv.index('--')+1]).read_text(encoding='utf-8'))
    try:
        helper_reports=[]
        shape_mode=job.get('options',{}).get('shape_key_mode','')
        if shape_mode not in {'','current_static','basis_static'}:
            raise ValueError('Unsupported Shape key handling choice.')
        if job['operation']=='stage_blend':
            # Snapshot the file exactly as saved. Character isolation runs on
            # every later inspection/preparation, so it is never baked into
            # the workspace copy (and never applied twice).
            bpy.ops.wm.open_mainfile(filepath=job['source'], load_ui=False, use_scripts=False)
            if len(bpy.data.scenes)!=1:
                helper_reports.append({'policy':'active-scene-only-v1','rigid_body_meshes':0,'joint_empties':0,'objects':[],
                    'warning':'This Blender file has '+str(len(bpy.data.scenes))+' scenes. Only the active scene ('+bpy.context.scene.name+') is used.'})
            if any(METADATA_KEY in o for o in bpy.context.scene.objects if o.type=='ARMATURE'):
                raise ValueError('This is an already prepared Deadlock correction file. Use Re-export corrected Blender file for this project, or select the original replacement source for a new preparation.')
            helpers=mmd_helpers(list(bpy.context.scene.objects))
            if helpers:helper_reports.append(helper_receipt(helpers))
            report=snapshot_blend(bpy,job['source'],job['destination'])
            report['source_helpers']=helper_reports
            Path(job['result']).write_text(json.dumps(report,allow_nan=False),encoding='utf-8')
            return
        scene_choices=job.get('scene') if job['operation']=='inspect' else job.get('options',{}).get('scene')
        if job.get('correction'):
            scene_choices=None
        meshes,rigs=import_source(job['source'],allow_shape_keys=job['operation'] in {'inspect','recover_materials'} or bool(job.get('face')) or shape_mode in {'current_static','basis_static'},helper_reports=helper_reports,scene=scene_choices)
        if job['operation']=='recover_materials':
            materials,_,warnings=texture_and_materials(meshes,Path(job['workspace'])/'textures',job['hero_root'])
            Path(job['result']).write_text(json.dumps({'status':'ok','materials':materials,'warnings':warnings},allow_nan=False),encoding='utf-8')
            return
        face_receipt=[];shape_receipt=[]
        if job['operation']=='prepare' and job.get('face'):
            face_receipt=apply_static_face(meshes,job['face'])
        elif job['operation']=='prepare' and shape_mode:
            shape_receipt=apply_static_shapes(meshes,shape_mode)
        if job['operation']=='prepare':
            # Generative modifiers on shape-keyed meshes wait for static baking.
            apply_pending_modifiers(bpy,meshes)
        if job.get('garment_source'):
            meshes,rigs=import_garment(job['garment_source'],meshes,rigs,helper_reports)
        if job.get('rotation'):
            # Explicit input orientation, applied to root objects once.
            from mathutils import Euler
            rot=Euler(tuple(math.radians(v) for v in job['rotation']),'XYZ').to_matrix().to_4x4()
            for obj in bpy.context.scene.objects:
                if not obj.parent:obj.matrix_world=rot@obj.matrix_world
            bpy.context.view_layer.update()
        if job['operation']=='inspect':
            report=describe(meshes,rigs)
        elif job.get('mode')=='weapon':
            report=prepare_weapon_model(job,meshes,rigs)
        else:
            report=prepare(job,meshes,rigs)
        report['status']='ok'
        report['source_helpers']=helper_reports
        report.setdefault('warnings',[]).extend(item['warning'] for item in helper_reports if item['warning'])
        if face_receipt:
            report['static_face']=face_receipt
            report.setdefault('warnings',[]).append('Facial shapes were baked into the static rest shape. Runtime facial morphs/expressions are not exported; surface normals were recomputed.')
        if shape_receipt:
            report['static_shapes']=shape_receipt
            report.setdefault('warnings',[]).append('Shape keys were baked into one static rest shape using the reviewed setting. Animated morphs/expressions are not exported; the original source file retains its shape keys. Surface normals were recomputed.')
        Path(job['result']).write_text(json.dumps(report,allow_nan=False),encoding='utf-8')
    except Exception as ex:
        Path(job['result']).write_text(json.dumps({'status':'error','error':str(ex)}),encoding='utf-8')
        raise

if __name__=='__main__': main()
