[Setup]
AppId=io.github.JosiahHands.CarGame
AppName=Car Game
AppVersion={#GameVersion}
DefaultDirName={localappdata}\Programs\Car Game
DefaultGroupName=Car Game
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputBaseFilename=Car-Game-Windows-Setup
SetupIconFile={#GameDir}\icon.ico
UninstallDisplayIcon={app}\car-game.exe
CloseApplications=yes
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
DisableProgramGroupPage=yes

[Files]
Source: "{#GameDir}\car-game.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#GameDir}\assets\*"; DestDir: "{app}\assets"; Flags: ignoreversion recursesubdirs createallsubdirs

Source: "{#GameDir}\licenses\*"; DestDir: "{app}\licenses"; Flags: ignoreversion recursesubdirs createallsubdirs

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; Flags: unchecked

[Icons]
Name: "{group}\Car Game"; Filename: "{app}\car-game.exe"
Name: "{autodesktop}\Car Game"; Filename: "{app}\car-game.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\car-game.exe"; Description: "Play Car Game"; Flags: nowait postinstall skipifsilent
