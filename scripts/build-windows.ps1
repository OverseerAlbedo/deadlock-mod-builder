param([switch]$Release, [switch]$SkipStartupSmoke)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if ($env:OS -ne 'Windows_NT') { throw 'This script requires Windows. Use the documented Linux checks for development.' }
$root = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $root
$out = Join-Path $root 'artifacts'
$reports = Join-Path $out 'validation'
New-Item -ItemType Directory -Force -Path $reports | Out-Null
$env:CARGO_TARGET_DIR = Join-Path $root 'src-tauri/target'
$target = 'x86_64-pc-windows-msvc'
# Keep the builder's user folder and checkout path out of the program: panic
# locations of dependency sources would otherwise name C:\Users\<name>\.cargo\…
# (encoded flags, so paths with spaces survive).
$remap = @("--remap-path-prefix=$env:USERPROFILE=~", "--remap-path-prefix=$root=.")
if ($env:CARGO_HOME) { $remap += "--remap-path-prefix=$env:CARGO_HOME=~/.cargo" }
$env:CARGO_ENCODED_RUSTFLAGS = ($remap -join [char]0x1f)
function Invoke-Checked {
    param([string]$Name, [string]$Program, [string[]]$Arguments)
    Write-Host "::group::$Name"
    & $Program @Arguments 2>&1 | Tee-Object -FilePath (Join-Path $reports "$Name.log")
    $code = $LASTEXITCODE
    Write-Host '::endgroup::'
    if ($code -ne 0) { throw "$Name failed with exit code $code. See artifacts/validation/$Name.log." }
}
Invoke-Checked 'private-assets' 'node' @('scripts/sync-private-assets.mjs')
Invoke-Checked 'repository-checks' 'node' @('scripts/check-repository.mjs')
Invoke-Checked 'npm-install' 'npm.cmd' @('ci', '--no-audit', '--no-fund')
Invoke-Checked 'frontend-tests' 'npm.cmd' @('test', '--', '--maxWorkers=2')
Invoke-Checked 'rust-tests' 'cargo' @('test', '--manifest-path', 'src-tauri/Cargo.toml', '--workspace', '--locked', '--target', $target)
$buildArguments = @('run', 'tauri', '--', 'build', '--ci', '--target', $target)
if ($Release) { $buildArguments += @('--bundles', 'nsis') } else { $buildArguments += '--no-bundle' }
$buildArguments += @('--', '--locked')
Invoke-Checked 'windows-build' 'npm.cmd' $buildArguments
$build = Join-Path $env:CARGO_TARGET_DIR "$target/release"
$version = (Get-Content package.json -Raw | ConvertFrom-Json).version
$installer = '--no-installer'
if ($Release) {
    $installers = @(Get-ChildItem -LiteralPath (Join-Path $build 'bundle/nsis') -Filter "*_$($version)_x64-setup.exe")
    if ($installers.Count -ne 1) { throw "Expected exactly one $version x64 NSIS installer; found $($installers.Count)." }
    $installer = $installers[0].FullName
}
# Delete only this script's generated distribution, never source or user work.
foreach ($folder in @('portable','release')) {
    $generated = Join-Path $out $folder
    if (Test-Path -LiteralPath $generated) { Remove-Item -LiteralPath $generated -Recurse -Force }
}
Invoke-Checked 'package-checks' 'node' @('scripts/package-windows.mjs', (Join-Path $build 'deadlock-mod-builder.exe'), $installer, $out)
$portable = Join-Path $out 'portable/DeadlockModBuilder'
if (-not $SkipStartupSmoke) {
    & "$PSScriptRoot/smoke-windows.ps1" -Portable $portable -Report (Join-Path $reports 'startup.json')
} else {
    Write-Warning 'Startup smoke was explicitly skipped; this build is not startup-verified.'
}
$zip = Join-Path $out "release/DeadlockModBuilder-v$version-windows-x64-portable.zip"
Compress-Archive -LiteralPath $portable -DestinationPath $zip -CompressionLevel Optimal
$releaseFolder = Join-Path $out 'release'
Get-ChildItem -LiteralPath $releaseFolder -File | Where-Object Name -ne 'SHA256SUMS.txt' | Sort-Object Name | ForEach-Object {
    '{0}  {1}' -f (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant(), $_.Name
} | Set-Content -LiteralPath (Join-Path $releaseFolder 'SHA256SUMS.txt') -Encoding ascii
Write-Host "Windows build complete: $releaseFolder"
