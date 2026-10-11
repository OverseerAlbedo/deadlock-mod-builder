use crate::toolchain::{self, ModelToolset};
use deadlock_core::{
    process::{CommandSpec, ProcessResult, ToolExecutor},
    safety::{self, digest_with_cancel, relative_asset, require_native_path},
    CancellationToken, CoreError, Result, TimingRecorder,
};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

const MAX_CAPTURE: u64 = 512 * 1024 * 1024;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CompileResult {
    pub process: ProcessResult,
    pub output_sha256: String,
    pub resource_references: Vec<String>,
}
fn failure(message: impl std::fmt::Display) -> CoreError {
    CoreError::Integrity(message.to_string())
}
fn tail(text: &str, count: usize) -> String {
    text.chars()
        .rev()
        .take(count)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}
/// Both streams are scanned for diagnostics. Their raw files remain separate and byte exact.
pub fn diagnostic_text(result: &ProcessResult) -> Result<String> {
    Ok(format!(
        "{}\n{}",
        result.read_stdout_lossy(MAX_CAPTURE)?,
        result.read_stderr_lossy(MAX_CAPTURE)?
    ))
}
// Valve's game discovery does not reliably accept std::fs::canonicalize's
// extended Windows spelling. Convert only ordinary drive/UNC paths at the CLI
// boundary; internal filesystem containment checks keep their resolved paths.
#[cfg(any(windows, test))]
fn windows_tool_path(value: &str) -> Result<PathBuf> {
    let (ordinary, converted) = if let Some(tail) = value.strip_prefix(r"\\?\UNC\") {
        let mut parts = tail.split('\\');
        if parts.next().is_none_or(str::is_empty) || parts.next().is_none_or(str::is_empty) {
            return Err(failure(
                "Native compiler requires a complete UNC share path.",
            ));
        }
        (format!(r"\\{tail}"), true)
    } else if let Some(tail) = value.strip_prefix(r"\\?\") {
        let bytes = tail.as_bytes();
        if bytes.len() < 3
            || !bytes[0].is_ascii_alphabetic()
            || bytes[1] != b':'
            || bytes[2] != b'\\'
        {
            return Err(failure(
                "Native compiler requires an ordinary drive or UNC path.",
            ));
        }
        (tail.to_owned(), true)
    } else if value.starts_with(r"\\.\") {
        return Err(failure("Native compiler does not accept device paths."));
    } else {
        (value.to_owned(), false)
    };
    if converted
        && ordinary.split(['\\', '/']).any(|part| {
            !part.is_empty() && (part.ends_with([' ', '.']) || matches!(part, "." | ".."))
        })
    {
        return Err(failure("Native compiler path cannot be represented without changing a filename. Remove trailing dots or spaces from the selected folder or filename."));
    }
    let result = PathBuf::from(ordinary);
    safety::require_windows_native_path(&result)?;
    Ok(result)
}
fn native_path(path: &Path) -> Result<PathBuf> {
    #[cfg(windows)]
    {
        windows_tool_path(
            path.to_str()
                .ok_or_else(|| failure("Native compiler path must be Unicode."))?,
        )
    }
    #[cfg(not(windows))]
    {
        Ok(path.to_owned())
    }
}
pub fn compile_command(
    compiler: &Path,
    source: &Path,
    target_game: Option<&Path>,
) -> Result<CommandSpec> {
    if !compiler.is_file() {
        return Err(failure("Select resourcecompiler.exe in Setup."));
    }
    let compiler = native_path(compiler)?;
    let source = native_path(source)?;
    require_native_path(&source)?;
    let spec = if let Some(game) = target_game {
        CommandSpec::new(&compiler)
            .arg("-f")
            .arg("-i")
            .arg(&source)
            .arg("-game")
            .arg(native_path(game)?)
            .arg("-nop4")
    } else {
        CommandSpec::new(&compiler)
            .arg("-i")
            .arg(&source)
            .arg("-nop4")
    };
    let mut spec = spec.current_dir(
        compiler
            .parent()
            .ok_or_else(|| failure("Compiler has no parent directory"))?,
    );
    spec.check = false;
    Ok(spec)
}
pub fn compile_resource(
    runner: &dyn ToolExecutor,
    compiler: &Path,
    source: &Path,
    output: &Path,
    target_game: Option<&Path>,
    timing: &TimingRecorder,
    parent: Option<&str>,
    cancel: &CancellationToken,
) -> Result<CompileResult> {
    timing.measure("Compile and verify resource",parent,|span|{
        cancel.check()?;require_native_path(output)?;let command=compile_command(compiler,source,target_game)?;
        match fs::remove_file(output){Ok(())=>{},Err(e)if e.kind()==std::io::ErrorKind::NotFound=>{},Err(e)=>return Err(CoreError::io("Remove stale compiler output",e))}
        let process=timing.measure("Run resource compiler",Some(span.id()),|_|runner.run(&command))?;
        timing.measure("Verify compiler output",Some(span.id()),|_|{
            cancel.check()?;let text=diagnostic_text(&process)?;
            if let Some(problem)=deadlock_model::modeldoc::diagnostic(&text){return Err(failure(format!("{problem}\n{}",tail(&text,2500))));}
            if !process.success{return Err(process.failure(compiler));}
            let errors=Regex::new(r"(?im)(?:\b[1-9]\d*\s+failed\b|^\s*(?:fatal error\b|error:|resource compile error:))").expect("constant regex");
            if errors.is_match(&text){return Err(failure(format!("Resource compiler reported a failure for {}.\n{}",source.display(),tail(&text,2500))));}
            if !output.is_file()||fs::metadata(output).map_err(|e|CoreError::io("Inspect compiled output",e))?.len()<16{return Err(failure(format!("Compiler produced no valid output for {}: {}",source.display(),output.display())));}
            let raw=fs::read(output).map_err(|e|CoreError::io("Read compiled output",e))?;let layout=deadlock_vpk::resource_blocks(&raw).map_err(failure)?;
            if source.extension().is_some_and(|extension|extension=="vmdl")&&!layout.blocks.iter().any(|block|block.name=="DATA"){return Err(failure("Compiler output has no model DATA block."));}
            let refs=deadlock_vpk::resource_references(&raw).map_err(failure)?;
            Ok(CompileResult{process,output_sha256:digest_with_cancel(output,cancel)?,resource_references:refs})
        })
    })
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FormatProbeReport {
    pub schema: u32,
    pub native: bool,
    pub target: Option<u32>,
    pub requested: String,
    pub evidence: Option<String>,
    pub toolset: toolchain::ToolIdentity,
    pub process: Option<ProcessResult>,
    pub error: Option<String>,
    pub scope: String,
}
/// Fresh probe only; expected compiler error identifies its own loader target.
pub fn detect_target(
    toolset: &ModelToolset,
    runner: &dyn ToolExecutor,
    content: &Path,
    game: &Path,
    report_path: &Path,
    requested: &str,
    timing: &TimingRecorder,
    parent: Option<&str>,
    cancel: &CancellationToken,
) -> Result<FormatProbeReport> {
    if !matches!(requested, "auto" | "40" | "41") {
        return Err(failure("Compiler format must be Auto, 40 or 41."));
    }
    timing.measure("Detect ModelDoc compiler format",parent,|span|{
        let mut report=FormatProbeReport{schema:1,native:runner.native_execution(),target:None,requested:requested.into(),evidence:None,toolset:toolchain::identity(toolset,cancel)?,process:None,error:None,scope:"Loader target only. This does not certify feature support, AG2, skinning or game behavior.".into()};
        if requested!="auto"{report.target=requested.parse().ok();report.native=false;report.evidence=Some("explicit_unverified_selection".into());safety::write_json(report_path,&report)?;return Ok(report);}
        let name=format!("dmb_format_probe_{}.vmdl",uuid::Uuid::new_v4().simple());let source=content.join(&name);let output=game.join(format!("{name}_c"));require_native_path(&source)?;require_native_path(&output)?;
        safety::atomic_write(&source,deadlock_model::modeldoc::probe_source())?;
        let outcome:Result<()>=(||{
            let result=timing.measure("Run ModelDoc format probe",Some(span.id()),|_|runner.run(&compile_command(&toolset.compiler,&source,Some(&toolset.game))?))?;
            let text=diagnostic_text(&result)?;report.process=Some(result);
            let target=deadlock_model::modeldoc::interpret_probe(&text).map_err(failure)?;
            report.target=Some(target);report.evidence=Some(if report.native{"native_loader_diagnostic"}else{"non_native_process_loader_diagnostic"}.into());Ok(())
        })();
        if let Err(error)=&outcome{report.error=Some(error.to_string());}
        let saved=safety::write_json(report_path,&report);
        let mut cleanup_error=None;for path in [&source,&output]{if let Err(error)=fs::remove_file(path){if error.kind()!=std::io::ErrorKind::NotFound{cleanup_error=Some(CoreError::io("Clean up format probe",error));}}}
        outcome?;saved?;if let Some(error)=cleanup_error{return Err(error);}Ok(report)
    })
}
pub fn decompile_from_archive(
    cli: &Path,
    archive: &Path,
    resource: &str,
    destination: &Path,
) -> Result<CommandSpec> {
    if !cli.is_file() {
        return Err(failure("Select Source2Viewer CLI in Setup."));
    }
    let resource = relative_asset(resource)?;
    let source = resource.strip_suffix("_c").unwrap_or(&resource);
    require_native_path(&safety::inside(destination, source)?)?;
    let mut directory = destination.as_os_str().to_os_string();
    directory.push(std::path::MAIN_SEPARATOR.to_string());
    Ok(CommandSpec::new(cli)
        .arg("-i")
        .arg(archive)
        .arg("-f")
        .arg(resource)
        .arg("-o")
        .arg(directory)
        .arg("-d"))
}
pub fn decompile_resource(cli: &Path, resource: &Path, destination: &Path) -> Result<CommandSpec> {
    require_native_path(destination)?;
    Ok(CommandSpec::new(cli)
        .arg("-i")
        .arg(resource)
        .arg("-o")
        .arg(destination)
        .arg("-d"))
}
pub fn inspect_block(cli: &Path, resource: &Path, block: &str) -> Result<CommandSpec> {
    if block.len() != 4
        || !block
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
    {
        return Err(failure("Invalid resource block name"));
    }
    Ok(CommandSpec::new(cli)
        .arg("-i")
        .arg(resource)
        .arg("-b")
        .arg(block))
}

#[cfg(test)]
mod path_tests {
    use super::*;

    #[test]
    fn windows_cli_paths_keep_spaces_unicode_and_unc_identity() {
        for (input, expected) in [
            (
                r"\\?\C:\Program Installers\模型\rg_cloth.vmat",
                r"C:\Program Installers\模型\rg_cloth.vmat",
            ),
            (
                r"\\?\UNC\server\share\My Skin\cloth.vmat",
                r"\\server\share\My Skin\cloth.vmat",
            ),
            (
                r"C:\Program Installers\resourcecompiler.exe",
                r"C:\Program Installers\resourcecompiler.exe",
            ),
        ] {
            assert_eq!(windows_tool_path(input).unwrap(), PathBuf::from(expected));
        }
        for input in [
            r"\\?\C:",
            r"\\?\Volume{test}\file",
            r"\\.\pipe\tool",
            r"\\?\C:\folder.\file",
            r"\\?\C:\folder \file",
            r"\\?\C:\folder\..\file",
            r"\\?\UNC\server\",
        ] {
            assert!(windows_tool_path(input).is_err(), "{input}");
        }
        assert!(windows_tool_path(&format!(r"\\?\C:\{}", "a".repeat(260))).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn command_from_canonical_paths_uses_same_files_without_extended_prefix() {
        let temp = tempfile::tempdir().unwrap();
        let compiler = temp.path().join("compiler with spaces.exe");
        let source = temp.path().join("source with spaces.vmat");
        fs::write(&compiler, b"compiler identity fixture").unwrap();
        fs::write(&source, b"material path fixture").unwrap();
        let spec = compile_command(
            &compiler.canonicalize().unwrap(),
            &source.canonicalize().unwrap(),
            Some(&temp.path().canonicalize().unwrap()),
        )
        .unwrap();
        assert!(!spec.program.to_string_lossy().starts_with(r"\\?\"));
        assert_eq!(
            spec.program.canonicalize().unwrap(),
            compiler.canonicalize().unwrap()
        );
        for flag in ["-i", "-game"] {
            let args = spec.args.windows(2).find(|args| args[0] == flag).unwrap();
            assert!(!args[1].to_string_lossy().starts_with(r"\\?\"));
        }
        assert_eq!(
            Path::new(&spec.args[2]).canonicalize().unwrap(),
            source.canonicalize().unwrap()
        );
    }
}
