# Tokito 3D presentation standard

The accepted Ultimainboard viewer is the visual reference for old and future
Tokito CLI inspection views. The built-in profile is
`tokito.ultimaker-studio.v1`; its JSON contract is
`tokito.viewer-presentation.v1`. `twin prototype` embeds that profile and also
writes `presentation-profile.json`. All boards use the same implementation in
`src/twin_viewer.rs`.

## Appearance and interaction

The default **Ultimaker reference** lighting retains the accepted viewer's
neutral background, directional light, ambient/diffuse balance and initial
camera. Source colours, vertex colours, opacity and component transforms are
retained. A separately selectable **Material studio** mode uses authored
roughness/metallic factors with approximate studio illumination. Neither mode
measures a real material's reflectance. Board substrates retain their actual
asset colours; the standard does not repaint all boards to match Ultimaker.

The reference GLB is pinned by SHA-256:
`b005375032a454d04fd9bc75071c37c93e1999bf2285f8a189af33db0737b712`.
This identifies the visual reference, not the source of another board's CAD.

Exact component selectors, search, independent selection outlines, orbit, zoom,
and reset apply across the profile. Highlighting leaves component surface
colours intact. Missing bodies and incomplete geometry stay explicit.
Presentation does not manufacture terminals, silkscreen, hidden components or
photo-derived dimensions.

## Actual 4K capture

**Save 4K PNG** renders the current view and selection at exactly 3840 × 2160
pixels, then restores the interactive canvas dimensions and camera. This is a
model image, without the sidebar. It is not an enlargement of a low-resolution
page screenshot. The runtime checks viewport limits and the actual allocated
drawing-buffer dimensions, refusing an unsupported capture rather than
claiming 4K success. Texture/renderbuffer limits separately bound preview
picking attachments, which are not allocated at 4K during capture.
PNG encoding failure is also explicit. Interactive views use bounded device
pixel density so small devices need not continuously render at 4K.

Device-limit and canvas-encoding behavior follows the [WebGL viewport
contract](https://developer.mozilla.org/en-US/docs/Web/API/WebGLRenderingContext/viewport)
and [canvas PNG encoding contract](https://developer.mozilla.org/en-US/docs/Web/API/HTMLCanvasElement/toDataURL).

## Geometry acceptance is a separate gate

Every future model needs source-linked identities, declared units and coordinate
frames, validated dimensions and placement/rotation/top-bottom transforms,
bounded surface/chord error, correct outward normals and valid solid topology.
Retain source contours, cutouts, terminals and individual mesh ownership.
Use [the geometry gates](KICAD_3D_QUALITY_GATES.md) and
[release gates](CLI_RELEASE_READINESS.md); a sharp render cannot clear them.

Missing models require exact part/package/revision evidence or a reviewed,
dimension-supported generated model. Incomplete exports require explicit
partial consent. Nominal envelopes remain candidates. STEP research candidates
that fail containment or normal checks remain inadmissible. Physical fit and
App/AR registration are separate claims.

## Existing designs and regression checks

Regenerate historical viewers from their pinned GLB, map and manifest using
the current `twin prototype` command, preserving any source-bound reference BOM
comparison. Verify GLB bytes, component identities and transforms before
replacing a viewer. Do not reinterpret an old screenshot as a new acceptance
result. Third-party/App/Desktop renderers require their own presentation
integration; this CLI change does not modify those repositories.

`tests/presentation_browser.js` exercises the real default lighting, selection,
4K PNG dimensions and download, interactive-state restoration, both lighting
modes, missing bodies and offline operation. Rust/CLI E2E checks cover profile
emission and source/no-overwrite gates. The six packaged demos are the current
migration targets, including both baseline and refined variants.

The six-board migration passed on Microsoft Edge 155.0.4283.45. The [browser
receipt](evidence/ultimaker-presentation-browser-20261016.json) binds tested HTML
and downloaded PNG hashes, dimensions, visible-content checks and test-source
hash. The [migration receipt](../demo/presentation-standard-verification.json)
records unchanged GLB/map/manifest inputs and prior component fields. ESP8266
has no absent-model row in this source; missing-model UI behavior was exercised
on the other five viewers. PNG exports keep source limitations unchanged.
The tested optimized release CLI reproduced all six HTML,
profile, index, GLB, map and manifest outputs byte-for-byte. The migration
receipt records the exact retained debug builder and release reproducer;
an unrelated `target/debug` cache is not their binary identity.

Local QA can use an explicitly supplied installed Playwright module:

```powershell
node tests/run_presentation_browser.cjs <installed-playwright-module> <prototype-root> <new-QA-directory> [--headed]
```

Pass `--headed` to exercise the same assertions in visible Windows Edge. The
receipt records `browserMode` and the runner source hash. The test searches for
a component, selects it from the UI list, verifies the visible outline and
exact identity, and checks missing-model disclosure and offline selection.
Headless remains the default for repeatable local CI-style runs.

The runner launches a real local Edge browser sequentially per board; it does
not install browser dependencies or execute hosted Actions. In this run the
Playwright CLI daemon failed under disk/memory pressure, so its installed
Playwright library drove the same browser assertions directly. An initial
parallel optimized-build attempt also exhausted memory; serial compilation
was used afterward. These attempts are failures, not passing test evidence.
