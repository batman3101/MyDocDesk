# Builds the Windows installer: tests -> release build -> Inno Setup -> installer E2E.
#
#   powershell -ExecutionPolicy Bypass -File packaging\windows\build-installer.ps1 [-SkipTests] [-SkipE2E]
#
# Result: app\dist\MyDocDesk-Setup-<version>.exe  (version = Cargo.toml [package] version)
param([switch]$SkipTests, [switch]$SkipE2E)

$ErrorActionPreference = 'Stop'
$app = Resolve-Path (Join-Path $PSScriptRoot '..\..')
Set-Location $app

$version = (Select-String -Path 'Cargo.toml' -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
Write-Host "MyDocDesk $version"

$iscc = @(
    "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe",
    "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
    "$env:ProgramFiles\Inno Setup 6\ISCC.exe"
) | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $iscc) { throw 'Inno Setup 6 is not installed: winget install --id JRSoftware.InnoSetup -e --scope user' }

if (-not $SkipTests) {
    Write-Host '== cargo test'
    cargo test
    if ($LASTEXITCODE -ne 0) { throw 'tests failed: no installer built' }
}

Write-Host '== cargo build --release'
cargo build --release
if ($LASTEXITCODE -ne 0) { throw 'release build failed' }

Write-Host '== Inno Setup'
& $iscc /Q "/DAppVersion=$version" 'packaging\windows\MyDocDesk.iss'
if ($LASTEXITCODE -ne 0) { throw 'Inno Setup failed' }
$setup = Join-Path $app "dist\MyDocDesk-Setup-$version.exe"
Write-Host ("   {0}  ({1:N1} MB)" -f $setup, ((Get-Item $setup).Length / 1MB))

if (-not $SkipE2E) {
    Write-Host '== installer E2E'
    & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'test-installer.ps1') -Setup $setup
    if ($LASTEXITCODE -ne 0) { throw 'installer E2E failed' }
}
Write-Host "done: $setup"
