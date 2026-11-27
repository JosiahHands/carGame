[Setup]
AppId=io.github.JosiahHands.CarGame
AppName=Car Game
AppVersion={#GameVersion}
DefaultDirName={localappdata}\Programs\Car Game
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputBaseFilename=Car-Game-Windows-Setup
SetupIconFile={#GameDir}\icon.ico
UninstallDisplayIcon={app}\icon.ico
CloseApplications=yes
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
DisableProgramGroupPage=yes

[Files]
Source: "{#GameDir}\car-game.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#GameDir}\icon.ico"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#GameDir}\assets\*"; DestDir: "{app}\assets"; Flags: ignoreversion recursesubdirs createallsubdirs

Source: "{#GameDir}\licenses\*"; DestDir: "{app}\licenses"; Flags: ignoreversion recursesubdirs createallsubdirs

[InstallDelete]
; 1.3.0 and 1.3.1 put their Start menu shortcut in a "Car Game" folder. The
; shortcut now goes straight in the Start menu, where 1.2.x put its own.
Type: files; Name: "{autoprograms}\Car Game\Car Game.lnk"
Type: dirifempty; Name: "{autoprograms}\Car Game"

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; Flags: unchecked

[Icons]
; Same name and place as 1.2.x's Start menu shortcut, so an update replaces
; it rather than leaving the old game's shortcut next to the new one.
Name: "{autoprograms}\Car Game"; Filename: "{app}\car-game.exe"; WorkingDir: "{app}"; IconFilename: "{app}\icon.ico"
Name: "{autodesktop}\Car Game"; Filename: "{app}\car-game.exe"; WorkingDir: "{app}"; IconFilename: "{app}\icon.ico"; Tasks: desktopicon
; A desktop shortcut from an earlier version (1.2.x always made one) is
; pointed at this version, even when the installer runs silently.
Name: "{autodesktop}\Car Game"; Filename: "{app}\car-game.exe"; WorkingDir: "{app}"; IconFilename: "{app}\icon.ico"; Tasks: not desktopicon; Check: HadDesktopShortcut

[Run]
Filename: "{app}\car-game.exe"; Description: "Play Car Game"; Flags: nowait postinstall skipifsilent
; The game's own updater (Garage, Updates tab) installs silently with /LAUNCH=1,
; and the game starts again on the new version.
Filename: "{app}\car-game.exe"; Flags: nowait; Check: LaunchAfterUpdate

[Code]
function LaunchAfterUpdate: Boolean;
begin
  Result := ExpandConstant('{param:launch|0}') = '1';
end;

function HadDesktopShortcut: Boolean;
begin
  Result := FileExists(ExpandConstant('{autodesktop}\Car Game.lnk'));
end;
