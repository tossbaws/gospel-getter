---
name: rust-coding-standards
description: >
  REQUIRED whenever writing, reviewing, refactoring, or reviewing a plan for Rust code
  (any .rs file, Cargo.toml, or `cargo`/`rustc`/`clippy`/`rustfmt` invocation). Encodes
  current-edition Rust syntax, official API design guidelines, clippy/rustfmt conventions,
  error-handling and async idioms, and a pre-completion checklist. Triggers: Rust, cargo,
  clippy, rustfmt, .rs, crate, trait, lifetime, borrow checker, async/await, tokio, serde,
  thiserror, anyhow, unsafe, edition 2024, RPITIT, macro_rules, proc-macro.
---

# Rust Coding Standards

Baseline: **Rust 1.98.x, 2024 edition** (stabilized in 1.85, Feb 2025). Verify the
project's actual edition/MSRV in `Cargo.toml` before assuming 2024 — don't
backport 2024-only syntax (like unsafe-extern-block requirements) onto an
older edition.

Treat this skill as always-on for Rust work, not something to consult only
when stuck. Apply it while writing code, not as a post-hoc cleanup pass.

## 1. Non-negotiables before calling a task done

Run, in this order, and fix everything they report:

```
cargo fmt
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo doc --no-deps
```

If a repo's `.claude/settings.json` or CLAUDE.md gives stricter/different
commands, defer to those — this is the fallback, not an override.

## 2. Formatting (rustfmt defaults — don't hand-format against these)

- 4-space indent, spaces only, 100-column max line width.
- **Block indent** for wrapped call/struct-literal args, not visual indent:
  ```rust
  a_function_call(
      foo,
      bar,
  );
  ```
- Trailing comma on any comma-list that wraps to multiple lines.
- One `derive(...)` attribute per item, all traits combined into it, order
  preserved: `#[derive(Debug, Clone, PartialEq, Serialize)]`.
- `///` outer doc comments on items, `//!` only for module/crate-level docs,
  placed before any attributes.
- Line comments (`//`) over block comments; complete sentences, capitalized,
  punctuated, when they explain non-obvious *why*.
- Never hand-align or fight rustfmt — if output looks wrong, fix the code
  shape, not the formatter.

## 3. Clippy — know what each group means before silencing it

- `clippy::correctness` — deny-by-default, code is outright wrong. Never
  `#[allow]` these; fix the bug.
- `clippy::suspicious` — probably wrong; needs a deliberate reason to allow.
- `clippy::style`, `clippy::complexity`, `clippy::perf` — on by default at
  warn; treat warnings as build failures (`-D warnings`).
- `clippy::pedantic` / `clippy::nursery` — opt-in, cherry-pick lints that fit
  the project rather than enabling the whole group.
- `clippy::restriction` — never enable wholesale; individual lints from it
  can contradict each other and forbid perfectly good code.
- Any `#[allow(clippy::...)]` needs a one-line comment saying *why* the lint
  doesn't apply here, at the point of use — not a blanket crate-level allow.

## 4. Edition 2024 specifics (don't silently write pre-2024 habits)

- `extern` blocks require `unsafe extern { ... }`.
- `#[no_mangle]`, `#[export_name]`, `#[link_section]` must be written as
  `#[unsafe(no_mangle)]` etc.
- `unsafe_op_in_unsafe_fn` now warns by default — an `unsafe fn` body still
  needs explicit `unsafe { }` around unsafe operations; the signature alone
  no longer grants a free pass.
- Tail-expression temporary scoping changed — don't rely on old drop-order
  assumptions for the last expression in a block.
- Prefer file-per-module (`foo.rs` next to a `foo/` directory of
  submodules) over `foo/mod.rs` — both still work, but the flat form is now
  idiomatic and is what `cargo new`/rust-analyzer scaffold.
- Async closures (`async || {}`) and RPITIT (return-position `impl Trait` in
  traits) are stable — prefer them over manual boxed-future workarounds
  (`Pin<Box<dyn Future...>>`) unless object-safety across an `dyn Trait`
  boundary forces boxing.

## 5. API design (Rust API Guidelines — condensed; see
   https://rust-lang.github.io/api-guidelines/checklist.html for the full list)

- **Naming**: `as_`/`to_`/`into_` for conversions matching borrow→borrow,
  borrow→owned, owned→owned respectively; getters are bare `name()`, not
  `get_name()`; iterator methods are `iter`/`iter_mut`/`into_iter`.
