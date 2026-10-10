#Requires -Version 7.0
<#
.SYNOPSIS
Runs a reproducible acceptance journey against a built tokito-cli executable.

.DESCRIPTION
This is intentionally separate from unit tests and fixture-specific Python
tests. It invokes the release executable through real subprocesses, preserves
the generated artifacts and writes a local machine-readable receipt. It does
not open a CAD viewer, drive a mobile device, or establish KiCad parity.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$BinaryPath,
    [string]$OutputRoot
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$projectRoot = Split-Path -Parent $PSScriptRoot
$binary = [System.IO.Path]::GetFullPath($BinaryPath)
if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) {
    throw "CLI binary does not exist: $binary"
}

if (-not $OutputRoot) {
    $OutputRoot = Join-Path $projectRoot 'target/real-cli-e2e'
}
$outputRootFull = [System.IO.Path]::GetFullPath($OutputRoot)
$projectRootFull = [System.IO.Path]::GetFullPath($projectRoot)
$comparison = if ($IsWindows) { [StringComparison]::OrdinalIgnoreCase } else { [StringComparison]::Ordinal }
if (-not $outputRootFull.StartsWith($projectRootFull + [System.IO.Path]::DirectorySeparatorChar, $comparison)) {
    throw 'Acceptance output must remain inside the CLI checkout.'
}

function Assert-AcceptanceOutputPath([string]$Path) {
    $fullPath = [System.IO.Path]::GetFullPath($Path)
    if (-not $fullPath.StartsWith($projectRootFull + [System.IO.Path]::DirectorySeparatorChar, $comparison)) {
        throw 'Acceptance output must remain inside the CLI checkout.'
    }
    $currentPath = $fullPath
    while ($currentPath) {
        $item = Get-Item -LiteralPath $currentPath -Force -ErrorAction SilentlyContinue
        if ($item -and ($item.Attributes -band [System.IO.FileAttributes]::ReparsePoint)) {
            throw "Linked acceptance output path is not allowed: $currentPath"
        }
        if ([string]::Equals($currentPath, $projectRootFull, $comparison)) { break }
        $currentPath = Split-Path -Parent $currentPath
    }
}

