/** Native preparation IPC. Large prepared vertex/face arrays stay on disk. */
// i18n-area: prepare
import type { JsonValue, Material, PropChoices } from '../models';
import { m, t, tc } from '../i18n';
export type Vec3 = [number,number,number];
export type Quat = [number,number,number,number];
export type Matrix4 = [Quat,Quat,Quat,Quat];
export type PreparationMode = 'prepared' | 'rigged_humanoid' | 'unrigged_humanoid' | 'weapon';
export const ROLES = ['pelvis','spine','chest','neck','head','upperarm_L','forearm_L','hand_L','thigh_L','shin_L','foot_L','upperarm_R','forearm_R','hand_R','thigh_R','shin_R','foot_R'] as const;
export type Role = typeof ROLES[number];
export interface Bone {name:string;parent:string|null;position:Vec3;matrix?:Matrix4;deform?:boolean;physics?:{policy:string;dynamic_bodies:string[];kinematic_bodies:string[]};/** Gun candidates this bone belongs to (marked at inspection); charms hang from these. */weapon?:string[]}
export interface PreparedBone extends Bone {local_position:Vec3;local_rotation:Quat;local_scale:number;matrix:Matrix4;inverse_bind_matrix:Matrix4;length:number}
export interface Skeleton {m_boneName:string[];m_nParent:number[];m_bonePosParent:Vec3[];m_boneRotParent:Quat[];m_boneScaleParent:number[];m_nFlag:number[]}
export interface Identification {roles:Partial<Record<Role,string>>;missing:Role[];errors:string[];status:string;evidence:string}
export interface LandmarkEstimate {landmarks?:Partial<Record<Role,Vec3>>;bounds?:[Vec3,Vec3];status?:string;explanation?:string;errors?:string[]}
export type SceneStatus = 'character'|'included'|'repaired'|'bound'|'converted'|'excluded'|'ignored';
export interface SceneObject {name:string;type:string;status:SceneStatus|string;reason:string;bone?:string}
export interface SceneReceipt {policy:string;character_rig:string|null;objects:SceneObject[];notes:string[];warning:string}
export type SceneChoices = {exclude?:string[];include?:string[];character?:string};
export type Hand = 'right'|'left';
/** Model fitting → Feet on the floor; the same choices as `retarget.py FLOOR_MODES` and Rust `fitting::FLOOR_MODES`. */
export type FloorFit = 'ankles'|'legs'|'lift';
export const FLOOR_CHOICES: [FloorFit,string][] = [
  ['ankles','Line up the ankles (default)'],
  ['legs','Shorten or lengthen the legs'],
  ['lift','Move the whole character up or down'],
];
/** The help line of each Feet on the floor card (English; translated where it is drawn). */
export const FLOOR_HELP: Record<FloorFit,string> = {
  ankles: m('The fit lines up your model’s ankles with the hero’s, as before. Feet taller than the hero’s (heels, thick soles) then reach below the floor, shorter ones float.'),
  legs: m('Scales everything below the hips up or down so the soles touch the floor. The hips, body and arms stay on the hero’s joints, but your model’s knees and ankles move off the hero’s (up when the legs get shorter), so the ankle movement can look a little off.'),
  lift: m('Moves the whole model up or down so the soles touch the floor. Nothing is stretched, but the body, arms and hands sit that much off the hero’s joints (and the gun).'),
};
/** Model fitting → Leg joints (Preserve source proportions only); the same choices as `retarget.py LEG_JOINT_MODES` and Rust `fitting::LEG_JOINT_MODES`. */
export const LEG_JOINT_MODES = ['pelvis','hips','scaled','scaled_joints','joints'] as const;
export type LegJointFit = typeof LEG_JOINT_MODES[number];
/** Choices that put the legs on the hero's joints: Feet on the floor → Shorten or lengthen the legs is refused with them. */
export const LEGS_ON_HERO_JOINTS: readonly LegJointFit[] = ['scaled_joints','joints'];
export const LEG_JOINT_CHOICES: [LegJointFit,string][] = [
  ['pelvis','As before (body placed by the pelvis bone) · default'],
  ['joints','Hip joints, and legs on the hero’s joints (no resizing) · recommended if the knees stretch'],
  ['hips','Place the body by the hip joints'],
  ['scaled','Hip joints, and resize the character to the hero’s leg length'],
  ['scaled_joints','Hip joints, resize, and legs on the hero’s joints'],
];
/** The help line of each Leg joints card (English; translated where it is drawn). The two
 * choices that resize the character end with the same caution about stretched shoulders. */
