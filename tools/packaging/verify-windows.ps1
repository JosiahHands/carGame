# Check the Windows installer built by package.sh: what it installs, how it
# replaces older versions' shortcuts, and the in-game updater's arguments.
# Run from the top of the repository after packaging into upload/.
# Pull requests run it with a stand-in game; releases with the real one.
param([string]$Downloads = 'upload')
$ErrorActionPreference = 'Stop'

$setup = Join-Path $Downloads 'Car-Game-Windows-Setup.exe'
$install = Join-Path $env:RUNNER_TEMP 'Car Game update test'
$log = Join-Path $env:RUNNER_TEMP 'Car Game setup.log'

function Install-Game([string[]]$Extra) {
  $arguments = @('/SP-', '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', "`"/LOG=$log`"") + $Extra
  $installer = Start-Process -Wait -PassThru -FilePath $setup -ArgumentList $arguments
  if ($installer.ExitCode -ne 0) {
    if (Test-Path $log) { Get-Content $log | Write-Host }
    throw "Installer returned $($installer.ExitCode)"
  }
}

# An update over older versions: 1.2.x's shortcuts (Start menu and
# Desktop) and 1.3.0/1.3.1's Start menu folder must be replaced, not duplicated.
$shell = New-Object -ComObject WScript.Shell
$programs = [Environment]::GetFolderPath('Programs')
$desktop = [Environment]::GetFolderPath('Desktop')
New-Item -ItemType Directory -Force (Join-Path $programs 'Car Game') | Out-Null
foreach ($old in @((Join-Path $programs 'Car Game.lnk'), (Join-Path $desktop 'Car Game.lnk'), (Join-Path $programs 'Car Game\Car Game.lnk'))) {
  $link = $shell.CreateShortcut($old)
  $link.TargetPath = "$env:WINDIR\notepad.exe"
  $link.Save()
}

# Match the extended drive path passed by the v1.4.0 in-game updater.
Install-Game @("`"/DIR=\\?\$install`"")

$portableFolder = Join-Path $env:RUNNER_TEMP 'car-game-portable'
Remove-Item -Recurse -Force $portableFolder -ErrorAction SilentlyContinue
Expand-Archive (Join-Path $Downloads 'Car-Game-Windows.zip') -DestinationPath $portableFolder
$exe = Join-Path $install 'car-game.exe'
$portable = Join-Path $portableFolder 'Car Game/car-game.exe'
if ((Get-FileHash $exe).Hash -ne (Get-FileHash $portable).Hash) { throw 'Installed and portable executables differ' }
if (!(Test-Path "$install/assets/packs/photo_real/libs/car-models/cars.glb")) { throw 'Installer omitted Photo Real assets' }
if (Test-Path (Join-Path $programs 'Car Game')) { throw 'The 1.3.0/1.3.1 Start menu folder is still there' }
foreach ($path in @((Join-Path $programs 'Car Game.lnk'), (Join-Path $desktop 'Car Game.lnk'))) {
  $link = $shell.CreateShortcut($path)
  if ($link.TargetPath -ne (Resolve-Path $exe).Path) { throw "$path points at $($link.TargetPath)" }
  if ($link.IconLocation -notlike '*icon.ico*') { throw "$path has no icon ($($link.IconLocation))" }
}
$found = @(Get-ChildItem $programs -Recurse -Filter 'Car Game*.lnk')
if ($found.Count -ne 1) { throw "Expected one Start menu shortcut, found: $($found.FullName -join ', ')" }

# The in-game updater starts Setup with the game's process id, and Setup
# waits for that process to exit before replacing any files.
$game = Start-Process -PassThru -WindowStyle Hidden -FilePath 'powershell' -ArgumentList @('-NoProfile', '-Command', 'Start-Sleep -Seconds 6')
Install-Game @('/NORESTARTAPPLICATIONS', "/CAR_GAME_PID=$($game.Id)", "`"/DIR=$install`"")
if (!$game.HasExited) { throw 'Setup finished before the running game closed' }
if (!(Select-String -Quiet -Path $log -Pattern 'Waiting for Car Game to close')) { throw 'Setup did not wait for the game' }

# A process id that has already exited must not block the update.
Install-Game @("/CAR_GAME_PID=$($game.Id)", "`"/DIR=$install`"")
Write-Host 'Windows installer checks passed.'
