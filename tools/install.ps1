# Download and install the latest Car Game release, or update an existing
# installation. Runs in Windows PowerShell 5.1 and PowerShell 7:
#   irm https://raw.githubusercontent.com/JosiahHands/carGame/main/tools/install.ps1 | iex
# Everything is inside one script block, so a partial download runs nothing
# and the caller's session keeps its own variables and preferences.
& {
    $ErrorActionPreference = 'Stop'
    # Windows PowerShell's progress bar slows downloads to a crawl.
    $ProgressPreference = 'SilentlyContinue'
    $repo = 'https://github.com/JosiahHands/carGame'

    # PowerShell 7 on macOS or Linux runs the same installer as Bash.
    if ($PSVersionTable.PSEdition -eq 'Core' -and -not $IsWindows) {
        $script = Join-Path ([IO.Path]::GetTempPath()) "car-game-install-$PID.sh"
        try {
            Invoke-WebRequest 'https://raw.githubusercontent.com/JosiahHands/carGame/main/tools/install.sh' -OutFile $script
            & bash $script
            if ($LASTEXITCODE -ne 0) { throw "Car Game: the installer failed." }
        } finally {
            Remove-Item -Force -ErrorAction SilentlyContinue $script
        }
        return
    }

    if (-not [Environment]::Is64BitOperatingSystem) { throw 'Car Game: the Windows download needs 64-bit Windows.' }
    # Windows PowerShell 5.1 on older .NET only offers TLS 1.0, which GitHub refuses.
    [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

    $setup = Join-Path ([IO.Path]::GetTempPath()) "Car-Game-Windows-Setup-$PID.exe"
    try {
        Write-Host 'Downloading Car Game...'
        Invoke-WebRequest "$repo/releases/latest/download/Car-Game-Windows-Setup.exe" -OutFile $setup -UseBasicParsing
        Write-Host 'Installing...'
        # A running game is closed so its files can be replaced.
        $run = Start-Process $setup -ArgumentList '/SP-', '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/CLOSEAPPLICATIONS' -Wait -PassThru
        if ($run.ExitCode -ne 0) { throw "Car Game: setup failed with exit code $($run.ExitCode)." }
    } finally {
        Remove-Item -Force -ErrorAction SilentlyContinue $setup
    }

    $key = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\io.github.JosiahHands.CarGame_is1'
    $installed = Get-ItemProperty $key -ErrorAction SilentlyContinue
    if ($installed) {
        Write-Host "Car Game $($installed.DisplayVersion) is installed. Open it from the Start menu."
    } else {
        Write-Host 'Car Game is installed. Open it from the Start menu.'
    }
}
