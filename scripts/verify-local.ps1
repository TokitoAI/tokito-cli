#Requires -Version 7.0
[CmdletBinding()]
param([string]$CargoPath = 'cargo', [string]$PythonPath = 'python')

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$projectRoot = Split-Path -Parent $PSScriptRoot
$cargoCommand = Get-Command $CargoPath -ErrorAction SilentlyContinue
if (-not $cargoCommand -and $CargoPath -eq 'cargo' -and $env:USERPROFILE) {
    $cargoCommand = Get-Command (Join-Path $env:USERPROFILE '.cargo/bin/cargo.exe') -ErrorAction SilentlyContinue
}
if (-not $cargoCommand) { throw 'Cargo unavailable; install Rust or supply -CargoPath.' }
$pythonCommand = Get-Command $PythonPath -ErrorAction Stop
$rustcName = if ($IsWindows) { 'rustc.exe' } else { 'rustc' }
$rustcPath = if ($env:RUSTC) { $env:RUSTC } else { Join-Path (Split-Path -Parent $cargoCommand.Source) $rustcName }
$rustcCommand = Get-Command $rustcPath -ErrorAction Stop
$previousTarget = $env:CARGO_TARGET_DIR
$previousRustc = $env:RUSTC

function Assert-LocalOutputPath([string]$Path) {
    $fullPath = [System.IO.Path]::GetFullPath($Path)
    $rootPath = [System.IO.Path]::GetFullPath($projectRoot)
    $comparison = if ($IsWindows) { [StringComparison]::OrdinalIgnoreCase } else { [StringComparison]::Ordinal }
    if (-not $fullPath.StartsWith($rootPath + [System.IO.Path]::DirectorySeparatorChar, $comparison)) {
        throw 'Output path is outside the CLI folder.'
    }
    $currentPath = $fullPath
    while ($currentPath) {
        $item = Get-Item -LiteralPath $currentPath -Force -ErrorAction SilentlyContinue
        if ($item -and ($item.Attributes -band [System.IO.FileAttributes]::ReparsePoint)) {
            throw "Linked output path is not allowed: $currentPath"
        }
        if ([string]::Equals($currentPath, $rootPath, $comparison)) { break }
        $currentPath = Split-Path -Parent $currentPath
    }
}

