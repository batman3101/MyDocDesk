; MyDocDesk Windows installer (Inno Setup 6).
; Build it with packaging\windows\build-installer.ps1 - it passes the version from Cargo.toml.
; Rules kept here are checked by packaging_tests in src/main.rs; the installed result by test-installer.ps1.

#ifndef AppVersion
  #error AppVersion is not set. Run packaging\windows\build-installer.ps1 (it passes /DAppVersion from Cargo.toml).
#endif

#define AppName "MyDocDesk"
#define AppExe "MyDocDesk.exe"

[Setup]
; Windows recognises updates of the same app by this id. Never change it.
AppId={{AA5D7B82-B99D-4CD3-858E-0858E85F77BB}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher={#AppName}
VersionInfoVersion={#AppVersion}
VersionInfoProductName={#AppName}
; for this user only, no administrator rights: %LOCALAPPDATA%\Programs\MyDocDesk
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
DefaultDirName={autopf}\{#AppName}
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
DisableWelcomePage=no
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
SetupIconFile=..\..\assets\icon.ico
UninstallDisplayIcon={app}\{#AppExe}
UninstallDisplayName={#AppName}
OutputDir=..\..\dist
OutputBaseFilename=MyDocDesk-Setup-{#AppVersion}
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
; an open MyDocDesk is closed before files are replaced
CloseApplications=yes
RestartApplications=no
; The person's documents, settings and lock number live in %APPDATA%\MyDocDesk and are never
; removed by the uninstaller (no [UninstallDelete] section on purpose).

[Languages]
Name: "korean"; MessagesFile: "compiler:Languages\Korean.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "..\..\target\release\mydocdesk.exe"; DestDir: "{app}"; DestName: "{#AppExe}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExe}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#AppExe}"; Description: "{cm:LaunchProgram,{#AppName}}"; Flags: nowait postinstall skipifsilent