export const LEG_JOINT_HELP: Record<LegJointFit,string> = {
  pelvis: m('The body is placed by your model’s pelvis bone, as before. When that bone sits away from the hip joints, the body can lean a little and the knees and ankles sit off the hero’s, so knees can stretch in deep bends (jumping, crouching). If they do, try the recommended choice: Hip joints, and legs on the hero’s joints (no resizing).'),
  hips: m('Places and turns the body by its hip joints instead of the pelvis bone. Nothing is resized; the knees and ankles usually come closer to the hero’s, but legs longer or shorter than the hero’s still leave them a little off.'),
  scaled: m('Also scales the whole character evenly so its legs are as long as the hero’s. The character becomes smaller or larger by the ratio of the leg lengths, head and arms included, so the upper body moves relative to the hero’s skeleton and hands and head sit a little differently. Resizing can stretch the shoulders while the arms are aligned to the hero’s joints, so try it only when the other choices do not suit your character.'),
  scaled_joints: m('Scales the character as above and puts the hips, knees and ankles exactly on the hero’s joints. Thighs and shins are lengthened or shortened to the hero’s (their thickness stays), so the knees bend where the hero’s do; the upper body moves relative to the hero’s skeleton. Resizing can stretch the shoulders while the arms are aligned to the hero’s joints, so try it only when the other choices do not suit your character.'),
  joints: m('The one to try when the default leaves the knees stretching. Puts the hips, knees and ankles exactly on the hero’s joints without resizing the character. Thighs and shins are lengthened or shortened to the hero’s (their thickness stays); the upper body keeps its size and moves relative to the hero’s skeleton, so hands and head sit a little differently.'),
};
/** Two whole sentences shown one after the other (a space in English, none in Chinese). */
export const nextSentence=(first:string,next:string)=>tc('sentences','{first} {next}',{first,next});
/** What a conversion measured for Leg joints (`rest_alignment.leg_joint_fit`, inches). */
export interface LegFitAudit {policy:string;mode:LegJointFit;applied:boolean;ignored_reason?:string|null;body_scale:number;length_scale?:Record<string,number>;lean_degrees?:number;joint_offsets_inches:Record<string,number>;knee_gap_at_90_degrees_inches?:number|null;measured_after_floor?:boolean}
/** One plain line about the measured leg fit, or null without measurements. */
export function legFitLine(audit:LegFitAudit|null|undefined):string|null{
  if(!audit)return null;
  const o=audit.joint_offsets_inches??{},knees=[o.knee_L,o.knee_R].filter((v):v is number=>typeof v==='number');
  if(!knees.length)return null;
  const knee=Math.max(...knees),gap=audit.knee_gap_at_90_degrees_inches??0,size=Math.round((audit.body_scale??1)*100);
  const lengths=audit.length_scale??{},thigh=lengths.thigh_L,shin=lengths.shin_L;
  const values={knee:knee.toFixed(1),gap:gap.toFixed(1),size};
  let line=gap>=0.05&&knee>=0.05?t('This conversion: knees {knee} in from the hero’s knee joints, about {gap} in of stretch in a 90° bend. Character size {size}%.',values)
    :t('This conversion: knees {knee} in from the hero’s knee joints. Character size {size}%.',values);
  if((audit.mode==='scaled_joints'||audit.mode==='joints')&&typeof thigh==='number'&&typeof shin==='number')line=nextSentence(line,t('Thighs {thigh}%, shins {shin}% of their length.',{thigh:Math.round(thigh*100),shin:Math.round(shin*100)}));
  if(audit.ignored_reason)line=nextSentence(line,t('Leg joints is not used for this hero (its unusual proportions have their own fit).'));
  return line;
}
/** A small triangle mesh: x, y, z per vertex and three vertex indices per triangle. */
export interface Shape {positions:number[];triangles:number[]}
/** gun_file: the gun comes from the Gun tab's gun file. attach_kind "placed": moved into the hand by its grip. side: the hand of a gun-file pair. */
export interface WeaponCandidate {id:string;objects:string[];vertices:number;bounds:[Vec3,Vec3];sample:Vec3[];attach_bone?:string|null;attach_kind:string;keywords:string[];elongation:number;gun_file?:boolean;/** Deform bones the gun objects are weighted to. */bones?:string[];side?:Hand|null;/** Centres of the parts named for the front, rear and grip. */hints?:{front?:Vec3;rear?:Vec3;grip?:Vec3};/** The gun as a small solid mesh for the fit preview. */shape?:Shape}
/** One gun of a gun-file pair: where it went and the charm bones it keeps. */
export interface GunFileGun {side:Hand|null;objects:string[];bone?:string|null;hand?:string|null;placed:boolean;charm_bones?:string[]}
/** What Blender did with the gun file: its kept objects and the bone holding them (each gun of a pair under guns). */
export interface WeaponFileReceipt {policy:string;file:string;objects:string[];bone?:string|null;hand?:string|null;placed:boolean;guns?:GunFileGun[];charm_bones?:string[]}
export interface WeaponFileRecord {selection:{source:string;source_files?:SourceProof[]};source:string;source_sha256:string;hand?:string|null;left_hand?:string|null}
/** One gun of a dual-wield hero (points are omitted here). */
export interface WeaponSide {side:Hand;meshes:string[];primary:string[];bones:string[];shape?:Shape}
export interface WeaponReference {policy:string;source:'meshes'|'skeleton'|string;separate_model:boolean;meshes:string[];primary:string[];model_meshes:string[];bones:string[];notes:string[];/** A gun in each hand (Haze). */sides?:WeaponSide[];/** Gun meshes whose files could not be read. */unreadable?:string[];/** The hero's gun(s) and body as small solid meshes for the fit preview. */shape?:Shape;body_shape?:Shape;/** The hero holding its gun (first frame of its idle clip): the posed body and each hand's rest-to-stance matrix. */stance?:WeaponStance}
export interface WeaponStance {clip:string;body_shape:Shape;hands:Partial<Record<Hand,number[][]>>}
/** The left hand of a dual-wield hero: your gun for it (empty = a copy of the right gun), or keep the hero's own gun. */
export type WeaponHand = {objects?:string[];keep?:boolean;flip?:boolean;roll?:number;/** Degrees the muzzle tilts up and turns to the gun's right, around the grip. */pitch?:number;yaw?:number;scale?:number;offset?:Vec3;bone?:string};
/** Saved as options.weapon. action: replace = fit to the hero's gun; remove = leave out; keep = follow the hand bones. The adjustments are the right (or only) gun's. */
export type WeaponChoice = {enabled?:boolean;action?:''|'replace'|'remove'|'keep';objects?:string[];fit?:'match'|'keep_size';flip?:boolean;roll?:number;/** Degrees the muzzle tilts up and turns to the gun's right, around the grip. */pitch?:number;yaw?:number;scale?:number;offset?:Vec3;skin?:'auto'|'rigid';bone?:string;left?:WeaponHand;/** Join each gun's parts into one mesh (default on). */merge?:boolean};
export interface WeaponFrame {grip:Vec3;muzzle:Vec3;forward:Vec3;up:Vec3;length:number;confidence:string;notes:string[];/** How the barrel direction was found: hint, names, grip, shape, bones, flipped or flipped_names (flipped against the part names). */direction?:string}
export interface WeaponFit {policy:string;source:WeaponFrame;target:WeaponFrame;matrix:number[][];scale:number;muzzle_offset_mm:number;skin:string;skin_bones:string[];notes:string[]}
/** One hand of the preview; no fit when the hero keeps its own gun in that hand. */
export interface WeaponHandPreview {side:Hand;fit:WeaponFit|null;fitted:Vec3[];reference:Vec3[]}
export interface WeaponPreview {fit:WeaponFit;fitted:Vec3[];reference:Vec3[];/** Both hands of a dual-wield hero. */hands?:WeaponHandPreview[];/** The hero's body around the guns. */body?:Vec3[]}
/** Bone weights without their skeleton (worker.py `missing_skeleton`): the armature was deleted or never saved with the mesh. */
export interface MissingSkeleton {policy:string;/** Vertex groups that carry weights on the kept character meshes. */groups:number;meshes:string[];/** An armature modifier without its object was on one of them. */modifier:boolean}
export interface Inspection {/** Present only when the file has bone weights but no skeleton. */missing_skeleton?:MissingSkeleton;scene?:SceneReceipt;weapons?:WeaponCandidate[];source_helpers?:{policy:string;rigid_body_meshes:number;joint_empties:number;warning:string;objects:{name:string;kind:string}[]}[];warnings?:string[];armatures:number;bones:Bone[];vertices:number;bounds:[Vec3,Vec3];meshes:{name:string;vertices:number;polygons:number;materials:string[];disconnected_components:number}[];morphs:{name:string;mesh:string;shape:string}[];weighted_bones:string[];suggested_mode:PreparationMode;humanoid:Identification|LandmarkEstimate;status:string;/** Result of adding breast bones for breast motion. */breast_bones?:{policy:string;status:string;reason?:string;bones?:string[]};weapon_file?:WeaponFileReceipt;/** The source rig's own MMD/VRM spring settings by bone (source_springs.py). */springs?:Record<string,import('./springs').SpringValues>}
export interface FollowProposal {anchor:string;target:string;group:string;automatic:boolean;reason:string}
export interface MappingEntry {source:string;target:string|null;group:string;label:string;reason:string;automatic:boolean;follow?:FollowProposal}
export interface MappingAssistance {policy:string;entries:MappingEntry[];labels:Record<string,string>;warnings:string[];anchors?:Record<string,string>;arm_deformation?:Record<string,{target:string;role:Role;kind:'shoulder'|'twist'}>;preset?:string}
export interface Suggestions {source?:Identification;target:Identification;mapping:Record<string,string>;assistance?:MappingAssistance}
export interface ScaleAudit {policy:string;bone_count:number;local_scale_tolerance:number;world_scale_tolerance:number;world_joint_tolerance_mm:number;max_local_scale_delta:number;max_world_scale_delta:number;max_world_joint_delta_mm:number;max_omitted_scale_world_joint_drift_mm:number;max_omitted_scale_world_scale_drift:number;approximated_bones:number;scaled_bones:number;bones:{name:string;original_scale:number;export_scale:number;local_scale_delta:number;cumulative_scale:number;original_cumulative_scale:number;world_scale_delta:number;world_joint_delta_mm:number;omitted_scale_world_joint_drift_mm:number}[];original_skeleton_modified:boolean;exact_scale_preservation:boolean;world_inverse_bind_consistent:boolean;native_compiler_validated:boolean}
export type SourceProof = {relative:string;sha256:string};
export interface ClothingSelection {source:string;mode:string;source_files?:SourceProof[]}
export interface StaticFace {enabled?:boolean;mode?:string;weights?:Record<string,number>}
export interface PropSelection extends PropChoices {mode?:string;source_files?:SourceProof[]}
export interface PropRecord {selection:PropSelection;source:string;source_sha256:string;analysis:Inspection}
export interface GeometryRecord {props:PropRecord[];face:StaticFace|null}
export interface PreparationAnalysis {source:string;source_sha256:string;source_original:string;source_input:{received:string;resolved:string;cwd:string};root:string;target_model:string;target_sha256:string;target_skeleton:Skeleton;target:PreparedBone[];target_scale_audit:ScaleAudit;analysis:Inspection;suggestions:Suggestions;rotation:Vec3;geometry_customization:GeometryRecord;clothing:{selection:ClothingSelection;source:string;source_sha256:string}|null;weapon_reference?:WeaponReference|null;weapon_suggestion?:WeaponChoice|null;/** Breast bones added in Blender for this inspection. */breast_generation?:JsonValue|null;/** The gun file this inspection added. */weapon_file?:WeaponFileRecord|null}
/** Keep unfinished review values while saving; native review determines readiness. */
export interface PreparationOptions {source_roles?:Partial<Record<Role,string>>;target_roles?:Partial<Record<Role,string>>;mapping?:Record<string,string>;mapping_anchors?:Record<string,string>;follow_motion_reviewed?:boolean;preserve_physics_bones?:boolean;physics_roots?:string[];physics_offered?:string[];automatic_secondary_motion?:boolean;/** Optional gentle breast motion (off by default). */breast_motion?:boolean;/** Optional butt motion, separate from breasts (off by default). */butt_motion?:boolean;/** Part types chosen during preparation, for example breast. */physics_kinds?:Record<string,string>;shape_key_mode?:''|'current_static'|'basis_static';fit_mode?:'match_target'|'preserve_proportions';fit_arm_joints?:boolean;/** Bend the fingers to the hero's resting curl (default on). */fit_finger_curl?:boolean;/** Model fitting → Feet on the floor (default ankles: the fit as before). */fit_floor?:FloorFit;/** Model fitting → Leg joints, Preserve source proportions only (default pelvis: the fit as before). */fit_leg_joints?:LegJointFit;arm_skinning?:'source'|'smooth_twist';landmarks?:Partial<Record<Role,number[]>>;influence_limit?:number;reviewed?:boolean;complete_mapping?:boolean;scene?:SceneChoices;weapon?:WeaponChoice;/** Gun file: take the gun from this file instead of the character file. */weapon_source?:string;[key:string]:JsonValue|undefined}
/** Review issues the page fixes in one click (Rust `following::STALE_KIND`, `secondary_bones::STALE_GROUPS_KIND`). */
export type IssueKind='stale_following'|'stale_physics_groups';
export interface ReviewIssue {tab:string;message:string;scope?:string;roles?:string[];bones?:string[];/** Set for a problem with a one-click fix (an `IssueKind`). */kind?:string}
export interface PreparationResult {dmx:string;blend:string;report:string;textures:string;target_model:string;warnings:string[];/** Leg joints as this conversion measured it (Preserve source proportions). */leg_fit?:LegFitAudit|null}
export interface PreparationReview {target_model:string;dmx_sha256:string;options:PreparationOptions;warnings:string[];attached_prop_meshes:string[];materials:Record<string,Material>;meshes:{name:string;vertices:number;polygons:number}[];rest_alignment_policy:string;native_compiler_validated:boolean;in_game_validated:boolean}
export interface ResumedPreparation {analysis:PreparationAnalysis;result:PreparationResult;report:PreparationReview}