$runId = [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssZ') + '-' + [Guid]::NewGuid().ToString('N')
$runDirectory = Join-Path $outputRootFull $runId
Assert-AcceptanceOutputPath $outputRootFull
$null = New-Item -ItemType Directory -Path $runDirectory -ErrorAction Stop
Assert-AcceptanceOutputPath $runDirectory

function Invoke-RealCli {
    param(
        [Parameter(Mandatory = $true)] [string]$Name,
        [Parameter(Mandatory = $true)] [string[]]$CliArguments,
        [Parameter(Mandatory = $true)] [bool]$ExpectSuccess
    )

    $stdoutPath = Join-Path $runDirectory "$Name.stdout.txt"
    $stderrPath = Join-Path $runDirectory "$Name.stderr.txt"
    & $binary @CliArguments 1> $stdoutPath 2> $stderrPath
    $exitCode = $LASTEXITCODE
    $stdout = Get-Content -LiteralPath $stdoutPath -Raw -ErrorAction SilentlyContinue
    $stderr = Get-Content -LiteralPath $stderrPath -Raw -ErrorAction SilentlyContinue
    if ($ExpectSuccess -and $exitCode -ne 0) {
        throw "${Name} unexpectedly failed with exit code ${exitCode}: $stderr"
    }
    if (-not $ExpectSuccess -and $exitCode -eq 0) {
        throw "$Name unexpectedly succeeded."
    }
    return [ordered]@{
        name = $Name
        expectedSuccess = $ExpectSuccess
        exitCode = $exitCode
        passed = ($ExpectSuccess -eq ($exitCode -eq 0))
        arguments = $CliArguments
        stdoutFile = [System.IO.Path]::GetRelativePath($runDirectory, $stdoutPath).Replace('\', '/')
        stderrFile = [System.IO.Path]::GetRelativePath($runDirectory, $stderrPath).Replace('\', '/')
    }
}

try {
    $fixtures = Join-Path $projectRoot 'tests/fixtures/open-source'
    $porcupine = Join-Path $fixtures 'porcupine-routed/porcupine_routed.kicad_pcb'
    $invalidOutline = Join-Path $fixtures 'kicad-royalblue54l-nfc/RoyalBlue54L-NFC-Antenna.kicad_pcb'
    foreach ($fixture in @($porcupine, $invalidOutline)) {
        if (-not (Test-Path -LiteralPath $fixture -PathType Leaf)) {
            throw "Required acceptance fixture is missing: $fixture"
        }
    }

    $topSvg = Join-Path $runDirectory 'porcupine-top.svg'
    $bottomSvg = Join-Path $runDirectory 'porcupine-bottom.svg'
    $glb = Join-Path $runDirectory 'porcupine-board-shell.glb'
    $refused = Join-Path $runDirectory 'refused-default.glb'
    $invalidGlb = Join-Path $runDirectory 'invalid-outline.glb'
    $package = Join-Path $runDirectory 'porcupine-package'
    $records = @()

    $records += Invoke-RealCli -Name 'inspect' -CliArguments @('inspect', $porcupine, '--json') -ExpectSuccess $true
    $inspection = (Get-Content -LiteralPath (Join-Path $runDirectory 'inspect.stdout.txt') -Raw | ConvertFrom-Json)
    if ($inspection.counts.footprints -le 0 -or $inspection.counts.pads -le 0) {
        throw 'Inspect did not report the expected populated board inventory.'
    }

    $records += Invoke-RealCli -Name 'validate-outline' -CliArguments @('validate', 'outline', $porcupine, '--json') -ExpectSuccess $true
    $outline = (Get-Content -LiteralPath (Join-Path $runDirectory 'validate-outline.stdout.txt') -Raw | ConvertFrom-Json)
    if (-not $outline.valid) { throw 'Validated positive fixture reported an invalid outline.' }

    $records += Invoke-RealCli -Name 'svg-top' -CliArguments @('export', 'svg', $porcupine, '--side', 'top', '--output', $topSvg) -ExpectSuccess $true
    $records += Invoke-RealCli -Name 'svg-bottom' -CliArguments @('export', 'svg', $porcupine, '--side', 'bottom', '--output', $bottomSvg) -ExpectSuccess $true
    foreach ($expectedSvg in @(
        @{ path = $topSvg; side = 'top' },
        @{ path = $bottomSvg; side = 'bottom' }
    )) {
        [xml]$svg = Get-Content -LiteralPath $expectedSvg.path -Raw
        if ($svg.DocumentElement.LocalName -ne 'svg' -or
            $svg.DocumentElement.GetAttribute('data-completeness') -ne 'partial' -or
            $svg.DocumentElement.GetAttribute('data-side') -ne $expectedSvg.side) {
            throw "SVG acceptance artifact is malformed or misclassified: $($expectedSvg.path)"
        }
    }

    $records += Invoke-RealCli -Name 'glb-default-refusal' -CliArguments @('export', 'glb', $porcupine, '--output', $refused) -ExpectSuccess $false
    if (Test-Path -LiteralPath $refused) { throw 'GLB default-refusal case wrote an output.' }

    $records += Invoke-RealCli -Name 'glb-explicit-partial' -CliArguments @('export', 'glb', $porcupine, '--output', $glb, '--allow-partial', '--json-report') -ExpectSuccess $true
    $report = (Get-Content -LiteralPath (Join-Path $runDirectory 'glb-explicit-partial.stdout.txt') -Raw | ConvertFrom-Json)
    if (-not $report.partialExport -or $report.wholeBoardDigitalTwinComplete -or $report.coverage.componentModelsEmitted -ne 0) {
        throw 'GLB report did not preserve board-shell-only partial scope.'
    }
    $glbBytes = [System.IO.File]::ReadAllBytes($glb)
    if ($glbBytes.Length -lt 4 -or [System.Text.Encoding]::ASCII.GetString($glbBytes, 0, 4) -ne 'glTF') {
        throw 'GLB acceptance artifact does not have a glTF binary header.'
    }

    $records += Invoke-RealCli -Name 'glb-no-overwrite' -CliArguments @('export', 'glb', $porcupine, '--output', $glb, '--allow-partial') -ExpectSuccess $false
    $records += Invoke-RealCli -Name 'glb-invalid-outline' -CliArguments @('export', 'glb', $invalidOutline, '--output', $invalidGlb, '--allow-partial') -ExpectSuccess $false
    if (Test-Path -LiteralPath $invalidGlb) { throw 'Invalid-outline GLB case wrote an output.' }

    $records += Invoke-RealCli -Name 'package-build' -CliArguments @('package', 'build', $porcupine, '--output', $package) -ExpectSuccess $true
    $records += Invoke-RealCli -Name 'package-verify' -CliArguments @('package', 'verify', $package, '--json') -ExpectSuccess $true
    $packageReport = (Get-Content -LiteralPath (Join-Path $runDirectory 'package-verify.stdout.txt') -Raw | ConvertFrom-Json)
    if (-not $packageReport.verified -or $packageReport.mobileImportReady) {
        throw 'Package acceptance report made an unsupported mobile-ready claim.'
    }

    $receipt = [ordered]@{
        schemaVersion = 'tokito.real-cli-e2e.v1'
        verifiedAtUtc = [DateTime]::UtcNow.ToString('o')
        passed = $true
        binarySha256 = (Get-FileHash -LiteralPath $binary -Algorithm SHA256).Hash.ToLowerInvariant()
        binaryBytes = (Get-Item -LiteralPath $binary).Length
        fixtures = [ordered]@{
            positive = [System.IO.Path]::GetRelativePath($projectRoot, $porcupine).Replace('\', '/')
            invalidOutline = [System.IO.Path]::GetRelativePath($projectRoot, $invalidOutline).Replace('\', '/')
        }
        checks = $records
        limitations = @(
            'Direct local CLI process acceptance only.',
            'No CAD viewer, Android device, AR registration, Desktop integration, or GUI computer-control test was run.',
            'GLB remains an explicitly partial board shell with no component meshes or selectable component nodes.',
            'This does not establish KiCad CLI behavior or artifact parity.'
        )
    }
    $receipt | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $runDirectory 'receipt.json') -Encoding utf8
    Write-Host "PASS: real CLI E2E receipt: $runDirectory"
}
catch {
    $failure = [ordered]@{
        schemaVersion = 'tokito.real-cli-e2e.v1'
        verifiedAtUtc = [DateTime]::UtcNow.ToString('o')
        passed = $false
        error = $_.Exception.Message
    }
    $failure | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $runDirectory 'receipt.json') -Encoding utf8
    throw
}
