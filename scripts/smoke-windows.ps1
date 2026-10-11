param([Parameter(Mandatory)][string]$Portable, [Parameter(Mandatory)][string]$Report)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$Portable = (Resolve-Path -LiteralPath $Portable).Path
$exe = Join-Path $Portable 'deadlock-mod-builder.exe'
$reader = Join-Path $Portable 'native/cloth-reader/win-x64/Dmb.ClothReader.exe'
$readerVersion = & $reader --version
if ($LASTEXITCODE -ne 0) { throw "Bundled cloth reader failed to start (exit $LASTEXITCODE)." }
if ($readerVersion -notmatch '1\.0\.1') { throw "Unexpected cloth reader version: $readerVersion" }
$process = $null
try {
    # Launch the copied package, not the executable inside Cargo's build tree.
    $process = Start-Process -FilePath $exe -WorkingDirectory $Portable -PassThru
    $deadline = [DateTime]::UtcNow.AddSeconds(30)
    do {
        Start-Sleep -Milliseconds 250
        $process.Refresh()
        if ($process.HasExited) { throw "Portable application exited during startup: $($process.ExitCode)" }
    } while ($process.MainWindowHandle -eq 0 -and [DateTime]::UtcNow -lt $deadline)
    if ($process.MainWindowHandle -eq 0) { throw 'Portable application did not create its main window within 30 seconds.' }
    Start-Sleep -Seconds 3
    $process.Refresh()
    if ($process.HasExited) { throw 'Portable application exited after creating its window.' }
    if ($process.MainWindowTitle -ne 'Deadlock Mod Builder') { throw "Unexpected window title: $($process.MainWindowTitle)" }
    @{
        portable_startup_verified = $true
        window_title = $process.MainWindowTitle
        bundled_reader_started = $true
        reader_version = "$readerVersion"
        native_compiler_or_gameplay_tested = $false
    } | ConvertTo-Json | Set-Content -LiteralPath $Report -Encoding utf8
} finally {
    if ($null -ne $process -and -not $process.HasExited) {
        Stop-Process -Id $process.Id -Force
        $process.WaitForExit()
    }
}