- **Common traits**: derive `Debug, Clone, PartialEq, Eq, Hash, Default` on
  public types whenever the fields allow it — don't make callers wonder why
  a plain data type can't be compared or printed.
- **Conversions**: implement `From`/`TryFrom` instead of ad-hoc `new_from_x`
  constructors when the conversion is lossless/total or fallible respectively.
- **Errors**: error types implement `std::error::Error` + `Debug` + `Display`,
  are `Send + Sync + 'static` so they compose under `anyhow`/`?`, and carry
  enough context to act on without re-deriving it from a string.
- **Builders** for types with many optional/defaultable fields rather than
  N overloaded constructors or a giant positional-args function.
- **Newtypes** (`struct UserId(u64)`) instead of bare primitives for
  domain-meaningful values — catches mixing up two `u64`s at compile time.
- **Sealed traits** for public traits not meant to be implemented outside
  the crate.
- Public structs keep fields private with accessor methods unless the type
  is a plain data aggregate the caller is meant to construct by literal.

## 6. Error handling

- **Libraries/domain crates**: `thiserror` for typed, matchable error enums.
  Never `unwrap()`/`expect()`/`panic!()` on inputs or I/O — only on
  invariants the type system already guarantees (and say so in a comment
  when it's not obvious).
- **Binaries/application top-level**: `anyhow::Result` at the boundary is
  fine; use `.context("...")` to attach what was being attempted, not what
  the error already says.
- Propagate with `?`; don't match-and-rewrap just to add a `println!`.
- Don't use `Result<T, String>` or `Box<dyn Error>` in new code when a
  concrete `thiserror` enum is feasible — string errors aren't matchable.

## 7. Ownership, borrowing, generics

- Take `&str`/`&[T]` parameters over `&String`/`&Vec<T>`; take owned values
  only when the function actually needs to consume or store them.
- Reach for `Rc<RefCell<_>>`/`Arc<Mutex<_>>` only when shared mutable state
  is genuinely required (e.g. across `tokio::spawn` boundaries) — not as a
  default way to dodge the borrow checker.
- Prefer generic `impl Trait` / `<T: Trait>` parameters over trait objects
  in hot paths; use `dyn Trait` when you need a heterogeneous collection or
  to keep a public API's monomorphization cost down.
- Elide lifetimes wherever the compiler allows; only name them when two or
  more borrows need to be tied together explicitly.

## 8. Async (tokio/axum-style code)

- Never call blocking I/O or CPU-heavy work directly inside an `async fn`;
  wrap it in `tokio::task::spawn_blocking`.
- Hold `std::sync::Mutex` guards only across non-`.await` code; use
  `tokio::sync::Mutex` (or restructure to avoid holding a lock across an
  await point at all) otherwise.
- Structure long-running background work (schedulers, pollers) so errors
  are logged and the loop continues, rather than the task silently dying —
  match on the tick result instead of `?`-ing it out of the loop.
- Prefer `tokio::select!` / structured cancellation over manually tracking
  `AbortHandle`s unless you specifically need detached fire-and-forget.

## 9. Documentation & testing

- Every `pub` item gets a `///` doc comment; if the crate is a library,
  include a runnable example using `?` (not `unwrap()`) where practical.
- Unit tests live in `#[cfg(test)] mod tests` beside the code;
  cross-module/integration tests go in `tests/`.
- Async tests use `#[tokio::test]`; don't hand-roll a runtime in test code.

## 10. Dependency hygiene

- Pin dependency versions the same way the rest of the workspace does
  (check existing `Cargo.toml` conventions before adding a new crate).
- Enable only the feature flags actually used (e.g. don't pull in
  `tokio = { features = ["full"] }` in a library crate just because the
  binary needs it).
- Prefer well-maintained, widely-used crates already in the dependency tree
  over adding a new crate that duplicates functionality.

## Sources

- https://doc.rust-lang.org/edition-guide/rust-2024/index.html
- https://rust-lang.github.io/api-guidelines/checklist.html
- https://doc.rust-lang.org/style-guide/
- https://doc.rust-lang.org/clippy/
- https://blog.rust-lang.org/2025/02/20/Rust-1.85.0/

This file reflects Rust as of ~1.98 (Sept 2026). Re-check the edition guide
and API guidelines links above if a future edition ships and this starts
looking stale.
