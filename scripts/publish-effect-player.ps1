# Publishes the effect player (tools/effect-player) and stages it where the bundle takes it:
# resources/native/effect-player/win-x64/Dmb.EffectPlayer.exe (git-ignored, a build output).
#
# Needs the .NET SDK that tools/effect-player/global.json names: DMB_DOTNET (the full path of a
# dotnet.exe) first, then `dotnet` on PATH. Without it, a local build goes on without the player
# (the app then shows the colour strips only); with -Required (release builds) that is an error.
# A failed publish, a failed check or a failed smoke run is always an error.
#
#   pwsh -NoProfile -File scripts/publish-effect-player.ps1 [-Required]
param([switch]$Required)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if ($env:OS -ne 'Windows_NT') { throw 'The effect player is built on Windows only.' }

$root = Split-Path -Parent $PSScriptRoot
$project = Join-Path $root 'tools/effect-player'
$stageRoot = Join-Path $root 'resources/native/effect-player'
$stage = Join-Path $stageRoot 'win-x64'
$publish = Join-Path $project 'obj/dmb-publish'
$contentsFile = Join-Path $project 'obj/Release/net11.0/win-x64/dmb-bundle-contents.txt'
$dependencies = Join-Path $root 'licenses/effect-player/DEPENDENCIES.json'
$exeName = 'Dmb.EffectPlayer.exe'
$sdkVersion = (Get-Content -LiteralPath (Join-Path $project 'global.json') -Raw | ConvertFrom-Json).sdk.version

# A staged player is always from this run: a skipped or failed run leaves none behind.
if (Test-Path -LiteralPath $stageRoot) { Remove-Item -LiteralPath $stageRoot -Recurse -Force }

# dotnet picks its SDK by the global.json of the working folder, so every dotnet call runs in the
# project folder, and the SDK it reports there must be the pinned one.
function Test-Sdk([string]$dotnet) {
    Push-Location -LiteralPath $project
    try { $reported = & $dotnet --version 2>$null; $code = $LASTEXITCODE } finally { Pop-Location }
    return $code -eq 0 -and "$reported".Trim() -eq $sdkVersion
}
$dotnet = $null
if ($env:DMB_DOTNET) {
    if (-not (Test-Path -LiteralPath $env:DMB_DOTNET -PathType Leaf)) { throw "DMB_DOTNET does not name a file: $env:DMB_DOTNET" }
    if (-not (Test-Sdk $env:DMB_DOTNET)) { throw "DMB_DOTNET has no .NET SDK $sdkVersion (tools/effect-player/global.json)." }
    $dotnet = (Resolve-Path -LiteralPath $env:DMB_DOTNET).Path
} else {
    $onPath = Get-Command dotnet -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($onPath -and (Test-Sdk $onPath.Source)) { $dotnet = $onPath.Source }
}
if (-not $dotnet) {
    if ($Required) { throw "The effect player needs the .NET SDK $sdkVersion (DMB_DOTNET or dotnet on PATH); release builds always include it." }
    Write-Host "Effect player: not included in this build (no .NET SDK $sdkVersion; set DMB_DOTNET to its dotnet.exe); the app shows the colour strips only."
    exit 0
}

function Assert-WindowsX64([string]$file) {
    $bytes = [System.IO.File]::ReadAllBytes($file)
    if ([System.Text.Encoding]::ASCII.GetString($bytes, 0, 2) -ne 'MZ') { throw "Not a Windows executable: $file" }
    $pe = [BitConverter]::ToInt32($bytes, 0x3c)
    if ([System.Text.Encoding]::ASCII.GetString($bytes, $pe, 4) -ne "PE`0`0" -or [BitConverter]::ToUInt16($bytes, $pe + 4) -ne 0x8664) {
        throw "Not a Windows x64 executable: $file"
    }
}
function Invoke-Player([string[]]$Arguments, [int]$TimeoutSeconds = 120) {
    $info = [System.Diagnostics.ProcessStartInfo]::new((Join-Path $publish $exeName))
    foreach ($a in $Arguments) { $info.ArgumentList.Add($a) }
    $info.RedirectStandardOutput = $true
    $info.RedirectStandardError = $true
    $info.UseShellExecute = $false
    $info.CreateNoWindow = $true
    $process = [System.Diagnostics.Process]::Start($info)
    $stdout = $process.StandardOutput.ReadToEndAsync()
    $stderr = $process.StandardError.ReadToEndAsync()
    if (-not $process.WaitForExit($TimeoutSeconds * 1000)) {
        $process.Kill($true)
        throw "Effect player smoke run timed out: $($Arguments -join ' ')"
    }
    $process.WaitForExit()
    return [pscustomobject]@{ Code = $process.ExitCode; Out = $stdout.Result; Err = $stderr.Result }
}

