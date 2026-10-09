# Tokito CLI workspace rules

- Keep implementation and generated project files inside this `tokito_cli` folder unless the user explicitly authorizes another project change.
- Implement original Rust code with a pure-Rust dependency graph. Do not link native CAD libraries or shell out to KiCad.
- KiCad source may be used only as a behavior reference; do not copy or translate its code. Use the documented file format for independent parsers.
- Keep every capability truthful and evidence-backed. A parsed field is not proof that its full geometry was understood.
- Preserve offline operation, bounded inputs, explicit units/coordinate frames, and diagnostics for missing or unsupported geometry.
- Do not claim phone-package compatibility until the Tokito App has an explicit import contract and accepts the package.
