/** Native setup wire types. Integrity and native compatibility remain separate evidence. */
export interface SetupOptions {dd:string;toolroot:string;manifests:string}
export interface SetupDefaults {options:SetupOptions;export_root:string;automatic_root:string}
export type AcquireAction='manifests'|'download'|'verify';
export type AcquireAuth={mode:'qr'}|{mode:'username';username:string};
export interface SelectedDepot {depot:string;manifest:string;whole_depot_bytes:number;selected_files:number;selected_bytes:number}
export interface ManifestFile {path:string;bytes:number;sha1:string;directory:boolean;depot?:string}
export interface AcquisitionPlan {
  schema:number;kind:string;steam_build:string;branch_snapshot_utc:string;depots:SelectedDepot[];
  manifest_sha256:Record<string,string>;filelists:Record<string,string[]>;
  transfer_uncompressed_bytes:number;installed_bytes:number;files:ManifestFile[];
  overridden_paths:Record<string,string[]>;native_compile_passed:boolean;absolute_minimum_proven:boolean;
  scope:string;destination?:string;download_estimate_note?:string;estimated_download_bytes:number|null;
}
export interface IntegrityReceipt {steam_build:string;files_verified:number;bytes_verified:number;manifest_integrity_passed:boolean;native_compile_passed:boolean;absolute_minimum_proven:boolean}
export interface AcquisitionResult {plan:AcquisitionPlan;plan_digest:string;downloaded:boolean;receipt:IntegrityReceipt|null}
export interface ManualPreview {
  guide:string;commands:string;argv:Record<string,{metadata:string[];downloads:string[][];metadata_command:string;download_commands:string[]}>;
  plan:unknown;plan_digest:string|null;expected_files:string;export_dir:string|null;compatible:boolean;downloaded:boolean;
  /** The pinned depots in full, and the selected files once a plan was exported. */
  whole_bytes:number;selected_bytes:number|null;
}
export interface NativeChecks {native_modeldoc41:boolean;original_compiled:boolean;gameplay_animgraph2:boolean;ui_animgraph2:boolean;nm_skeleton:boolean}
export interface NodeEvidence {class:string;name:string;filename:string}
export interface ValidationEvidence {
  native:boolean;kind:string;compiled:boolean;compiled_model:string;compiled_sha256:string;
  ag2_nodes_expected:NodeEvidence[];ag2_nodes_checked:NodeEvidence[];ag2_output_checked:boolean;
  format_target:number|null;format_native:boolean;native_migration_verified:boolean;
}
export interface ValidationReport {
  schema:number;kind:string;compatible:boolean;downloaded:boolean;toolset_modified:boolean;source_toolset:string;hero:string;
  plan_digest:string;recipe_sha256:string;report_directory:string;validation_copy_directory:string;validation_copy_removed:boolean;
  native_test:string;integrity:IntegrityReceipt|null;static_audit:unknown;native_result:ValidationEvidence|null;checks:NativeChecks|null;
  source_reverification:IntegrityReceipt|null;error:string|null;cleanup_error:string|null;scope:string;
}
