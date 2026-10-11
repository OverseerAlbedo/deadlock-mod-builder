# CI-only prerequisite setup. Normal source builds require WebView2 to be installed.
# Detection/installation follows Microsoft's WebView2 distribution documentation.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if ($env:OS -ne 'Windows_NT') { throw 'WebView2 prerequisite setup requires Windows.' }
function Get-WebView2Version {
    foreach ($path in @(
        'HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}',
        'HKCU:\Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}'
    )) {
        $version = Get-ItemPropertyValue -LiteralPath $path -Name 'pv' -ErrorAction SilentlyContinue
        if ($version -match '^\d+\.\d+\.\d+\.\d+$' -and [version]$version -gt [version]'0.0.0.0') {
            return "$version"
        }
    }
    return $null
}
$version = Get-WebView2Version
if ($null -eq $version) {
    $bootstrapper = Join-Path ([IO.Path]::GetTempPath()) ('dmb-webview2-' + [guid]::NewGuid() + '.exe')
    try {
        Invoke-WebRequest -Uri 'https://go.microsoft.com/fwlink/p/?LinkId=2124703' -OutFile $bootstrapper -TimeoutSec 120
        $signature = Get-AuthenticodeSignature -LiteralPath $bootstrapper
        if ($signature.Status -ne 'Valid' -or $null -eq $signature.SignerCertificate -or $signature.SignerCertificate.Subject -notmatch 'O=Microsoft Corporation') {
            throw 'WebView2 bootstrapper does not have a valid Microsoft signature.'
        }
        $install = Start-Process -FilePath $bootstrapper -ArgumentList @('/silent','/install') -PassThru
        if (-not $install.WaitForExit(180000)) {
            Stop-Process -Id $install.Id -Force
            throw 'WebView2 installation exceeded three minutes.'
        }
        $version = Get-WebView2Version
        if ($null -eq $version) { throw "WebView2 was not installed (bootstrapper exit $($install.ExitCode))." }
    } finally {
        Remove-Item -LiteralPath $bootstrapper -Force -ErrorAction SilentlyContinue
    }
}
Write-Output "WebView2 Runtime available: $version"
