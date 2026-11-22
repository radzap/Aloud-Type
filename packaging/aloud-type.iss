; Aloud Type per-user installer. Build: iscc /DAppVersion=0.1.0 /DStageDir=<dir with release files> aloud-type.iss
; StageDir holds aloud-type.exe, aloud-type-settings-ui.exe, LICENSE, THIRD-PARTY-NOTICES.txt, README.txt (see scripts/release.py).
#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef StageDir
  #define StageDir "..\dist\stage"
#endif

[Setup]
AppId={{6B1F3A52-8D3E-4C6A-9F0B-2A7E5D41C8E9}
AppName=Aloud Type
AppVersion={#AppVersion}
AppPublisher=Aloud Type
AppPublisherURL=https://github.com/radzap/Aloud-Type
DefaultDirName={autopf}\Aloud Type
DefaultGroupName=Aloud Type
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
LicenseFile={#StageDir}\LICENSE
OutputBaseFilename=AloudType-{#AppVersion}-setup
OutputDir={#StageDir}\..
Compression=lzma2
SolidCompression=yes
ArchitecturesInstallIn64BitMode=x64compatible
CloseApplications=yes
UninstallDisplayIcon={app}\aloud-type.exe

[Tasks]
Name: "autostart"; Description: "Launch Aloud Type when I sign in to Windows"; Flags: unchecked

[Files]
; Program files only. Settings and Speech models live in %LOCALAPPDATA%\AloudType and are never installed or replaced.
Source: "{#StageDir}\aloud-type.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#StageDir}\aloud-type-settings-ui.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#StageDir}\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#StageDir}\THIRD-PARTY-NOTICES.txt"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#StageDir}\README.txt"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#StageDir}\*.dll"; DestDir: "{app}"; Flags: ignoreversion skipifsourcedoesntexist

[Icons]
Name: "{autoprograms}\Aloud Type"; Filename: "{app}\aloud-type.exe"

[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "AloudType"; ValueData: """{app}\aloud-type.exe"""; Flags: uninsdeletevalue; Tasks: autostart

[Run]
Filename: "{app}\aloud-type.exe"; Description: "Start Aloud Type now"; Flags: nowait postinstall skipifsilent

[Code]
procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  DataDir: String;
begin
  if CurUninstallStep = usPostUninstall then
  begin
    DataDir := ExpandConstant('{localappdata}\AloudType');
    { Silent uninstalls and "No" keep settings and downloaded models. }
    if (not UninstallSilent) and DirExists(DataDir) then
      if MsgBox('Also delete your Aloud Type settings and downloaded Speech models?' + #13#10 + DataDir,
                mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES then
        DelTree(DataDir, True, True, True);
  end;
end;