$saved = @{}
$variables = @{
    DOTNET_CLI_TELEMETRY_OPTOUT = '1'; DOTNET_NOLOGO = '1'; DOTNET_SKIP_FIRST_TIME_EXPERIENCE = '1'
    DOTNET_MULTILEVEL_LOOKUP = '0'; DOTNET_ROOT = (Split-Path -Parent $dotnet)
}
foreach ($name in $variables.Keys) {
    $saved[$name] = [Environment]::GetEnvironmentVariable($name)
    [Environment]::SetEnvironmentVariable($name, $variables[$name])
}
try {
    if (Test-Path -LiteralPath $publish) { Remove-Item -LiteralPath $publish -Recurse -Force }
    if (Test-Path -LiteralPath $contentsFile) { Remove-Item -LiteralPath $contentsFile -Force }
    Write-Host "Effect player: publishing with .NET SDK $sdkVersion"
    # Locked restore: exactly the packages and hashes of packages.lock.json, from nuget.org only
    # (nuget.config). The single-file, trimmed, self-contained settings are in the project.
    Push-Location -LiteralPath $project
    try {
        & $dotnet publish EffectPlayer.csproj -c Release -r win-x64 --nologo -p:RestoreLockedMode=true -o $publish
        $code = $LASTEXITCODE
    } finally { Pop-Location }
    if ($code -ne 0) { throw "Publishing the effect player failed with exit code $code." }

    # The output is the single file and nothing else (no debug symbols beside it).
    $files = @(Get-ChildItem -LiteralPath $publish -Recurse -File)
    $unexpected = @($files | Where-Object { $_.Name -ne $exeName })
    if ($unexpected.Count) { throw "Unexpected files in the effect player output: $(($unexpected | ForEach-Object Name) -join ', ')" }
    $exe = Join-Path $publish $exeName
    if (-not (Test-Path -LiteralPath $exe)) { throw "The effect player publish wrote no $exeName." }
    Assert-WindowsX64 $exe

    # What went into the single file: only the player's own assembly and files from the packages
    # that licenses/effect-player/DEPENDENCIES.json accounts for (so nothing from the game, and
    # notices that match the output).
    if (-not (Test-Path -LiteralPath $contentsFile)) { throw "The publish recorded no bundle contents ($contentsFile)." }
    $own = @('Dmb.EffectPlayer.exe', 'Dmb.EffectPlayer.dll', 'Dmb.EffectPlayer.deps.json', 'Dmb.EffectPlayer.runtimeconfig.json')
    $bundled = @{}
    foreach ($line in Get-Content -LiteralPath $contentsFile) {
        if (-not $line) { continue }
        $path, $id, $version = $line.Split('|')
        if (-not $id) {
            if ($own -notcontains $path) { throw "Bundled file from no package: $path" }
            continue
        }
        $bundled["$id $version"] = $true
    }
    $notices = Get-Content -LiteralPath $dependencies -Raw | ConvertFrom-Json
    $listed = @($notices.packages | ForEach-Object { "$($_.id) $($_.version)" }) + "$($notices.runtime.id) $($notices.runtime.version)"
    $missing = @($bundled.Keys | Where-Object { $listed -notcontains $_ } | Sort-Object)
    $stale = @($listed | Where-Object { -not $bundled.ContainsKey($_) } | Sort-Object)
    if ($missing.Count -or $stale.Count) {
        throw "licenses/effect-player does not match the published effect player (not listed: $($missing -join ', '); not shipped: $($stale -join ', ')). Run node scripts/generate-effect-player-notices.mjs."
    }

    # Smoke runs that need neither a GPU nor the game: the version (loads the renderer library),
    # a usage error, and image statistics of a 2 x 2 PNG (SkiaSharp's native library and the
    # trimmed JSON writer).
    $expected = "effect-player $($notices.version)"
    $run = Invoke-Player @('--version')
    if ($run.Code -ne 0 -or -not $run.Out.StartsWith($expected)) { throw "Effect player --version: exit $($run.Code), output '$($run.Out.Trim())' $($run.Err.Trim())" }
    Write-Host "Effect player smoke: $($run.Out.Trim())"
    $run = Invoke-Player @()
    if ($run.Code -ne 2) { throw "Effect player without arguments: exit $($run.Code), expected 2 (usage). $($run.Err.Trim())" }
    $frames = Join-Path ([System.IO.Path]::GetTempPath()) ("dmb-effect-player-smoke-" + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $frames | Out-Null
    try {
        # Top row red, bottom row black.
        [System.IO.File]::WriteAllBytes((Join-Path $frames 'frame_0000.png'),
            [Convert]::FromBase64String('iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAIAAAD91JpzAAAAD0lEQVR4nGP4z8DwnwEGABb3Af9iwaf5AAAAAElFTkSuQmCC'))
        $run = Invoke-Player @('image-stats', '--out', $frames, '--background', '000000')
        if ($run.Code -ne 0) { throw "Effect player image-stats: exit $($run.Code). $($run.Err.Trim())" }
        $stats = @($run.Out | ConvertFrom-Json)
        if ($stats.Count -ne 1 -or $stats[0].pixels -ne 2 -or $stats[0].brightestRgb -ne 'ff0000') {
            throw "Effect player image-stats gave unexpected results: $($run.Out.Trim())"
        }
    } finally {
        Remove-Item -LiteralPath $frames -Recurse -Force -ErrorAction SilentlyContinue
    }

    New-Item -ItemType Directory -Force -Path $stage | Out-Null
    Copy-Item -LiteralPath $exe -Destination (Join-Path $stage $exeName)
    $size = (Get-Item -LiteralPath (Join-Path $stage $exeName)).Length
    Write-Host ("Effect player staged: resources/native/effect-player/win-x64/{0} ({1:N1} MB)" -f $exeName, ($size / 1MB))
} finally {
    foreach ($name in $saved.Keys) { [Environment]::SetEnvironmentVariable($name, $saved[$name]) }
}
