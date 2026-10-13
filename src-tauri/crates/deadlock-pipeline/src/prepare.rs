//! Prepare only builder-owned source trees from a fresh inspection receipt.
use crate::{
    inspect::{hash, map_error, material_error, InspectionReport, ModelPipeline},
    invalid,
    sources::{files, read_text},
    vpk_error,
};
use deadlock_core::{safety, CancellationToken, Result};
use deadlock_dmx::{split, Dmx};
use deadlock_model::{self as model, SourcePaths};
use deadlock_project::{LodStrategy, Project};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[derive(Debug, Serialize)]
pub struct PreparedModel {
    pub content: PathBuf,
    pub game: PathBuf,
    pub model: String,
    pub generated_materials: Vec<String>,
    pub mesh_sources: BTreeMap<String, String>,
    pub dmx_sha256: String,
    pub automatic_split: Option<split::SplitReport>,
    pub tested_physics: Option<crate::motion::TestedContract>,
}
impl ModelPipeline<'_> {
    pub fn managed_addon(&self, p: &Project) -> Result<(PathBuf, PathBuf)> {
        if self.settings.csdk.is_empty() || !Path::new(&self.settings.csdk).is_dir() {
            return Err(invalid("Choose the Reduced CSDK12 folder in Setup."));
        }
        deadlock_vpk::check_unlinked_path(Path::new(&self.settings.csdk), false)
            .map_err(vpk_error)?;
        let csdk = safety::resolve_path(Path::new(&self.settings.csdk))?;
        if !self.settings.deadlock.is_empty()
            && csdk.starts_with(safety::resolve_path(Path::new(&self.settings.deadlock))?)
        {
            return Err(invalid(
                "CSDK must be a separate installation from the original Deadlock game.",
            ));
        }
        p.validate_action().map_err(map_error)?;
        let addon = format!("dmb_{}", &p.id[..16]);
        Ok((
            safety::inside(&csdk, &format!("content/citadel_addons/{addon}"))?,
            safety::inside(&csdk, &format!("game/citadel_addons/{addon}"))?,
        ))
    }
    /// Caller holds the workspace lease for the encompassing operation. This
    /// method never approves a deserialized inspection or modifies source inputs.
    pub fn prepare(
        &self,
        p: &mut Project,
        report: &InspectionReport,
        offline: bool,
    ) -> Result<PreparedModel> {
        self.clock.measure("Prepare model and materials",None,|parent|{
            report.verify_inputs(p,self.cancel)?;
            if report.needs_review{return Err(invalid("Review mesh mappings, material confirmations, LOD choices and unskinned meshes before building. See inspection.json."));}
            if !p.customization.model_enabled(){return Err(invalid("Use component preparation for an assets-only project."));}
            let root=self.root(p)?;
            let dmx=self.clock.measure("Read and verify replacement DMX",Some(parent.id()),|_|{
                let dmx=Dmx::load_with_cancel(Path::new(&p.dmx),self.cancel).map_err(deadlock_dmx::DmxError::into_core)?;
                if hash(dmx.bytes())!=report.source_sha256{return Err(invalid("Replacement DMX changed after analysis."));}Ok(dmx)
            })?;
            if let Some(selected)=crate::motion::selection(p)?{
                let expected=report.original.tested_physics.as_ref().ok_or_else(||invalid("Analyze the matching rigged replacement and tested cloth before preparing output."))?;
                if expected.source_sha256!=report.source_sha256||selected!=expected.reference{return Err(invalid("Tested cloth selection changed after analysis."));}
                if safety::digest_with_cancel(&selected,self.cancel)?!=expected.selection.qualification.reference_sha256{return Err(invalid("Tested cloth reference changed after analysis."));}
            }
            let mut generated=BTreeMap::new();let mut template_targets=BTreeMap::new();
            if let Some(split)=&report.automatic_split{
                let folder=split.directory.as_deref().ok_or_else(||invalid("Generated mesh split has no qualified source directory."))?;
                self.clock.measure("Validate generated mesh split",Some(parent.id()),|_|split::validate_bundle(&dmx,split,folder,self.cancel).map_err(deadlock_dmx::DmxError::into_core))?;
                for group in &split.groups{for piece in &group.pieces{
                    generated.insert(piece.name.clone(),safety::inside(folder,&piece.file)?);
                    template_targets.insert(piece.name.clone(),group.source_mesh.clone());
                }}
            }else{self.clock.skip_step("Validate generated mesh split",Some(parent.id()),"No generated mesh split")?;}
            let(content,game)=if offline{(root.join("prepared-content"),root.join("prepared-game"))}else{self.managed_addon(p)?};
            crate::compilation::reject_generated_inputs(p,&[content.clone(),game.clone()])?;
            let mut paths=BTreeMap::new();
            self.clock.measure("Prepare source trees and ModelDoc",Some(parent.id()),|span|{
                crate::compilation::clear_owned(p,&[content.clone(),game.clone()],self.cancel)?;
                self.clock.measure("Copy original source tree",Some(span.id()),|_|copy_tree_verified(&report.effective_sources,&content,self.cancel))?;
                let source=safety::inside(&content,p.source_model())?;let original_text=report.original.verified_source(self.cancel)?;
                let patched=if !p.mesh_template.is_empty(){
                    let name=Path::new(&p.dmx).file_name().and_then(|n|n.to_str()).ok_or_else(||invalid("Replacement DMX filename is not Unicode."))?;
                    let source_path=safety::relative_asset(&format!("{}/dmx/{name}",p.hero_root()))?;
                    safety::atomic_write(&safety::inside(&content,&source_path)?,dmx.bytes())?;
                    for target in model::selected_meshes(&report.mappings).map_err(map_error)?{paths.insert(target,source_path.clone());}
                    for(target,file)in &generated{
                        let name=file.file_name().and_then(|n|n.to_str()).ok_or_else(||invalid("Generated DMX filename is not Unicode."))?;
                        let path=safety::relative_asset(&format!("{}/dmx/generated/{name}",p.hero_root()))?;
                        copy_file_verified(file,&safety::inside(&content,&path)?,self.cancel)?;paths.insert(target.clone(),path);
                    }
                    let template=read_text(Path::new(&p.mesh_template))?;
                    model::merge_template(&original_text,&template,&report.mappings,&SourcePaths::PerMesh(paths.clone()),p.lod_strategy,Some(&template_targets),Some(&report.original.source_audit())).map_err(map_error)?
                }else{
                    if p.lod_strategy==LodStrategy::Single{return Err(invalid("Single LOD requires a filtered source-package template. Choose a supported strategy."));}
                    for target in model::selected_meshes(&report.mappings).map_err(map_error)?{
                        self.cancel.check()?;let path=model::isolated_filename(&p.hero_root(),&target).map_err(map_error)?;let destination=safety::inside(&content,&path)?;
                        if let Some(file)=generated.get(&target){copy_file_verified(file,&destination,self.cancel)?;}else{
                            let bytes=self.clock.measure(format!("Isolate replacement mesh: {target}"),Some(span.id()),|_|dmx.single_mesh(&target).map_err(deadlock_dmx::DmxError::into_core))?;
                            safety::atomic_write(&destination,&bytes)?;
                        }paths.insert(target,path);
                    }
                    model::patch(&original_text,&report.mappings,&paths).map_err(map_error)?
                };
                let patched=self.author_secondary(p,report,&content,&source,&patched)?;
                safety::atomic_write(&source,patched.as_bytes())?;Ok(())
            })?;
            self.clock.measure("Save source provenance",Some(parent.id()),|_|{
                let mut provenance=BTreeMap::new();
                for(key,value)in [("material_mapping",&p.material_mapping),("material_remap",&p.material_remap),("mesh_template",&p.mesh_template),("skeleton_reference",&p.skeleton_reference)]{
                    if value.is_empty(){continue;}
                    let file=Path::new(value);let extension=file.extension().and_then(|s|s.to_str()).map(|s|format!(".{s}")).unwrap_or_default();
                    let destination=root.join("source-provenance").join(format!("{key}{extension}"));
                    copy_file_verified(file,&destination,self.cancel)?;
                    provenance.insert(key,serde_json::json!({"source":file,"sha256":safety::digest_with_cancel(&destination,self.cancel)?}));
                }
                safety::write_json(&root.join("source-provenance/manifest.json"),&provenance)
            })?;
            let mut created=if p.generate_materials{self.clock.measure("Generate material sources and textures",Some(parent.id()),|_|deadlock_materials::generate(&report.materials,&content,&p.hero_root(),self.cancel).map_err(material_error))?}
            else{
                for material in report.materials.keys(){if !safety::inside(&content,material)?.is_file(){return Err(invalid(format!("Missing material: {material}")));}}
                self.clock.skip_step("Generate material sources and textures",Some(parent.id()),"Existing material sources selected")?;vec![]
            };
            for changed in self.clock.measure("Apply material customization",Some(parent.id()),|_|deadlock_materials::patch_materials(&content,&report.materials,&p.customization,self.cancel).map_err(material_error))?{if !created.contains(&changed){created.push(changed);}}
            let texture_sizes=self.clock.measure("Adapt texture dimensions",Some(parent.id()),|_|deadlock_materials::normalize_staged_textures(&content,&report.materials.keys().cloned().collect::<Vec<_>>(),self.cancel).map_err(material_error))?;
            safety::write_json(&root.join("texture-dimensions.json"),&texture_sizes)?;
            if !texture_sizes.resized.is_empty(){(self.log)(&format!("Resized {} copied textures to power-of-two dimensions for the native compiler; original images and UVs are unchanged.",texture_sizes.resized.len()));}
            // Textures the compiler makes are named after their material; a
            // path past the Windows limit fails late and cryptically otherwise.
            self.clock.measure("Check compiler path lengths",Some(parent.id()),|_|deadlock_materials::check_native_paths(&content,&report.materials.keys().cloned().collect::<Vec<_>>()).map_err(material_error))?;
            // Recheck all original evidence after copying and generated writes.
            // A cancellation or changed source cannot publish a prepared receipt.
            self.clock.measure("Verify preparation source stability",Some(parent.id()),|_|report.verify_sources(self.cancel))?;
            p.meshes=model::equipment::project_mappings(&report.model,&report.source_mappings,&report.project_mappings,&p.customization).map_err(map_error)?;
            p.materials=report.materials.clone();
            p.save(&root.join("last-project.dmb.json")).map_err(map_error)?;
            let prepared=PreparedModel{content,game,model:p.source_model().into(),generated_materials:created,mesh_sources:paths,dmx_sha256:report.source_sha256.clone(),automatic_split:report.automatic_split.clone(),tested_physics:report.original.tested_physics.clone()};
            safety::write_json(&root.join("prepared.json"),&prepared)?;
            (self.log)("Model patched; skeleton, attachment, collision and animation source definitions retained. Runtime cloth is validated separately.");
            (self.log)(&format!("{} material sources generated",prepared.generated_materials.len()));Ok(prepared)
        })
    }
}

