$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot

function Step($msg) { Write-Host "`n==> $msg" -ForegroundColor Yellow }

$cargoBin = Join-Path $env:USERPROFILE ".cargo\bin"
if (Test-Path $cargoBin) { $env:Path = "$cargoBin;$env:Path" }
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Step "Rust fehlt, wird installiert (rustup)"
    winget install --id Rustlang.Rustup -e --silent --accept-package-agreements --accept-source-agreements
    $env:Path = "$cargoBin;$env:Path"
}

$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
$hasMsvc = (Test-Path $vswhere) -and (& $vswhere -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath)
if ($hasMsvc) {
    $toolchain = "stable-x86_64-pc-windows-msvc"
} else {
    $toolchain = "stable-x86_64-pc-windows-gnu"
    $mingw = Get-ChildItem "$env:LOCALAPPDATA\Microsoft\WinGet\Packages" -Directory -Filter "BrechtSanders.WinLibs*" -ErrorAction SilentlyContinue |
        ForEach-Object { Join-Path $_.FullName "mingw64\bin" } | Where-Object { Test-Path (Join-Path $_ "as.exe") } | Select-Object -First 1
    if (-not $mingw -and -not (Get-Command as.exe -ErrorAction SilentlyContinue)) {
        Step "MinGW fehlt, wird installiert (WinLibs)"
        winget install --id BrechtSanders.WinLibs.POSIX.UCRT -e --silent --scope user --accept-package-agreements --accept-source-agreements
        $mingw = Get-ChildItem "$env:LOCALAPPDATA\Microsoft\WinGet\Packages" -Directory -Filter "BrechtSanders.WinLibs*" |
            ForEach-Object { Join-Path $_.FullName "mingw64\bin" } | Select-Object -First 1
    }
    if ($mingw) { $env:Path = "$mingw;$env:Path" }
}
Step "Toolchain: $toolchain"
rustup toolchain install $toolchain --profile minimal | Out-Null

$env:RUSTFLAGS = "--remap-path-prefix=$env:USERPROFILE=~"

Step "Baue Claude Crab"
cargo "+$toolchain" build --release
if ($LASTEXITCODE -ne 0) { throw "App-Build fehlgeschlagen" }

Step "Baue Setup"
Copy-Item Cargo.lock installer\Cargo.lock -Force
$env:CARGO_TARGET_DIR = Join-Path $PSScriptRoot "target"
Push-Location installer
cargo "+$toolchain" build --release
$code = $LASTEXITCODE
Pop-Location
Remove-Item Env:\CARGO_TARGET_DIR
if ($code -ne 0) { throw "Setup-Build fehlgeschlagen" }

New-Item -ItemType Directory -Force dist | Out-Null
Copy-Item target\release\ClaudeCrab-Setup.exe dist\ClaudeCrab-Setup.exe -Force
$size = "{0:N1} MB" -f ((Get-Item dist\ClaudeCrab-Setup.exe).Length / 1MB)
Step "Fertig: dist\ClaudeCrab-Setup.exe ($size)"
