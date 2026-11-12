; DevKit Windows installer — Inno Setup 6 script
; Build:  ISCC.exe /DAPP_VERSION=0.1.0 devkit.iss
; Output: Output\DevKit-Windows-x64-setup.exe
;
; Design notes:
; - Per-user installation ({localappdata}\Programs\DevKit) with
;   PrivilegesRequired=lowest: DevKit never elevates by default
;   (docs/security/security-model.md §7).
; - Includes the LICENSE (source-available draft) in the wizard.
; - The uninstaller removes only files DevKit installed.

#ifndef APP_VERSION
#define APP_VERSION "0.1.0"
#endif

[Setup]
AppId={{5C3A9D2E-1F47-4B6A-9E28-7D41C0AF3B62}}
AppName=DevKit
AppVersion={#APP_VERSION}
AppPublisher=DevKit contributors
AppPublisherURL=https://github.com/example/devkit
DefaultDirName={localappdata}\Programs\DevKit
DefaultGroupName=DevKit
DisableProgramGroupPage=yes
LicenseFile=..\..\LICENSE
OutputDir=Output
OutputBaseFilename=DevKit-Windows-x64-setup
SetupIconFile=..\..\apps\desktop\windows\runner\resources\app_icon.ico
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
ArchitecturesInstallIn64BitMode=x64compatible
ArchitecturesAllowed=x64compatible
PrivilegesRequired=lowest
UninstallDisplayIcon={app}\devkit.exe

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; \
    GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "..\..\apps\desktop\build\windows\x64\runner\Release\*"; \
    DestDir: "{app}"; Flags: recursesubdirs createallsubdirs replacesameversion

[Icons]
Name: "{group}\DevKit"; Filename: "{app}\devkit.exe"
Name: "{autodesktop}\DevKit"; Filename: "{app}\devkit.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\devkit.exe"; Description: "{cm:LaunchProgram,DevKit}"; \
    Flags: nowait postinstall skipifsilent

[UninstallDelete]
; Remove runtime-generated user files left next to the install (logs etc.),
; never user data stored in the OS data root.
Type: filesandordirs; Name: "{app}\*.log"
