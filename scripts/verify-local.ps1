#Requires -Version 7.0
[CmdletBinding()]
param(
    [string]$CargoPath = 'cargo',
    [string]$PythonPath = 'python',
    [string]$TargetBase = '',
    [string]$DeliveryBase = '',
    [string]$DatasetRepo = '',
    [string]$DatasetBomReport = '',
    [string]$DatasetTwinGlb = '',
    [string]$DatasetTwinComponentMap = '',
    [string]$DatasetTwinManifest = '',
    [string]$DatasetTwinProvenance = '',
    [string]$EmbeddedDatasetBoard = '',
    [string]$EmbeddedDatasetBoardSha256 = '',
    [string]$EmbeddedDatasetExpectedModelCount = '',
    [string]$EmbeddedDatasetExpectedCapturedBytes = '',
    [string]$EmbeddedDatasetExpectedFormat = '',
    [string]$SourceCandidateRoot = '',
    [string]$Esp32CandidateRoot = '',
    [string]$Esp32ReferenceBom = '',
    [string]$Esp32Pcb = '',
    [string]$Esp32ModelLibrary = '',
    [string]$Esp8266SourceRoot = '',
    [string]$Esp8266UnitReceipt = '',
    [string]$Esp8266FreecadReference = '',
    [string]$NxpRefinementBaseline = '',
    [string]$NxpRefinementSpec = '',
    [string]$NxpRefinementPcb = ''
)

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

function Assert-NoLinkedPath([string]$Path) {
    $currentPath = [System.IO.Path]::GetFullPath($Path)
    while ($currentPath) {
        $item = Get-Item -LiteralPath $currentPath -Force -ErrorAction SilentlyContinue
        if ($item -and ($item.Attributes -band [System.IO.FileAttributes]::ReparsePoint)) {
            throw "Linked build path is not allowed: $currentPath"
        }
        $parentPath = Split-Path -Parent $currentPath
        if (-not $parentPath -or [string]::Equals($parentPath, $currentPath, [StringComparison]::OrdinalIgnoreCase)) { break }
        $currentPath = $parentPath
    }
}

