# E2E test of the Windows installer: installs it silently into a temporary folder, checks what
# was installed, starts the installed app (screenshot mode: demo data, writes nothing), then
# uninstalls it and checks that the program is gone and the person's data is untouched.
#
#   powershell -ExecutionPolicy Bypass -File packaging\windows\test-installer.ps1 [-Setup <path>]
#
# Stops without doing anything if MyDocDesk is already installed on this PC (the test would
# otherwise upgrade and then uninstall the real installation).
param([string]$Setup)

$ErrorActionPreference = 'Stop'
$app = Resolve-Path (Join-Path $PSScriptRoot '..\..')
$appId = '{AA5D7B82-B99D-4CD3-858E-0858E85F77BB}_is1'
$version = (Select-String -Path (Join-Path $app 'Cargo.toml') -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
if (-not $Setup) { $Setup = Join-Path $app "dist\MyDocDesk-Setup-$version.exe" }

$failed = 0
function Check([bool]$ok, [string]$what) {
    if ($ok) { Write-Host "  ok    $what" } else { Write-Host "  FAIL  $what" -ForegroundColor Red; $script:failed++ }
}
function UninstallKey { Get-Item "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\$appId" -ErrorAction SilentlyContinue }

Write-Host "MyDocDesk installer E2E ($version)"
if (-not (Test-Path $Setup)) { Write-Host "  FAIL  setup file not found: $Setup" -ForegroundColor Red; exit 1 }
if (UninstallKey) { Write-Host "  STOP  MyDocDesk is already installed here; uninstall it first (the test must not touch a real installation)." -ForegroundColor Yellow; exit 2 }

$tmp = Join-Path ([IO.Path]::GetTempPath()) ("mdd-installer-e2e-" + [guid]::NewGuid().ToString('N').Substring(0, 8))
New-Item -ItemType Directory $tmp | Out-Null
$dir = Join-Path $tmp 'app'
$exe = Join-Path $dir 'MyDocDesk.exe'
$data = Join-Path $env:APPDATA 'MyDocDesk'
$dataBefore = if (Test-Path $data) { @(Get-ChildItem $data -Recurse -File | ForEach-Object FullName) } else { $null }
$shortcut = Join-Path ([Environment]::GetFolderPath('Programs')) 'MyDocDesk.lnk'

# 1. install
$p = Start-Process $Setup -ArgumentList '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/CURRENTUSER', "/DIR=`"$dir`"", "/LOG=`"$tmp\install.log`"" -Wait -PassThru
Check ($p.ExitCode -eq 0) "installer finished (exit $($p.ExitCode))"
Check (Test-Path $exe) 'MyDocDesk.exe installed'
if (Test-Path $exe) {
    $vi = (Get-Item $exe).VersionInfo
    Check ($vi.ProductName -eq 'MyDocDesk') "exe product name ($($vi.ProductName))"
    Check ($vi.ProductVersion -like "$version*") "exe version $($vi.ProductVersion) = Cargo.toml $version"
    Add-Type -AssemblyName System.Drawing
    $bmp = [System.Drawing.Icon]::ExtractAssociatedIcon($exe).ToBitmap()
    $c = $bmp.GetPixel(4, 16)
    Check ([math]::Abs($c.R - 0x1f) -le 12 -and [math]::Abs($c.G - 0x5b) -le 12 -and [math]::Abs($c.B - 0xd1) -le 12) ("exe carries the app icon (pixel #{0:x2}{1:x2}{2:x2})" -f $c.R, $c.G, $c.B)
}
Check (Test-Path $shortcut) 'Start menu shortcut'
$key = UninstallKey
Check ($null -ne $key) 'listed in Settings > Apps'
if ($key) {
    Check ($key.GetValue('DisplayVersion') -eq $version) "listed version $($key.GetValue('DisplayVersion'))"
    Check ($key.GetValue('DisplayName') -like 'MyDocDesk*') "listed name $($key.GetValue('DisplayName'))"
}

# 2. the installed app starts and draws its screen (screenshot mode never writes user data)
if (Test-Path $exe) {
    $shot = Join-Path $tmp 'home.png'
    $r = Start-Process $exe -ArgumentList '--screenshot', "`"$shot`"", '--page', 'home' -Wait -PassThru
    Check ($r.ExitCode -eq 0 -and (Test-Path $shot) -and (Get-Item $shot).Length -gt 20000) "installed app runs and draws its window ($shot)"
}

# 3. uninstall (the uninstaller re-launches itself from %TEMP%, so wait for the result)
$unins = Join-Path $dir 'unins000.exe'
Check (Test-Path $unins) 'uninstaller present'
if (Test-Path $unins) {
    Start-Process $unins -ArgumentList '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART' -Wait
    $deadline = (Get-Date).AddSeconds(60)
    while (((Test-Path $exe) -or (UninstallKey)) -and (Get-Date) -lt $deadline) { Start-Sleep -Milliseconds 500 }
}
Check (-not (Test-Path $exe)) 'program removed'
Check (-not (Test-Path $shortcut)) 'Start menu shortcut removed'
Check ($null -eq (UninstallKey)) 'no longer listed in Settings > Apps'
if ($null -ne $dataBefore) {
    $after = @(Get-ChildItem $data -Recurse -File -ErrorAction SilentlyContinue | ForEach-Object FullName)
    $lost = @($dataBefore | Where-Object { $after -notcontains $_ })
    Check ($lost.Count -eq 0) "the person's data in $data is untouched ($($dataBefore.Count) files)"
} else {
    Check $true "no data folder existed before (nothing to keep)"
}

Write-Host ''
if ($failed -eq 0) { Write-Host "PASS  installer E2E" -ForegroundColor Green; Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue; exit 0 }
Write-Host "FAIL  $failed check(s) failed; logs kept in $tmp" -ForegroundColor Red
exit 1
