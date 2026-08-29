$ErrorActionPreference = "Stop"

$projectRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$projectCargoCache = Join-Path $projectRoot ".cargo-home"
$rustSysroot = (& rustc --print sysroot).Trim()
$rustLinker = Join-Path $rustSysroot "lib\rustlib\x86_64-pc-windows-msvc\bin\rust-lld.exe"

if (-not (Test-Path -LiteralPath $rustLinker)) {
    throw "Rust linker not found: $rustLinker"
}

$env:CARGO_HOME = $projectCargoCache
$env:RUSTFLAGS = "-C linker=$rustLinker"

& cargo build --release --locked
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}

$distDir = Join-Path $projectRoot "dist"
New-Item -ItemType Directory -Force -Path $distDir | Out-Null
Copy-Item -Force -LiteralPath (Join-Path $projectRoot "target\release\quietmd.exe") -Destination (Join-Path $distDir "QuietMD.exe")

Write-Host "Built: $distDir\QuietMD.exe"