function Get-InputFingerprints {
    $paths = @('Cargo.toml', 'Cargo.lock', 'scripts/verify-local.ps1', 'scripts/real-cli-e2e.ps1', 'README.md')
    foreach ($optionalRootFile in @('AGENTS.md')) {
        if (Test-Path -LiteralPath (Join-Path $projectRoot $optionalRootFile)) { $paths += $optionalRootFile }
    }
    foreach ($directory in @('src', 'tests', 'examples', 'experiments', 'docs', '.github')) {
        $inputDirectory = Join-Path $projectRoot $directory
        if (Test-Path -LiteralPath $inputDirectory) {
            $paths += Get-ChildItem -LiteralPath $inputDirectory -Recurse -File |
                Where-Object {
                    $_.FullName -notmatch '(?i)(^|[\\/])__pycache__([\\/]|$)' -and
                    $_.FullName -notmatch '(?i)(^|[\\/])target([\\/]|$)' -and
                    # KiCad creates these per-user editor/view preferences
                    # beside a board file. They are not source fixtures and
                    # must not make a portable verification receipt depend on
                    # local UI state.
                    $_.Extension -notin @('.pyc', '.pyo', '.kicad_prl')
                } |
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
    # Fresh outputs avoid replacing Windows test executables that may be locked.
    # Each run remains isolated inside this checkout; previous evidence is retained.
    $verificationRun = [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssZ') + '-' + [Guid]::NewGuid().ToString('N')
    if ([string]::IsNullOrWhiteSpace($TargetBase)) {
        $targetParent = Join-Path $projectRoot 'target/local-verification'
        Assert-LocalOutputPath $targetParent
    } else {
        if (-not [System.IO.Path]::IsPathFullyQualified($TargetBase)) {
            throw 'TargetBase must be an absolute path.'
        }
        $targetParent = [System.IO.Path]::GetFullPath($TargetBase)
        Assert-NoLinkedPath $targetParent
        New-Item -ItemType Directory -Path $targetParent -Force -ErrorAction Stop | Out-Null
        Assert-NoLinkedPath $targetParent
    }
    $env:CARGO_TARGET_DIR = Join-Path $targetParent $verificationRun
    if ([string]::IsNullOrWhiteSpace($TargetBase)) {
        Assert-LocalOutputPath $env:CARGO_TARGET_DIR
    } else {
        Assert-NoLinkedPath $env:CARGO_TARGET_DIR
    }
    if (Test-Path -LiteralPath $env:CARGO_TARGET_DIR) {
        foreach ($item in (Get-ChildItem -LiteralPath $env:CARGO_TARGET_DIR -Recurse -Force)) {
            if ($item.Attributes -band [System.IO.FileAttributes]::ReparsePoint) {
                throw "Linked build output is not allowed: $($item.FullName)"
            }
        }
    }
    $binaryName = if ($IsWindows) { 'tokito-cli.exe' } else { 'tokito-cli' }
    $binaryPath = Join-Path $env:CARGO_TARGET_DIR "release/$binaryName"
    $bundleHookTarget = Join-Path $env:CARGO_TARGET_DIR 'bundle-test-hooks'
    $bundleHookBinary = Join-Path $bundleHookTarget "release/$binaryName"
    $geometryOutputPath = Join-Path $env:CARGO_TARGET_DIR 'geometry-e2e'
    $inputs = Get-InputFingerprints
    $checks = @(
        @{ name = 'format'; command = $cargoCommand.Source; arguments = @('fmt', '--all', '--check') },
        @{ name = 'embedded-dataset-verifier-tests'; command = $pythonCommand.Source; arguments = @('tests/test_embedded_dataset_e2e.py') },
        @{ name = 'tests'; command = $cargoCommand.Source; arguments = @('test', '--offline', '--locked') },
        @{ name = 'clippy'; command = $cargoCommand.Source; arguments = @('clippy', '--offline', '--locked', '--all-targets', '--', '-D', 'warnings') },
        @{ name = 'release'; command = $cargoCommand.Source; arguments = @('build', '--release', '--offline', '--locked') },
        @{ name = 'e2e'; command = $pythonCommand.Source; arguments = @('tests/e2e_cli.py', '--binary', $binaryPath) },
        @{ name = 'kicad-10.0.7-pos-e2e'; command = $pythonCommand.Source; arguments = @('tests/kicad_pos_compat_e2e.py', '--binary', $binaryPath) },
        @{ name = 'kicad-10.0.7-type-filter-e2e'; command = $pythonCommand.Source; arguments = @('tests/kicad_type_filter_e2e.py', '--binary', $binaryPath) },
        @{ name = 'geometry-e2e'; command = $pythonCommand.Source; arguments = @('tests/geometry_e2e.py', '--binary', $binaryPath, '--output-dir', $geometryOutputPath) },
        @{ name = 'glb-e2e'; command = $pythonCommand.Source; arguments = @('tests/glb_e2e.py', '--binary', $binaryPath) },
        @{ name = 'model-resolution-e2e'; command = $pythonCommand.Source; arguments = @('tests/model_resolution_e2e.py', '--binary', $binaryPath) },
        @{ name = 'compat-v0'; command = $pythonCommand.Source; arguments = @('tests/compat_v0.py', '--binary', $binaryPath) },
        @{ name = 'bom-e2e'; command = $pythonCommand.Source; arguments = @('tests/bom_e2e.py', '--binary', $binaryPath) },
        @{ name = 'model-assignment-e2e'; command = $pythonCommand.Source; arguments = @('tests/model_assignments_e2e.py', '--binary', $binaryPath) },
        @{ name = 'component-envelope-e2e'; command = $pythonCommand.Source; arguments = @('tests/component_envelope_e2e.py', '--binary', $binaryPath) },
        @{ name = 'project-intake-e2e'; command = $pythonCommand.Source; arguments = @('tests/project_intake_e2e.py', '--bin', $binaryPath) },
        @{ name = 'project-bundle-e2e'; command = $pythonCommand.Source; arguments = @('tests/project_bundle_e2e.py', '--binary', $binaryPath) },
        @{ name = 'project-bundle-test-hooks-build'; command = $cargoCommand.Source; arguments = @('build', '--release', '--offline', '--locked', '--features', 'e2e-test-hooks', '--target-dir', $bundleHookTarget) },
        @{ name = 'project-bundle-cleanup-e2e'; command = $pythonCommand.Source; arguments = @('tests/project_bundle_e2e.py', '--binary', $bundleHookBinary, '--fault-hook-only') },
        @{ name = 'footprint-family-e2e'; command = $pythonCommand.Source; arguments = @('tests/footprint_family_e2e.py', '--binary', $binaryPath) },
        @{ name = 'footprint-spec-e2e'; command = $pythonCommand.Source; arguments = @('tests/footprint_spec_e2e.py', '--binary', $binaryPath) },
        @{ name = 'footprint-inspect-e2e'; command = $pythonCommand.Source; arguments = @('tests/footprint_inspect_e2e.py', '--binary', $binaryPath) },
        @{ name = 'twin-e2e'; command = $pythonCommand.Source; arguments = @('tests/twin_e2e.py', '--binary', $binaryPath) },
        @{ name = 'dataset-source-e2e'; command = $pythonCommand.Source; arguments = @('tests/dataset_e2e.py', '--binary', $binaryPath) },
        @{ name = 'dataset-git-e2e'; command = $pythonCommand.Source; arguments = @('tests/dataset_git_e2e.py', '--binary', $binaryPath) },
        @{ name = 'real-cli-e2e'; command = (Join-Path $PSHOME 'pwsh.exe'); arguments = @('-NoProfile', '-File', 'scripts/real-cli-e2e.ps1', '-BinaryPath', $binaryPath) },
        @{ name = 'step-probe-format'; command = $cargoCommand.Source; arguments = @('fmt', '--manifest-path', 'experiments/step-evaluation/Cargo.toml', '--all', '--check') },
        @{ name = 'step-probe-tests'; command = $cargoCommand.Source; arguments = @('test', '--manifest-path', 'experiments/step-evaluation/Cargo.toml', '--offline', '--locked') },
        @{ name = 'torus-inverse-regression'; command = $cargoCommand.Source; arguments = @('test', '--manifest-path', 'experiments/step-evaluation/Cargo.toml', '--example', 'torus_inverse_regression', '--offline', '--locked') },
        @{ name = 'step-probe-clippy'; command = $cargoCommand.Source; arguments = @('clippy', '--manifest-path', 'experiments/step-evaluation/Cargo.toml', '--offline', '--locked', '--all-targets', '--', '-D', 'warnings') },
        @{ name = 'step-probe-build'; command = $cargoCommand.Source; arguments = @('build', '--manifest-path', 'experiments/step-evaluation/Cargo.toml', '--offline', '--locked') },
        @{ name = 'altium-probe-format'; command = $cargoCommand.Source; arguments = @('fmt', '--manifest-path', 'experiments/altium-evaluation/Cargo.toml', '--all', '--check') },
        @{ name = 'altium-probe-tests'; command = $cargoCommand.Source; arguments = @('test', '--manifest-path', 'experiments/altium-evaluation/Cargo.toml', '--offline', '--locked') },
        @{ name = 'altium-probe-clippy'; command = $cargoCommand.Source; arguments = @('clippy', '--manifest-path', 'experiments/altium-evaluation/Cargo.toml', '--offline', '--locked', '--all-targets', '--', '-D', 'warnings') },
        @{ name = 'altium-probe-build'; command = $cargoCommand.Source; arguments = @('build', '--manifest-path', 'experiments/altium-evaluation/Cargo.toml', '--offline', '--locked') }
    )
    $embeddedDatasetInputs = @($EmbeddedDatasetBoard, $EmbeddedDatasetBoardSha256, $EmbeddedDatasetExpectedModelCount, $EmbeddedDatasetExpectedCapturedBytes, $EmbeddedDatasetExpectedFormat)
    $embeddedDatasetInputCount = @($embeddedDatasetInputs | Where-Object { -not [string]::IsNullOrWhiteSpace($_) }).Count
    if ($embeddedDatasetInputCount -notin @(0,5)) { throw 'Embedded dataset board, SHA-256, expected model count, captured-byte total and format must be supplied together.' }
    if ($embeddedDatasetInputCount -eq 5) {
        Assert-NoLinkedPath $EmbeddedDatasetBoard
        if (-not (Test-Path -LiteralPath $EmbeddedDatasetBoard -PathType Leaf)) { throw "Missing embedded dataset board: $EmbeddedDatasetBoard" }
        if ($EmbeddedDatasetBoardSha256 -notmatch '^[0-9A-Fa-f]{64}$') { throw 'Embedded dataset board SHA-256 must contain 64 hexadecimal characters.' }
        $embeddedModelCount = 0
        if (-not [int]::TryParse($EmbeddedDatasetExpectedModelCount, [ref]$embeddedModelCount) -or $embeddedModelCount -le 0) { throw 'Embedded dataset expected model count must be a positive integer.' }
        $embeddedCapturedBytes = 0
        if (-not [int]::TryParse($EmbeddedDatasetExpectedCapturedBytes, [ref]$embeddedCapturedBytes) -or $embeddedCapturedBytes -le 0) { throw 'Embedded dataset expected captured-byte total must be a positive integer.' }
        if ($EmbeddedDatasetExpectedFormat -notin @('step','wrl','glb','gltf')) { throw 'Embedded dataset expected format must be step, wrl, glb, or gltf.' }
        $embeddedReceipt = Join-Path $env:CARGO_TARGET_DIR 'embedded-dataset-e2e.json'
        $embeddedAssets = Join-Path $env:CARGO_TARGET_DIR 'embedded-model-assets'
        $checks += @{ name='embedded-dataset-e2e'; command=$pythonCommand.Source; arguments=@('tests/embedded_dataset_e2e.py','--binary',$binaryPath,'--board',$EmbeddedDatasetBoard,'--expected-sha256',$EmbeddedDatasetBoardSha256,'--expected-model-count',[string]$embeddedModelCount,'--expected-captured-bytes',[string]$embeddedCapturedBytes,'--expected-format',$EmbeddedDatasetExpectedFormat,'--extract-dir',$embeddedAssets,'--require-unique-owners','--receipt',$embeddedReceipt); receiptPath=$embeddedReceipt }
    }
    $checks += @(
        @{ name='twin-refinement-format'; command=$cargoCommand.Source; arguments=@('fmt','--manifest-path','experiments/twin-refinement/Cargo.toml','--all','--check') },
        @{ name='twin-refinement-tests'; command=$cargoCommand.Source; arguments=@('test','--manifest-path','experiments/twin-refinement/Cargo.toml','--offline','--locked') },
        @{ name='twin-refinement-clippy'; command=$cargoCommand.Source; arguments=@('clippy','--manifest-path','experiments/twin-refinement/Cargo.toml','--offline','--locked','--all-targets','--','-D','warnings') },
        @{ name='twin-refinement-build'; command=$cargoCommand.Source; arguments=@('build','--manifest-path','experiments/twin-refinement/Cargo.toml','--offline','--locked') }
    )
    $nxpInputs=@($NxpRefinementBaseline,$NxpRefinementSpec,$NxpRefinementPcb)
    $nxpInputCount=@($nxpInputs | Where-Object { -not [string]::IsNullOrWhiteSpace($_) }).Count
    if ($nxpInputCount -notin @(0,3)) { throw 'NXP baseline, explicit refinement spec and PCB must be supplied together.' }
    if ($nxpInputCount -eq 3) {
        foreach ($path in $nxpInputs) { Assert-NoLinkedPath $path; if (-not (Test-Path -LiteralPath $path)) { throw "Missing NXP input: $path" } }
        $nxpOutput=Join-Path $env:CARGO_TARGET_DIR 'nxp-refinement-e2e'
        $nxpBinary=Join-Path $env:CARGO_TARGET_DIR ('debug/tokito-twin-refinement' + $(if ($IsWindows) { '.exe' } else { '' }))
        $checks += @{ name='nxp-refinement-source-e2e'; command=$pythonCommand.Source; arguments=@('tests/nxp_refinement_e2e.py','--baseline',$NxpRefinementBaseline,'--spec',$NxpRefinementSpec,'--pcb',$NxpRefinementPcb,'--binary',$nxpBinary,'--cli-binary',$binaryPath,'--output',$nxpOutput,'--generate'); receiptPath=(Join-Path $nxpOutput 'refinement-e2e.json') }
    }
    $esp32Inputs = @($Esp32CandidateRoot, $Esp32ReferenceBom, $Esp32Pcb, $Esp32ModelLibrary)
    $esp32Count = @($esp32Inputs | Where-Object { -not [string]::IsNullOrWhiteSpace($_) }).Count
    if ($esp32Count -notin @(0,4)) { throw 'All four ESP32 source inputs must be supplied together.' }
    if ($esp32Count -eq 4) {
        foreach ($path in $esp32Inputs) { Assert-NoLinkedPath $path; if (-not (Test-Path -LiteralPath $path)) { throw "Missing ESP32 input: $path" } }
        $esp32Prototype = Join-Path $env:CARGO_TARGET_DIR 'esp32-candidate-e2e'
        $esp32Native = Join-Path $env:CARGO_TARGET_DIR 'esp32-native-e2e'
        $checks += @{ name='esp32-candidate-e2e'; command=$pythonCommand.Source; arguments=@('tests/esp32_candidate_e2e.py','--binary',$binaryPath,'--source',$Esp32CandidateRoot,'--reference-bom',$Esp32ReferenceBom,'--output',$esp32Prototype); receiptPath=(Join-Path $esp32Prototype 'e2e-receipt.json') }
        $checks += @{ name='esp32-native-e2e'; command=$pythonCommand.Source; arguments=@('tests/esp32_chaining_e2e.py','--binary',$binaryPath,'--board',$Esp32Pcb,'--library',$Esp32ModelLibrary,'--library-profile','kicad9-wrl','--reference-bom',$Esp32ReferenceBom,'--output',$esp32Native); receiptPath=(Join-Path $esp32Native 'receipt.json') }
        $esp32Switches = Join-Path $env:CARGO_TARGET_DIR 'esp32-switches-e2e'
        $checks += @{ name='esp32-switches-e2e'; command=$pythonCommand.Source; arguments=@('tests/esp32_switch_e2e.py','--binary',$binaryPath,'--board',$Esp32Pcb,'--library',$Esp32ModelLibrary,'--baseline',(Join-Path $esp32Native 'derived-module'),'--bom',$Esp32ReferenceBom,'--output',$esp32Switches); receiptPath=(Join-Path $esp32Switches 'receipt.json') }
        $esp32ZeroArea = Join-Path $env:CARGO_TARGET_DIR 'esp32-zero-area-e2e'
        $checks += @{ name='esp32-zero-area-e2e'; command=$pythonCommand.Source; arguments=@('tests/esp32_zero_area_e2e.py','--binary',$binaryPath,'--board',$Esp32Pcb,'--library',$Esp32ModelLibrary,'--baseline',$esp32Switches,'--bom',$Esp32ReferenceBom,'--output',$esp32ZeroArea); receiptPath=(Join-Path $esp32ZeroArea 'receipt.json') }
    }
    $esp8266Inputs = @($Esp8266SourceRoot, $Esp8266UnitReceipt, $Esp8266FreecadReference)
    $esp8266InputCount = @($esp8266Inputs | Where-Object { -not [string]::IsNullOrWhiteSpace($_) }).Count
    if ($esp8266InputCount -notin @(0,3)) { throw 'ESP8266 source, independent unit receipt and FreeCAD reference must be supplied together.' }
    if ($esp8266InputCount -eq 3) {
        foreach ($path in @($Esp8266SourceRoot, $Esp8266UnitReceipt, $Esp8266FreecadReference)) { Assert-NoLinkedPath $path; if (-not (Test-Path -LiteralPath $path)) { throw "Missing ESP8266 input: $path" } }
        # This probe enforces that its receipt directory lives inside the CLI
        # checkout's target tree. Keep build artifacts on an optional external
        # Cargo target (for example, when the system drive is low on space), but
        # place this source-derived receipt under the guarded project target.
        $stepOutput=Join-Path $projectRoot "target/github-delivery/step-length-unit-preflight-$verificationRun"
        Assert-LocalOutputPath $stepOutput
        $occurrenceOutput=Join-Path $env:CARGO_TARGET_DIR 'esp8266-step-occurrence-world-e2e'
        $altiumOutput=Join-Path $env:CARGO_TARGET_DIR 'esp8266-altium-e2e'
        $stepBinary=Join-Path $env:CARGO_TARGET_DIR ('debug/tokito-step-evaluation' + $(if ($IsWindows) { '.exe' } else { '' }))
        $altiumBinary=Join-Path $env:CARGO_TARGET_DIR ('debug/tokito-altium-evaluation' + $(if ($IsWindows) { '.exe' } else { '' }))
        $checks += @{ name='esp8266-step-units-e2e'; command=$pythonCommand.Source; arguments=@('tests/step_units_e2e.py','--binary',$stepBinary,'--models',(Join-Path $Esp8266SourceRoot 'structured-source/embedded-models'),'--source-receipt',$Esp8266UnitReceipt,'--output',$stepOutput); receiptPath=(Join-Path $stepOutput 'receipt.json') }
        $checks += @{ name='esp8266-step-occurrence-world-e2e'; command=$pythonCommand.Source; arguments=@('tests/step_occurrence_world_e2e.py','--binary',$stepBinary,'--models-root',(Join-Path $Esp8266SourceRoot 'structured-source/embedded-models'),'--reference',$Esp8266FreecadReference,'--source-receipt',(Join-Path $stepOutput 'receipt.json'),'--output',$occurrenceOutput); receiptPath=(Join-Path $occurrenceOutput 'receipt.json') }
        $candidateMeshOutput=Join-Path $env:CARGO_TARGET_DIR 'esp8266-step-candidate-mesh-e2e'
        $checks += @{ name='esp8266-step-candidate-mesh-e2e'; command=$pythonCommand.Source; arguments=@('tests/step_candidate_mesh_e2e.py','--binary',$stepBinary,'--models',(Join-Path $Esp8266SourceRoot 'structured-source/embedded-models'),'--reference',$Esp8266FreecadReference,'--output',$candidateMeshOutput); receiptPath=(Join-Path $candidateMeshOutput 'receipt.json') }
        $checks += @{ name='esp8266-altium-e2e'; command=$pythonCommand.Source; arguments=@('tests/altium_source_e2e.py','--binary',$altiumBinary,'--source',$Esp8266SourceRoot,'--output',$altiumOutput); receiptPath=(Join-Path $altiumOutput 'receipt.json') }
        $associationOutput=Join-Path $env:CARGO_TARGET_DIR 'esp8266-associations-e2e'
        $checks += @{ name='esp8266-associations-e2e'; command=$pythonCommand.Source; arguments=@('tests/esp8266_assembly_candidates_e2e.py','--binary',$altiumBinary,'--source',$Esp8266SourceRoot,'--output',$associationOutput); receiptPath=(Join-Path $associationOutput 'receipt.json') }
        $prototypeOutput=Join-Path $env:CARGO_TARGET_DIR 'esp8266-prototype-e2e'
        $drillReceipt=Join-Path $env:CARGO_TARGET_DIR 'esp8266-drill-inventory-e2e.json'
        $checks += @{ name='esp8266-drill-inventory-e2e'; command=$pythonCommand.Source; arguments=@('tests/esp8266_drill_inventory_e2e.py','--binary',$altiumBinary,'--source',(Join-Path $Esp8266SourceRoot 'gerbers-verified/NODEMCU_DEVKIT_V1.0_GERBER_V4'),'--output',$drillReceipt); receiptPath=$drillReceipt }
        $checks += @{ name='esp8266-prototype-e2e'; command=$pythonCommand.Source; arguments=@('tests/esp8266_prototype_e2e.py','--binary',$altiumBinary,'--pcb',(Join-Path $Esp8266SourceRoot 'NODEMCU_DEVKIT_V1.0.PcbDoc'),'--meshes',$candidateMeshOutput,'--output',$prototypeOutput,'--cli-binary',(Join-Path $env:CARGO_TARGET_DIR ('release/tokito-cli' + $(if ($IsWindows) { '.exe' } else { '' })))); receiptPath=(Join-Path $prototypeOutput 'e2e-receipt.json') }
    }
    if ([string]::IsNullOrWhiteSpace($DatasetRepo) -xor [string]::IsNullOrWhiteSpace($DatasetBomReport)) {
        throw 'DatasetRepo and DatasetBomReport must be supplied together.'
    }
    if (-not [string]::IsNullOrWhiteSpace($DatasetRepo)) {
        if (-not (Test-Path -LiteralPath $DatasetRepo -PathType Container)) {
            throw "DatasetRepo directory does not exist: $DatasetRepo"
        }
        if (-not (Test-Path -LiteralPath $DatasetBomReport -PathType Leaf)) {
            throw "DatasetBomReport file does not exist: $DatasetBomReport"
        }
        $datasetOutputPath = Join-Path $targetParent "component-forge-dataset/verification-$verificationRun"
        if ([string]::IsNullOrWhiteSpace($TargetBase)) {
            Assert-LocalOutputPath $datasetOutputPath
        } else {
            Assert-NoLinkedPath $datasetOutputPath
        }
        $checks += @{
            name = 'component-dataset-e2e'
            command = $pythonCommand.Source
            arguments = @(
                'tests/component_dataset_e2e.py', '--binary', $binaryPath,
                '--dataset-repo', $DatasetRepo, '--bom-report', $DatasetBomReport,
                '--output-dir', $datasetOutputPath, '--output-root', $targetParent
            )
        }
    }
    $datasetTwinInputs = @($DatasetTwinGlb, $DatasetTwinComponentMap, $DatasetTwinManifest, $DatasetTwinProvenance)
    $datasetTwinInputCount = @($datasetTwinInputs | Where-Object { -not [string]::IsNullOrWhiteSpace($_) }).Count
    if ($datasetTwinInputCount -notin @(0, 4)) {
        throw 'DatasetTwinGlb, DatasetTwinComponentMap, DatasetTwinManifest, and DatasetTwinProvenance must be supplied together.'
    }
    if ($datasetTwinInputCount -eq 4) {
        foreach ($inputPath in $datasetTwinInputs) {
            if (-not (Test-Path -LiteralPath $inputPath -PathType Leaf)) {
                throw "Dataset twin input does not exist: $inputPath"
            }
            Assert-NoLinkedPath $inputPath
        }
        $datasetTwinOutputPath = Join-Path $targetParent "dataset-twin-e2e/verification-$verificationRun"
        if ([string]::IsNullOrWhiteSpace($TargetBase)) {
            Assert-LocalOutputPath $datasetTwinOutputPath
        } else {
            Assert-NoLinkedPath $datasetTwinOutputPath
        }
        $checks += @{
            name = 'dataset-twin-e2e'
            command = $pythonCommand.Source
            arguments = @(
                'tests/dataset_twin_e2e.py', '--glb', $DatasetTwinGlb,
                '--component-map', $DatasetTwinComponentMap, '--manifest', $DatasetTwinManifest,
                '--provenance', $DatasetTwinProvenance, '--output-dir', $datasetTwinOutputPath,
                '--output-root', $targetParent, '--binary', $binaryPath
            )
        }
        if (-not [string]::IsNullOrWhiteSpace($DatasetRepo)) {
            $checks[-1].arguments += @('--dataset-repo', $DatasetRepo)
        }
    }
    if (-not [string]::IsNullOrWhiteSpace($SourceCandidateRoot)) {
        Assert-NoLinkedPath $SourceCandidateRoot
        $checks += @{
            name = 'source-candidate-e2e'
            command = $pythonCommand.Source
            arguments = @('tests/source_candidate_e2e.py', '--binary', $binaryPath,
                '--source-root', $SourceCandidateRoot, '--output-dir', (Join-Path $targetParent "source-candidate-e2e/verification-$verificationRun"))
        }
    }
    $datasetTwinHashesBefore = $null
    if ($datasetTwinInputCount -eq 4) {
        $datasetTwinHashesBefore = @($datasetTwinInputs | ForEach-Object { (Get-FileHash -LiteralPath $_ -Algorithm SHA256).Hash.ToLowerInvariant() })
    }
    $results = @()
    $testedBinaryHash = $null
    foreach ($check in $checks) {
        Write-Host "Running $($check.name)..."
        $timer = [System.Diagnostics.Stopwatch]::StartNew()
        $command = $check.command
        $commandArguments = $check.arguments
        if ($check.name -eq 'e2e') {
            if ([string]::IsNullOrWhiteSpace($TargetBase)) {
                Assert-LocalOutputPath $binaryPath
            } else {
                Assert-NoLinkedPath $binaryPath
            }
            $testedBinaryHash = (Get-FileHash -LiteralPath $binaryPath -Algorithm SHA256).Hash.ToLowerInvariant()
        }
        if ($check.name -eq 'geometry-e2e') {
            if ([string]::IsNullOrWhiteSpace($TargetBase)) {
                Assert-LocalOutputPath $geometryOutputPath
            } else {
                Assert-NoLinkedPath $geometryOutputPath
            }
        }
        $datasetBomReportHashBefore = $null
        if ($check.name -eq 'component-dataset-e2e') {
            $datasetBomReportHashBefore = (Get-FileHash -LiteralPath $DatasetBomReport -Algorithm SHA256).Hash.ToLowerInvariant()
        }
        & $command @commandArguments
        if ($LASTEXITCODE -ne 0) { throw "$($check.name) failed with exit code $LASTEXITCODE; no delivery receipt created." }
        if ($testedBinaryHash -and (Get-FileHash -LiteralPath $binaryPath -Algorithm SHA256).Hash.ToLowerInvariant() -ne $testedBinaryHash) {
            throw 'Binary changed during E2E; rerun before delivery.'
        }
        $timer.Stop()
        $checkResult = @{ name = $check.name; passed = $true; seconds = $timer.Elapsed.TotalSeconds }
        if ($check.ContainsKey('receiptPath')) {
            $evidencePath = $check.receiptPath
            if (-not (Test-Path -LiteralPath $evidencePath -PathType Leaf)) { throw "Missing E2E receipt: $evidencePath" }
            $evidence = Get-Content -LiteralPath $evidencePath -Raw | ConvertFrom-Json
            if ($evidence.passed -ne $true) { throw "E2E receipt did not pass: $evidencePath" }
            $checkResult.evidence = @{ receiptPath=[System.IO.Path]::GetRelativePath($projectRoot,$evidencePath).Replace('\','/'); receiptSha256=(Get-FileHash -LiteralPath $evidencePath -Algorithm SHA256).Hash.ToLowerInvariant() }
        }
        if ($check.name -eq 'component-dataset-e2e') {
            $datasetE2eReceiptPath = Join-Path $targetParent "component-forge-dataset/verification-$verificationRun/e2e-receipt.json"
            if (-not (Test-Path -LiteralPath $datasetE2eReceiptPath -PathType Leaf)) {
                throw 'Dataset E2E passed without its required evidence receipt; no delivery receipt created.'
            }
            $datasetBomReportHashAfter = (Get-FileHash -LiteralPath $DatasetBomReport -Algorithm SHA256).Hash.ToLowerInvariant()
            if ($datasetBomReportHashAfter -ne $datasetBomReportHashBefore) {
                throw 'BOM report changed during dataset E2E; rerun with a stable input.'
            }
            $datasetE2eReceipt = Get-Content -LiteralPath $datasetE2eReceiptPath -Raw | ConvertFrom-Json
            if ($datasetE2eReceipt.bomReportFileSha256 -ne $datasetBomReportHashAfter) {
                throw 'Dataset E2E receipt does not bind the BOM report bytes supplied to the verifier.'
            }
            $checkResult.evidence = @{
                receiptPath = if ($datasetE2eReceiptPath.StartsWith($projectRoot + [System.IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
                    [System.IO.Path]::GetRelativePath($projectRoot, $datasetE2eReceiptPath).Replace('\', '/')
                } else {
                    [System.IO.Path]::GetFullPath($datasetE2eReceiptPath)
                }
                receiptSha256 = (Get-FileHash -LiteralPath $datasetE2eReceiptPath -Algorithm SHA256).Hash.ToLowerInvariant()
                datasetCommit = $datasetE2eReceipt.datasetCommit
                boardFileSha256 = $datasetE2eReceipt.boardFileSha256
                bomReportFileSha256 = $datasetE2eReceipt.bomReportFileSha256
            }
        }
        if ($check.name -eq 'dataset-twin-e2e') {
            $twinE2eReceiptPath = Join-Path $datasetTwinOutputPath 'e2e-receipt.json'
            if (-not (Test-Path -LiteralPath $twinE2eReceiptPath -PathType Leaf)) {
                throw 'Dataset twin E2E passed without its required evidence receipt; no delivery receipt created.'
            }
            $datasetTwinHashesAfter = @($datasetTwinInputs | ForEach-Object { (Get-FileHash -LiteralPath $_ -Algorithm SHA256).Hash.ToLowerInvariant() })
            if (($datasetTwinHashesBefore -join ',') -ne ($datasetTwinHashesAfter -join ',')) {
                throw 'Dataset twin source files changed during E2E; rerun with stable inputs.'
            }
            $twinE2eReceipt = Get-Content -LiteralPath $twinE2eReceiptPath -Raw | ConvertFrom-Json
            $receiptHashes = @(
                $twinE2eReceipt.sourceSha256.glb,
                $twinE2eReceipt.sourceSha256.componentMap,
                $twinE2eReceipt.sourceSha256.manifest,
                $twinE2eReceipt.sourceSha256.provenance
            )
            if (($datasetTwinHashesAfter -join ',') -ne ($receiptHashes -join ',')) {
                throw 'Dataset twin receipt hashes do not bind the supplied source files.'
            }
            $checkResult.evidence = @{
                receiptPath = if ($twinE2eReceiptPath.StartsWith($projectRoot + [System.IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
                    [System.IO.Path]::GetRelativePath($projectRoot, $twinE2eReceiptPath).Replace('\', '/')
                } else {
                    [System.IO.Path]::GetFullPath($twinE2eReceiptPath)
                }
                receiptSha256 = (Get-FileHash -LiteralPath $twinE2eReceiptPath -Algorithm SHA256).Hash.ToLowerInvariant()

                datasetPullRequestHead = $twinE2eReceipt.datasetPullRequestHead
                glbSha256 = $twinE2eReceipt.sourceSha256.glb
                componentMapSha256 = $twinE2eReceipt.sourceSha256.componentMap
                selectorsFoundExactlyOnce = $twinE2eReceipt.selectorsFoundExactlyOnce
                selectedSubtreeMeshes = $twinE2eReceipt.selectedSubtreeMeshes
            }
        }
        $results += $checkResult
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
    if ([string]::IsNullOrWhiteSpace($DeliveryBase)) {
        $deliveryParent = Join-Path $projectRoot 'target/delivery/local'
        Assert-LocalOutputPath (Join-Path $deliveryParent $deliveryName)
    } else {
        if (-not [System.IO.Path]::IsPathFullyQualified($DeliveryBase)) {
            throw 'DeliveryBase must be an absolute path.'
        }
        $deliveryParent = [System.IO.Path]::GetFullPath($DeliveryBase)
        Assert-NoLinkedPath $deliveryParent
        New-Item -ItemType Directory -Path $deliveryParent -Force -ErrorAction Stop | Out-Null
        Assert-NoLinkedPath $deliveryParent
    }
    $deliveryPath = Join-Path $deliveryParent $deliveryName
    if ([string]::IsNullOrWhiteSpace($DeliveryBase)) {
        Assert-LocalOutputPath $deliveryPath
    } else {
        Assert-NoLinkedPath $deliveryPath
    }
    if ([string]::IsNullOrWhiteSpace($TargetBase)) {
        Assert-LocalOutputPath $binaryPath
    } else {
        Assert-NoLinkedPath $binaryPath
    }
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
        buildConfiguration = @{
            devDebug = $env:CARGO_PROFILE_DEV_DEBUG
            testDebug = $env:CARGO_PROFILE_TEST_DEBUG
            incremental = $env:CARGO_INCREMENTAL
        }
        checks = $results
        inputsSha256 = $inputs
        binary = @{ file = $binaryName; sha256 = $testedBinaryHash; bytes = (Get-Item -LiteralPath $deliveredBinary).Length }
        limitations = @('Local platform/toolchain only.', 'The release CLI acceptance receipt exercises one bounded local VRML component assembly; the full subprocess suite additionally checks one- and two-component GLBs, exact-refdes selection behavior, unsupported-side diagnostics, and coverage truncation. These CLI tests do not exercise the App importer or Android renderer.', 'General VRML, STEP, glTF and GLB model import, KiCad-export equivalence, physical fit, phone/AR and Desktop acceptance remain unverified.')
    }
    $receipt | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $deliveryPath 'verification.json') -Encoding utf8
    Write-Host "PASS: verified local binary and receipt: $deliveryPath"
}
finally {
    $env:RUSTC = $previousRustc
    $env:CARGO_TARGET_DIR = $previousTarget
    Pop-Location
}
