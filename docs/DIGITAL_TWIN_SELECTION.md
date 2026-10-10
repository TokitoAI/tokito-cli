# Component selection and AR handoff plan

Status: pure-Rust CLI preparation and exact-reference/class-claim querying are implemented. App import, colour changes, reviewed board-reference admission and physical registration are separate, unfinished stages.

## Current ecosystem evidence

App main inspected: 780f3fbef5e0c207c736b7ad4ab9a28f3e15ec50. Its [DigitalTwinView](https://github.com/TokitoAI/tokito-app/blob/780f3fbef5e0c207c736b7ad4ab9a28f3e15ec50/physical-runtime/android/ar-runtime/src/main/kotlin/ai/tokito/physical/ar/DigitalTwinView.kt) filters actual renderable entity names using ref:<reference>. Its [host](https://github.com/TokitoAI/tokito-app/blob/780f3fbef5e0c207c736b7ad4ab9a28f3e15ec50/physical-runtime/app/src/main/java/dev/tokito/physical/MainActivity.kt) reads active.board-reference.json, active.digital-twin.json and active.digital-twin.glb from its private contexts directory. The twin must bind to the admitted board-reference revision. This is not an established desktop/mobile package-import bridge.

Dataset main remains 257ea7e68ee2e700522bd1b19ad9cbf1c0c3980a. [Dataset PR #6](https://github.com/TokitoAI/tokito-app-dataset/pull/6) advanced to 5283bc059e30e51f4d3a81a674425936fc213214. Its 42-file update adds capture/annotation evidence and validation tooling; it changes no STEP/GLB files. The original NXP map, manifest and provenance remain byte-identical to the previously pinned ab1420fb snapshot. The externally referenced GLB contains 391 mapped references and 354 independently isolated component meshes.

The source NXP GLB names each placement parent ref:<reference>, while its mesh child has another name. None of the 354 reference nodes is directly renderable. Since App's filter checks renderable names, this hierarchy does not implement the intended filter. CLI preparation moves each reference name to its single mesh-bearing descendant and names the parent placement:<reference>. All parent/child links, geometry, material references and transform values remain unchanged. Separate files per component are unnecessary for this runtime: one GLB can contain independently selectable renderable nodes. The [glTF scene and mesh specification](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html) defines those relationships.

The [latest NXP identity review](https://github.com/TokitoAI/tokito-app-dataset/blob/5283bc059e30e51f4d3a81a674425936fc213214/data/video-captures/2026-10-10/nxp-keyframes/identity-review.json) records a photographed B marking against available C source data. Exact source-to-specimen mapping is unverified. ESP12E's new 56 annotation instances have null GLB selectors/source-component IDs and do not establish a board frame. These observations block positional guidance on the photographed specimens until reviewed evidence resolves them.

## CLI flow

~~~text
tokito-cli twin inspect board.glb --component-map component-map.json --manifest digital-twin.json
tokito-cli twin prepare board.glb --component-map component-map.json --manifest digital-twin.json --output new-candidate-folder
tokito-cli twin select new-candidate-folder/active.digital-twin.glb --component-map new-candidate-folder/component-map.json --manifest new-candidate-folder/active.digital-twin.json --reference U11
tokito-cli twin select new-candidate-folder/active.digital-twin.glb --component-map new-candidate-folder/component-map.json --manifest new-candidate-folder/active.digital-twin.json --class diode_or_led
~~~

The exact authored class label is required. A class query can return multiple candidates; it does not choose a diode from an ambiguous diode/LED class. Exact reference selection returns a unique candidate only when that reference has geometry. Missing geometry and no match have distinct statuses.

Preparation outputs a new directory containing:

- active.digital-twin.glb: existing geometry with compatible renderable selector names.
- active.digital-twin.json: original metric frame/context claims, updated digest, length and content-derived asset ID.
- component-map.json: retained dataset labels/quality claims, updated asset ID.
- component-index.json: hashes of the prepared files, selector/renderable indexes, query labels, missing required models and unmapped renderables.
- prepare-report.json: source digests and explicit candidate/registration limitations.

No board-reference manifest is generated or copied. These files are an unregistered reference candidate, not an App-admitted package. Context revision remains a source claim. The CLI does not independently recover physical axes, compare decoded mesh bounds to manifest bounds, certify package fit, join model references to imported PCB UUIDs, or prove complete physical component coverage.

The dataset PR tree does not contain a reviewed board-reference manifest matching its declared NXP assembly-context digest. The prepared NXP asset also retains one unmapped renderable node (node 732, mesh 354); its engineering classification remains unverified. The CLI audit and E2E receipt expose it, rather than assuming every unprefixed mesh is an approved board shell.

Inputs are bounded at 64 MiB GLB, 4 MiB map and 64 KiB manifest; duplicate JSON keys, invalid framing, external resources, identity/hash mismatches, duplicate/unreachable selectors, cyclic/shared node trees and shared meshes are refused. The current preparation profile requires one mesh-bearing node per component subtree. Multi-mesh component merging is unsupported and refused.

Missing required model rows refuse preparation unless --allow-partial is explicit. Unmapped renderables are listed; the App keeps non-reference entities visible, so each listed item needs review to distinguish board geometry from an unlabelled component. Source package-quality labels are preserved as claims.

The output directory is created exclusively and files never overwrite existing files. All validation occurs before directory creation. An I/O failure after creation can leave an incomplete directory; no successful CLI report is returned. Use another new output path after resolving the failure. No files are automatically copied to a phone.

## Architecture and ownership

~~~mermaid
flowchart LR
    DATA[Dataset CAD + component map + manifest] --> AUDIT[Rust twin audit]
    NATIVE[Tokito CLI component GLB] --> AUDIT
    AUDIT --> PREP[Renderable selector preparation]
    PREP --> INDEX[Hash-bound component index]
    INDEX --> QUERY[Exact ref or authored class query]
    PREP --> CAND[Unregistered candidate GLB + manifest]
    CAND --> GATE[Reviewed board-reference identity + context + bounds]
    GATE --> BRIDGE[Future Desktop / mobile import bridge]
    BRIDGE --> APP[App registration and scene lifecycle]
    QUERY --> APP
    APP --> VIEW[Per-component colour / visibility / guidance]
~~~

The CLI owns asset preparation and diagnostics. Dataset owns sourced design identity, labels and provenance. App owns import admission, camera registration, dialogue selection, material changes and overlay lifecycle. A camera detector box is not a reference-designator mapping.

## Acceptance roadmap

| Task | Owner | State | Evidence / gate |
|---|---|---|---|
| Read updated PR assets and App selector contract | CLI evidence agents | Done | Pinned commits and observations above |
| Exact ref/class candidate query | CLI | Implemented | Portable subprocess E2E; ambiguity/missing-model states |
| Move selectors onto isolated renderable meshes | CLI | Implemented | E2E compares binary payload and transform values independently |
| Portable and actual NXP rewrite E2E | CLI QA | Implemented gate | Local verifier runs fixture journey and optional 354-component dataset journey |
| Current-source full verification | CLI QA | Release gate | 18 default gates, 20 with both dataset profiles; authoritative matching receipt in selected delivery base |
| Reviewed board-reference digest and source PCB crosswalk | CLI + dataset | Open | Exact board/revision/context, reviewed targets, source hashes and UUID/refdes join |
| Decoded geometry-bounds/reference comparisons | CLI + App | Open | App performs an additional bounds acceptance check; CLI currently reports it unverified |
| Desktop/mobile import bridge | App/Desktop | Open | Scoped sibling changes need user authorization; no bridge verified |
| Colour highlighting and clear-selection restore | App | Open | Per-entity material instances; avoid modifying shared materials; test selected and nonselected entities |
| Natural-language diode request | App dialogue | Open | Resolve exact component or expose multiple authored candidates; abstain on unknown mapping |
| Registered 2D/3D alignment and lifecycle | App/device QA | Open | Reviewed specimen revision, board frame, expiry, interruption, permissions and physical-device tests |

Current new CLI code is pure Rust and offline; Python is local test tooling. It prepares the dataset's existing CAD geometry and does not claim to have generated its original 3D component bodies. Original body/footprint generation remains the separate [Component Forge workstream](COMPONENT_FORGE_ARCHITECTURE.md).
