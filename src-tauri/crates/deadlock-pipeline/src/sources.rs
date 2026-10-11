//! Original source cache and transitive compiler dependencies. These functions
//! never modify the original VPK or caller-imported source tree.
use crate::{invalid, vpk_error, workspace};
use deadlock_core::{
    process::ToolExecutor, safety, CancellationToken, CoreError, Result, TimingRecorder,
};
use deadlock_project::{Project, Settings};
use deadlock_tools::compiler;
use deadlock_vpk::{Hashes, Vpk};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct BaseFingerprint {
    pub model: String,
    pub cli: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pak_index_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pak: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files: Option<Hashes>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compiled: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compiled_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cli_sha256: Option<String>,
}

pub struct SourceService<'a> {
    pub settings: &'a Settings,
    pub runner: &'a dyn ToolExecutor,
    pub cancel: &'a CancellationToken,
    pub clock: &'a TimingRecorder,
    pub log: &'a dyn Fn(&str),
}

impl SourceService<'_> {
    pub fn decompile(
        &self,
        pak: &Path,
        resource: &str,
        destination: &Path,
        parent: Option<&str>,
    ) -> Result<()> {
        self.clock
            .measure("Decompile original resource", parent, |_| {
                self.cancel.check()?;
                let command = compiler::decompile_from_archive(
                    Path::new(&self.settings.source2viewer),
                    pak,
                    resource,
                    destination,
                )?;
                fs::create_dir_all(destination)
                    .map_err(|e| CoreError::io("Create decompile destination", e))?;
                self.runner.run(&command)?;
                self.cancel.check()
            })
    }
    pub fn ensure_base(&self, project: &Project) -> Result<PathBuf> {
        self.clock.measure("Prepare original hero sources", None, |span| {
            project.validate_action().map_err(|e| invalid(e.to_string()))?;
            let root = workspace::root(self.settings, project)?;
            let base = root.join("base");
            let pak = (!self.settings.pak.is_empty()).then(|| Path::new(&self.settings.pak)).filter(|p|p.is_file());
            let compiled = (!project.original_compiled.is_empty()).then(||Path::new(&project.original_compiled));
            let source = (!project.imported_source.is_empty()).then(||Path::new(&project.imported_source));
            if source.is_some_and(|p|!p.is_dir()) {return Err(invalid(format!("Imported source folder does not exist: {}", project.imported_source)));}
            if source.is_none() && pak.is_none() && !compiled.is_some_and(|p|p.is_file()) {return Err(invalid("Select Deadlock's pak01_dir.vpk in Setup, or import a decompiled source folder."));}
            let fingerprint = self.clock.measure("Fingerprint original source inputs", Some(span.id()), |_| {
                let mut signature = BaseFingerprint {model:project.original_model.clone(), cli:self.settings.source2viewer.clone(), ..Default::default()};
                if let Some(pak) = pak {signature.pak_index_sha256=Some(safety::digest_with_cancel(pak,self.cancel)?);}
                if let Some(source) = source {
                    signature.source=Some(source.canonicalize().map_err(|e|CoreError::io("Resolve original sources",e))?);
                    signature.files=Some(inventory(source,self.cancel)?);
                } else {
                    if let Some(pak) = pak {signature.pak=Some(pak.canonicalize().map_err(|e|CoreError::io("Resolve game archive",e))?);}
                    else if let Some(compiled) = compiled {
                        signature.compiled=Some(compiled.canonicalize().map_err(|e|CoreError::io("Resolve original compiled model",e))?);
                        signature.compiled_sha256=Some(safety::digest_with_cancel(compiled,self.cancel)?);
                    }
                    if Path::new(&self.settings.source2viewer).is_file() {signature.cli_sha256=Some(safety::digest_with_cancel(Path::new(&self.settings.source2viewer),self.cancel)?);}
                }
                Ok(signature)
            })?;
            let marker = root.join("base-fingerprint.json");
            if marker.is_file() && base.is_dir() {
                let saved: BaseFingerprint=serde_json::from_slice(&fs::read(&marker).map_err(|e|CoreError::io("Read base cache fingerprint",e))?)?;
                if saved==fingerprint {
                    self.clock.measure("Reuse cached original sources",Some(span.id()),|_|{self.cancel.check()?; (self.log)("Reusing cached original hero sources");Ok(())})?;
                    return Ok(base);
                }
            }
            if !project.auto_decompile && source.is_none() {return Err(invalid("Auto-decompile is disabled and this hero has no cached original sources."));}
            let temp=tempfile::Builder::new().prefix("base-pending-").tempdir_in(&root).map_err(|e|CoreError::io("Create original-source staging",e))?;
            if let Some(source)=source {self.clock.measure("Copy imported original sources",Some(span.id()),|_|safety::copy_tree(source,temp.path(),self.cancel))?;}
            else if let Some(pak)=pak {self.decompile(pak,&project.original_model,temp.path(),Some(span.id()))?;}
            else if let Some(compiled)=compiled {
                let destination=safety::inside(temp.path(),project.source_model())?;
                let command=compiler::decompile_resource(Path::new(&self.settings.source2viewer),compiled,&destination)?;
                if let Some(parent)=destination.parent(){fs::create_dir_all(parent).map_err(|e|CoreError::io("Create source-model folder",e))?;}
                self.clock.measure("Decompile imported compiled model",Some(span.id()),|_|self.runner.run(&command))?;
            }
            let main=safety::inside(temp.path(),project.source_model())?;
            if !main.is_file(){return Err(invalid(format!("Original hero VMDL could not be decompiled at its original path: {}. For imports select the folder containing models/.",project.source_model())));}
            deadlock_model::analyze(&read_text(&main)?).map_err(|e|invalid(e.to_string()))?;
            if let Some(pak)=pak {
                let archive=Vpk::open(pak).map_err(vpk_error)?;
                let mut seen=BTreeSet::new(); let mut pending=vec![main];
                while let Some(current)=pending.pop(){
                    self.cancel.check()?;
                    for reference in deadlock_model::source_references(&read_text(&current)?).map_err(|e|invalid(e.to_string()))? {
                        if !seen.insert(reference.clone()){continue;}
                        if seen.len()>10000 {return Err(invalid("Hero dependency graph exceeds 10,000 source resources."));}
                        let dest=safety::inside(temp.path(),&reference)?;
                        if (reference.ends_with(".vmdl")||reference.ends_with(".vmat"))&&!dest.exists()&&archive.entries.contains_key(&deadlock_core::text::casefold(&format!("{reference}_c"))) {
                            self.decompile(pak,&format!("{reference}_c"),temp.path(),Some(span.id()))?;
                        }
                        if reference.ends_with(".vmdl")&&dest.is_file(){pending.push(dest);}
                    }
                }
            }
            self.cancel.check()?;
            // Preserve the previous cache until the complete new source graph has
            // passed parsing; keep its backup if publication itself fails.
            if base.exists(){let backup=root.join("backups"); fs::create_dir_all(&backup).map_err(|e|CoreError::io("Create source backups",e))?; fs::rename(&base,backup.join(format!("base-{}",uuid::Uuid::new_v4().simple()))).map_err(|e|CoreError::io("Back up original-source cache",e))?;}
            fs::rename(temp.path(),&base).map_err(|e|CoreError::io("Publish original sources",e))?;
            safety::write_json(&marker,&fingerprint)?;
            (self.log)("Base hero model found; original paths preserved");
            Ok(base)
        })
    }
    pub fn support_resources(
        &self,
        project: &Project,
        content: &Path,
        game: &Path,
    ) -> Result<Hashes> {
        self.clock
            .measure("Stage compiler dependencies", None, |_| {
                if self.settings.pak.is_empty() || !Path::new(&self.settings.pak).is_file() {
                    return Ok(Hashes::new());
                }
                let archive = Vpk::open(&self.settings.pak).map_err(vpk_error)?;
                let source = safety::inside(content, project.source_model())?;
                let mut pending = deadlock_model::source_references(&read_text(&source)?)
                    .map_err(|e| invalid(e.to_string()))?
                    .into_iter()
                    .filter(|r| !r.ends_with(".dmx"))
                    .map(|r| format!("{r}_c"))
                    .collect::<Vec<_>>();
                let original = format!("{}_c", project.source_model());
                if archive
                    .entries
                    .contains_key(&deadlock_core::text::casefold(&original))
                {
                    pending.extend(
                        deadlock_vpk::resource_references(
                            &archive.read(&original).map_err(vpk_error)?,
                        )
                        .map_err(vpk_error)?
                        .into_iter()
                        .map(compiled_path),
                    );
                }
                let mut seen = BTreeSet::new();
                let mut hashes = Hashes::new();
                while let Some(reference) = pending.pop() {
                    self.cancel.check()?;
                    let key = deadlock_core::text::casefold(&reference);
                    if key == deadlock_core::text::casefold(&original) || !seen.insert(key.clone())
                    {
                        continue;
                    }
                    if seen.len() > 20000 {
                        return Err(invalid(
                            "Original dependency graph exceeds 20,000 resources.",
                        ));
                    }
                    if !archive.entries.contains_key(&key) {
                        continue;
                    }
                    let raw = archive.read(&reference).map_err(vpk_error)?;
                    let dest = safety::inside(game, &reference)?;
                    safety::require_native_path(&dest)?;
                    safety::atomic_write(&dest, &raw)?;
                    hashes.insert(reference, safety::digest_with_cancel(&dest, self.cancel)?);
                    pending.extend(
                        deadlock_vpk::resource_references(&raw)
                            .map_err(vpk_error)?
                            .into_iter()
                            .map(compiled_path),
                    );
                }
                Ok(hashes)
            })
    }
}
fn compiled_path(path: String) -> String {
    if path.ends_with("_c") {
        path
    } else {
        format!("{path}_c")
    }
}
pub fn read_text(path: &Path) -> Result<String> {
    let value = fs::read_to_string(path)
        .map_err(|e| CoreError::io(format!("Read {}", path.display()), e))?;
    Ok(value.strip_prefix('\u{feff}').unwrap_or(&value).to_owned())
}
pub fn files(root: &Path, cancel: &CancellationToken) -> Result<Vec<PathBuf>> {
    deadlock_vpk::check_unlinked_path(root, false).map_err(vpk_error)?;
    let mut pending = vec![root.to_path_buf()];
    let mut output = Vec::new();
    while let Some(folder) = pending.pop() {
        for entry in fs::read_dir(folder).map_err(|e| CoreError::io("Read source folder", e))? {
            cancel.check()?;
            let entry = entry.map_err(|e| CoreError::io("Read source entry", e))?;
            let path = entry.path();
            deadlock_vpk::check_unlinked_path(&path, false).map_err(vpk_error)?;
            let kind = entry
                .file_type()
                .map_err(|e| CoreError::io("Read source type", e))?;
            if kind.is_dir() {
                pending.push(path);
            } else if kind.is_file() {
                output.push(path);
            } else {
                return Err(invalid("Source tree contains a special file."));
            }
        }
    }
    output.sort();
    Ok(output)
}
pub fn inventory(root: &Path, cancel: &CancellationToken) -> Result<Hashes> {
    files(root, cancel)?
        .into_iter()
        .map(|p| {
            let key = p
                .strip_prefix(root)
                .map_err(|_| invalid("Source escapes root."))?
                .to_str()
                .ok_or_else(|| invalid("Source filename is not Unicode."))?
                .replace('\\', "/");
            Ok((key, safety::digest_with_cancel(&p, cancel)?))
        })
        .collect::<Result<BTreeMap<_, _>>>()
}
