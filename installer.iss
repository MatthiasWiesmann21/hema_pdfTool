; Inno Setup script for hema pdfTool.
; Build the release binary first (build.cmd build --release), then compile:
;   "%LOCALAPPDATA%\Programs\Inno Setup 6\ISCC.exe" installer.iss
; Output: installer\hema_pdfTool-Setup-<version>.exe

#define AppName "hema pdfTool"
#define AppExeName "hema_pdf_tool.exe"
#define AppVersion "0.3.0"
#define AppId "{{551BC3FC-D69E-4602-9162-DBA9BD45C5F8}"

[Setup]
AppId={#AppId}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
DefaultDirName={autopf}\hema pdfTool
DefaultGroupName={#AppName}
; Per-user install into %LOCALAPPDATA%\Programs -> no admin/UAC needed.
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=installer
OutputBaseFilename=hema_pdfTool-Setup-{#AppVersion}
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
UninstallDisplayIcon={app}\{#AppExeName}
; German is the default: no auto-detection, the first [Languages] entry is
; preselected in the language dialog.
ShowLanguageDialog=yes
LanguageDetectionMethod=none

[Languages]
Name: "german";  MessagesFile: "compiler:Languages\German.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "french";  MessagesFile: "compiler:Languages\French.isl"
Name: "spanish"; MessagesFile: "compiler:Languages\Spanish.isl"
Name: "italian"; MessagesFile: "compiler:Languages\Italian.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"

[Files]
Source: "target\release\hema_pdf_tool.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "target\release\pdfium.dll";        DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\{#AppName}";                          Filename: "{app}\{#AppExeName}"
Name: "{group}\{cm:UninstallProgram,{#AppName}}";    Filename: "{uninstallexe}"
Name: "{autodesktop}\{#AppName}";                    Filename: "{app}\{#AppExeName}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#AppExeName}"; Description: "{cm:LaunchProgram,{#AppName}}"; Flags: nowait postinstall skipifsilent
