# Decisions

The standing decisions a change to Gospel Getter has to respect: what was
decided, why, and what enforces it. One file per decision, numbered in the
order they were recorded. A decision that changes gets a new record that
says which one it replaces, and the old one's status says "replaced by".
The "D" numbers in some records are the maintainer's own decision log's.

- [0001](0001-scripture-text-is-never-altered.md): scripture text is never altered.
- [0002](0002-the-ai-rule.md): agents find and quote scripture, never interpret it; findings are the only agent write.
- [0003](0003-leptos-hybrid.md): the UI is a Leptos and hand-built DOM hybrid; watch the wasm size (D23).
- [0004](0004-release-cadence-and-rollback.md): a version ships only when the app or CLI changes; GitHub keeps two releases (D21).
- [0005](0005-qa-issues.md): one QA issue per release: the standing four plus one check per change (D22).
- [0006](0006-window-title.md): the window title is always "Gospel Getter" (D24).
- [0007](0007-schema-evolution.md): schema steps, compatibility both ways, and Rust owning the limits.
- [0008](0008-errors.md): `anyhow` everywhere, typed only at the retry seam.
- [0009](0009-ui-logic-is-rust.md): the page's logic is Rust; `ui/index.html` keeps three small inline scripts.