/// Stable checked copies reused by model compilation. Inputs and outputs are
/// hashed, and complete source inventory is checked before and after tree copy.
/// Copy one file and return its SHA-256 (the copy is verified against it).
pub(crate) fn copy_file_verified(
    source: &Path,
    destination: &Path,
    cancel: &CancellationToken,
) -> Result<String> {
    deadlock_vpk::check_unlinked_path(source, false).map_err(vpk_error)?;
    deadlock_vpk::check_unlinked_path(destination, true).map_err(vpk_error)?;
    safety::require_native_path(destination)?;
    safety::copy_verified(source, destination, cancel)
        .map_err(|_| invalid("Source changed while staging generated output."))
}
/// Copy a source tree; every file is read while it is copied and the tree is
/// hashed once afterwards, so a file that changed during the copy is caught.
pub(crate) fn copy_tree_verified(
    source: &Path,
    destination: &Path,
    cancel: &CancellationToken,
) -> Result<()> {
    let mut copied = BTreeMap::new();
    for file in files(source, cancel)? {
        let relative = file
            .strip_prefix(source)
            .map_err(map_error)?
            .to_str()
            .ok_or_else(|| invalid("Source path is not Unicode."))?
            .replace('\\', "/");
        if relative == ".deadlock-builder.json" {
            continue;
        }
        let hash = copy_file_verified(&file, &safety::inside(destination, &relative)?, cancel)?;
        copied.insert(relative, hash);
    }
    let mut after = crate::sources::inventory(source, cancel)?;
    after.remove(".deadlock-builder.json");
    if after != copied {
        return Err(invalid("Source tree changed during preparation."));
    }
    Ok(())
}