function Get-InputFingerprints {
    $paths = @('Cargo.toml', 'Cargo.lock', 'scripts/verify-local.ps1', 'README.md')
    foreach ($optionalRootFile in @('AGENTS.md')) {
        if (Test-Path -LiteralPath (Join-Path $projectRoot $optionalRootFile)) { $paths += $optionalRootFile }
    }
    foreach ($directory in @('src', 'tests', 'examples', 'docs', '.github')) {
        $inputDirectory = Join-Path $projectRoot $directory
        if (Test-Path -LiteralPath $inputDirectory) {
            $paths += Get-ChildItem -LiteralPath $inputDirectory -Recurse -File |
                ForEach-Object { [System.IO.Path]::GetRelativePath($projectRoot, $_.FullName) }
        }
    }
    foreach ($optional in @('build.rs', 'rust-toolchain', 'rust-toolchain.toml', '.cargo/config', '.cargo/config.toml')) {
        if (Test-Path -LiteralPath (Join-Path $projectRoot $optional)) { $paths += $optional }
    }
    $fingerprints = [ordered]@{}
    foreach ($path in ($paths | Sort-Object)) {
        $fingerprints[$path.Replace('\', '/')] = (Get-FileHash -LiteralPath (Join-Path $projectRoot $path) -Algorithm SHA256).Hash.ToLowerInvariant()
    }
    return $fingerprints
}

Push-Location -LiteralPath $projectRoot
try {
    $env:RUSTC = $rustcCommand.Source
    $env:CARGO_TARGET_DIR = Join-Path $projectRoot 'target/local-verification'
    Assert-LocalOutputPath $env:CARGO_TARGET_DIR
    if (Test-Path -LiteralPath $env:CARGO_TARGET_DIR) {
        foreach ($item in (Get-ChildItem -LiteralPath $env:CARGO_TARGET_DIR -Recurse -Force)) {
            if ($item.Attributes -band [System.IO.FileAttributes]::ReparsePoint) {
                throw "Linked build output is not allowed: $($item.FullName)"
            }
        }
    }
    $binaryName = if ($IsWindows) { 'tokito-cli.exe' } else { 'tokito-cli' }
    $binaryPath = Join-Path $env:CARGO_TARGET_DIR "release/$binaryName"
    $inputs = Get-InputFingerprints
    $checks = @(
        @{ name = 'format'; command = $cargoCommand.Source; arguments = @('fmt', '--all', '--check') },
        @{ name = 'tests'; command = $cargoCommand.Source; arguments = @('test', '--offline', '--locked') },
        @{ name = 'clippy'; command = $cargoCommand.Source; arguments = @('clippy', '--offline', '--locked', '--all-targets', '--', '-D', 'warnings') },
        @{ name = 'release'; command = $cargoCommand.Source; arguments = @('build', '--release', '--offline', '--locked') },
        @{ name = 'e2e'; command = $pythonCommand.Source; arguments = @('tests/e2e_cli.py', '--binary', $binaryPath) },
        @{ name = 'geometry-e2e'; command = $pythonCommand.Source; arguments = @('tests/geometry_e2e.py', '--binary', $binaryPath) },
        @{ name = 'compat-v0'; command = $pythonCommand.Source; arguments = @('tests/compat_v0.py', '--binary', $binaryPath) }
    )
    $results = @()
    $testedBinaryHash = $null
    foreach ($check in $checks) {
        Write-Host "Running $($check.name)..."
        $timer = [System.Diagnostics.Stopwatch]::StartNew()
        $command = $check.command
        $commandArguments = $check.arguments
        if ($check.name -eq 'e2e') {
            Assert-LocalOutputPath $binaryPath
            $testedBinaryHash = (Get-FileHash -LiteralPath $binaryPath -Algorithm SHA256).Hash.ToLowerInvariant()
        }
        & $command @commandArguments
        if ($LASTEXITCODE -ne 0) { throw "$($check.name) failed with exit code $LASTEXITCODE; no delivery receipt created." }
        if ($testedBinaryHash -and (Get-FileHash -LiteralPath $binaryPath -Algorithm SHA256).Hash.ToLowerInvariant() -ne $testedBinaryHash) {
            throw 'Binary changed during E2E; rerun before delivery.'
        }
        $timer.Stop()
        $results += @{ name = $check.name; passed = $true; seconds = $timer.Elapsed.TotalSeconds }
    }
    if (($inputs | ConvertTo-Json -Compress) -ne ((Get-InputFingerprints) | ConvertTo-Json -Compress)) {
        throw 'Source/test inputs changed during verification; rerun before delivery.'
    }
    $cargoVersion = & $cargoCommand.Source --version
    if ($LASTEXITCODE -ne 0) { throw 'Cannot record Cargo version.' }
    $pythonVersion = & $pythonCommand.Source --version
    if ($LASTEXITCODE -ne 0) { throw 'Cannot record Python version.' }
    $rustcVersion = & $rustcCommand.Source --version --verbose
    if ($LASTEXITCODE -ne 0) { throw 'Cannot record compiler version.' }
    $deliveryName = [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssZ') + '-' + [Guid]::NewGuid().ToString('N')
    $deliveryPath = Join-Path $projectRoot "target/delivery/local/$deliveryName"
    Assert-LocalOutputPath $deliveryPath
    Assert-LocalOutputPath $binaryPath
    if ((Get-FileHash -LiteralPath $binaryPath -Algorithm SHA256).Hash.ToLowerInvariant() -ne $testedBinaryHash) {
        throw 'Tested binary changed before delivery.'
    }
    New-Item -ItemType Directory -Path $deliveryPath -ErrorAction Stop | Out-Null
    $deliveredBinary = Join-Path $deliveryPath $binaryName
    Copy-Item -LiteralPath $binaryPath -Destination $deliveredBinary -ErrorAction Stop
    if ((Get-FileHash -LiteralPath $deliveredBinary -Algorithm SHA256).Hash.ToLowerInvariant() -ne $testedBinaryHash) {
        throw 'Delivery copy differs from the binary tested by E2E; no receipt created.'
    }
    $receipt = [ordered]@{
        schemaVersion = '1.0.0'
        verifiedAtUtc = [DateTime]::UtcNow.ToString('o')
        platform = [System.Runtime.InteropServices.RuntimeInformation]::OSDescription
        architecture = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
        cargoVersion = "$cargoVersion"
        rustcVersion = $rustcVersion -join "`n"
        pythonVersion = "$pythonVersion"
        checks = $results
        inputsSha256 = $inputs
        binary = @{ file = $binaryName; sha256 = $testedBinaryHash; bytes = (Get-Item -LiteralPath $deliveredBinary).Length }
        limitations = @('Local platform/toolchain only.', 'No KiCad visual oracle, phone/AR E2E, GLB/STEP or desktop integration acceptance.')
    }
    $receipt | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $deliveryPath 'verification.json') -Encoding utf8
    Write-Host "PASS: verified local binary and receipt: $deliveryPath"
}
finally {
    $env:RUSTC = $previousRustc
    $env:CARGO_TARGET_DIR = $previousTarget
    Pop-Location
}
