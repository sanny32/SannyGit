param(
    [switch]$Tests,
    [switch]$Install,
    [string]$InstallPrefix = "",
    [switch]$Help
)

$ErrorActionPreference = "Stop"

if ($Help) {
    Write-Host @"
Usage: .\build.ps1 [-Tests] [-Install] [-InstallPrefix <PREFIX>]

Build and package SannyGit in Release mode on Windows.
Missing tools are installed with winget (elevation may be requested).

  -Tests                  Run the Rust test suite before packaging
  -Install                Install after packaging
  -InstallPrefix <PREFIX> Install into PREFIX (implies -Install)
  -Help                   Show this help without installing or building anything

Default install location: %LOCALAPPDATA%\Programs\SannyGit.
Packages are written to Cargo's target directory under package/.
"@
    exit 0
}

function Update-SessionPath {
    $cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $env:USERPROFILE ".cargo" }
    $paths = @(
        $env:PATH,
        (Join-Path $cargoHome "bin"),
        [Environment]::GetEnvironmentVariable("Path", "Machine"),
        [Environment]::GetEnvironmentVariable("Path", "User")
    )
    $env:PATH = $paths -join ";"
}

function Install-Tool {
    param([string]$Id, [string[]]$ExtraArguments = @())
    if (-not (Get-Command winget -ErrorAction SilentlyContinue)) {
        throw "winget is required to install $Id automatically. Install App Installer or install the tool manually, then rerun this script."
    }
    Write-Host "Installing $Id..."
    & winget install --id $Id --exact --source winget --accept-source-agreements --accept-package-agreements @ExtraArguments
    if ($LASTEXITCODE -ne 0) { throw "Installation of $Id failed (exit $LASTEXITCODE)." }
    Update-SessionPath
}

function Find-Python {
    foreach ($name in @("python", "python3", "py")) {
        if (-not (Get-Command $name -ErrorAction SilentlyContinue)) { continue }
        $arguments = if ($name -eq "py") { @("-3") } else { @() }
        & $name @arguments -c "import sys; sys.exit(0 if sys.version_info >= (3, 9) else 1)"
        if ($LASTEXITCODE -eq 0) {
            return @{ Command = $name; Arguments = $arguments }
        }
    }
    return $null
}

function Find-VisualStudio {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
    if (Test-Path $vswhere) {
        $installation = & $vswhere -latest -products * -requires $script:VcComponent -property installationPath
        if ($LASTEXITCODE -eq 0 -and $installation) { return $installation }
    }
    return $null
}

try {
    if ($env:OS -ne "Windows_NT") { throw "Use build.sh on Linux and macOS." }
    if ($PSBoundParameters.ContainsKey("InstallPrefix") -and [string]::IsNullOrWhiteSpace($InstallPrefix)) {
        throw "InstallPrefix must not be empty."
    }
    Update-SessionPath
    $python = Find-Python
    if (-not $python) {
        Install-Tool "Python.Python.3.13"
        $python = Find-Python
        if (-not $python) { throw "Python 3.9+ is not available. Restart the terminal and rerun this script." }
    }
    if (-not (Get-Command git -ErrorAction SilentlyContinue)) { Install-Tool "Git.Git" }
    if (-not (Get-Command git -ErrorAction SilentlyContinue)) {
        throw "Git is not available. Restart the terminal and rerun this script."
    }
    $script:VcComponent = if ($env:PROCESSOR_ARCHITECTURE -eq 'ARM64' -or $env:PROCESSOR_ARCHITEW6432 -eq 'ARM64') {
        "Microsoft.VisualStudio.Component.VC.Tools.ARM64"
    } else { "Microsoft.VisualStudio.Component.VC.Tools.x86.x64" }
    $visualStudio = Find-VisualStudio
    if (-not $visualStudio) {
        Install-Tool -Id "Microsoft.VisualStudio.2022.BuildTools" -ExtraArguments @("--override", "--wait --passive --norestart --add Microsoft.VisualStudio.Workload.VCTools --add $script:VcComponent --includeRecommended")
        $visualStudio = Find-VisualStudio
        if (-not $visualStudio) { throw "Visual Studio C++ tools were not found. Complete installation and rerun this script." }
    }

    if (-not (Get-Command cargo -ErrorAction SilentlyContinue) -or -not (Get-Command rustc -ErrorAction SilentlyContinue)) {
        Install-Tool "Rustlang.Rustup"
    }
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue) -or -not (Get-Command rustc -ErrorAction SilentlyContinue)) {
        throw "Rust is not available. Restart the terminal and rerun this script."
    }
    $minimumLine = Select-String -Path (Join-Path $PSScriptRoot "Cargo.toml") -Pattern '^rust-version = "([^"]+)"'
    if (-not $minimumLine) { throw "Cannot read rust-version from Cargo.toml." }
    $minimumRust = [version]$minimumLine.Matches[0].Groups[1].Value
    $rustVersion = & rustc --version
    if ($LASTEXITCODE -ne 0) { throw "Cannot run rustc." }
    if ($rustVersion -notmatch 'rustc (\d+\.\d+\.\d+)') { throw "Cannot parse rustc version: $rustVersion" }
    if ([version]$Matches[1] -lt $minimumRust) {
        if (-not (Get-Command rustup -ErrorAction SilentlyContinue)) { throw "Rust $minimumRust+ is required. Update Rust through rustup." }
        & rustup update stable --no-self-update
        if ($LASTEXITCODE -ne 0) { throw "Rust update failed." }
        $env:RUSTUP_TOOLCHAIN = "stable"
    }
    $hostInfo = & rustc -vV
    if ($LASTEXITCODE -ne 0) { throw "Cannot detect Rust host." }
    if (-not ($hostInfo -match '^host: .*windows-msvc$')) { throw "A Windows MSVC Rust toolchain is required. Install it through rustup." }

    $architecture = if ($hostInfo -match '^host: aarch64-') { "arm64" } elseif ($hostInfo -match '^host: i686-') { "x86" } else { "x64" }
    $devCmd = Join-Path $visualStudio "Common7\Tools\VsDevCmd.bat"
    if (-not (Test-Path $devCmd)) { throw "VsDevCmd.bat was not found at $devCmd." }
    $environmentLines = & $env:COMSPEC /d /s /c "`"`"$devCmd`" -arch=$architecture -host_arch=$architecture >nul && set`""
    if ($LASTEXITCODE -ne 0) { throw "Cannot initialize the Visual Studio build environment." }
    foreach ($line in $environmentLines) {
        if ($line -match '^([^=]+)=(.*)$') { Set-Item -Path "Env:$($Matches[1])" -Value $Matches[2] }
    }

    $packageArguments = @((Join-Path $PSScriptRoot "scripts\package.py"))
    if ($Tests) { $packageArguments += "--tests" }
    if ($InstallPrefix) { $packageArguments += "--install=$InstallPrefix" } elseif ($Install) { $packageArguments += "--install" }
    $pythonArguments = $python.Arguments
    $pythonExecutable = $python.Command
    & $pythonExecutable @pythonArguments @packageArguments
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}
catch {
    Write-Error $_ -ErrorAction Continue
    exit 1
}
