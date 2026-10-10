# Ultimainboard selectable source prototype

This inspection prototype uses the dataset PR #13 source candidate pinned at `aa9db20f574c4ba0273a5f5a03460f5cafb39cdb`. It preserves imported component surfaces, board substrate, pads, soldermask, silkscreen and source placements. It does not certify the photographed population or AR registration.

## Flow

```text
tokito-cli twin adapt-source source.glb --component-map source-map.json --geometry-index geometry-index.json --provenance provenance.json --coverage geometry-coverage.json --board-id ultimainboard --board-revision 2.1.4 --output new-adapted-folder
tokito-cli twin prototype new-adapted-folder/active.digital-twin.glb --component-map new-adapted-folder/component-map.json --manifest new-adapted-folder/active.digital-twin.json --output new-demo-folder --allow-partial
```

Open `new-demo-folder/index.html` in Edge or Chrome. Search an exact reference, select an IC or capacitor, rotate and zoom, and clear the selection. One component can own multiple mesh nodes; highlighting changes every part of that logical component together. The source GLB binary payload, materials and transforms remain unchanged in the output asset.

The adapter verifies provenance hashes/sizes for the source GLB, map and geometry index; validates every component subtree's actual node and mesh ownership; preserves source component identities; and derives bounds from actual decoded vertices and original JSON transforms. It retains the source's metric/Y-up frame as a declared profile claim. It does not invent a measured board plane or source-to-camera transform.

## Coverage and gaps

- 574 source references; 310 selectable bodies; 94 bodies contain several mesh nodes.
- 433 component-owned mesh nodes; six board/non-component mesh nodes remain explicit in the index.
- Missing package bodies: J34, U9, K1 and L2.
- Four mechanical spacers have unresolved fitted population/body status and conservatively require partial consent too.
- 252 test contacts, three fiducials and one solder jumper have source classification claims but no independently selectable contact geometry. No fabricated IC bodies are substituted for them.
- Component classes remain unspecified where the source map does not author them. The adapter does not infer an MPN, pin map or reviewed photo label from a reference prefix.

The coverage file links to the source index digest, but is not itself in the supplied provenance file inventory. Its current digest is reported and its classifications remain source claims, with `coverageOriginAuthenticated: false`. Git-pinned intake evidence is separate from the adapter's supplied-file checks.

Single-mesh selector naming stays compatible with the earlier NXP preparation. Multi-mesh roots retain one logical selector; the current App's direct mesh-name filter is not certified compatible with these groups. This is a desktop/browser demo, not a claim of phone import acceptance.

## Geometry and presentation checks

Independent decoding of the pinned source produced 712,527 triangles, 125 line segments, 811,884 positions and 841 render batches. Scene bounds in declared metres are approximately `[-0.0995835015, -0.0027213980, -0.0395000018]` to `[0.0874999985, 0.0166999997, 0.0395000018]`. Actual CLI comparisons preserve tighter unrounded reference values; this is a numerical source comparison, not physical metrology.

The offline viewer renders static source triangles, lines and points, preserves source base colours and BLEND opacity, and supports per-component GPU picking/list selection. Transparent groups use depth sorting; intersecting transparent surfaces and full metallic/roughness PBR remain limitations. A locally bundled Three.js renderer is a later presentation improvement, separate from component geometry qualification.

Current-source acceptance requires local Rust checks, real subprocess adaptation/preparation/refusal journeys, both dataset prototype browser journeys and independent review before a PR push. GitHub Actions remain disabled. See [local output locations](LOCAL_OUTPUT_LOCATIONS.md) and [first prototype QA](SELECTABLE_PROTOTYPE_QA.md).
