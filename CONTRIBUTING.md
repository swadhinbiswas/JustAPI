# Contributing to JustAPI

Welcome! This guide outlines the development workflow, coding standards, and repository conventions for anyone — human or AI — contributing to the JustAPI runtime.

---

## Technical & Architectural Guidelines

Before making any changes, please read [ARCHITECTURE.md](ARCHITECTURE.md) to understand the crate structure, FFI boundaries, GIL concurrency model, and subsystems.

---

## Repository Conventions

### Commit Messages
We follow a structured commit format that links changes to their roadmap phase:
```
<phase>: <short description>
```
*   *Example:* `p3: add per-request arena allocator`
*   *Rule:* Exactly one logical change per commit. Avoid grouping unrelated updates.

### Branch Naming
Create feature branches using this prefix style:
```
phase-N/<short-description>
```
*   *Example:* `phase-42/paged-kv-cache`

---

## Code Quality Standards

### Memory Safety & `unsafe`
To maintain the performance boundary without introducing memory safety hazards:
*   Every `unsafe` block **MUST** be accompanied by a `// SAFETY:` comment explaining the specific invariant that makes the block sound.
*   Whenever feasible, add a test that would trigger Miri or cause a panic if that invariant were violated.

### Dependency Management
*   Do not add new dependencies lightly.
*   Every new dependency **MUST** be justified in [DECISIONS.md](DECISIONS.md) detailing what it replaces or enables, and why alternative approaches (like writing the logic yourself) are inferior.

### Public API Changes
*   Any changes to public Rust APIs or public Python functions/decorators **MUST** include complete documentation (rustdoc or Python docstrings) *before* the pull request is marked as ready for review.

### Error Handling
*   **Application Code (CLI, Benchmarks):** Use `anyhow::Result` for generic context-wrapping errors.
*   **Library Crates (`justapi-core`, `justapi-py`):** Use typed errors (`thiserror`) for any error crossing crate or FFI boundaries to guarantee type safety and clear error messages.

---

## Pre-Submission Checklist

Before submitting a Pull Request, verify that all of the following steps pass:

1.  **Run Workspace Tests:**
    ```bash
    cargo test --workspace
    ```
2.  **Lint Check:**
    ```bash
    cargo clippy --workspace --tests -- -D warnings
    ```
3.  **Formatting Check:**
    ```bash
    cargo fmt --check
    ```
4.  **Miri Validation (for core changes):**
    ```bash
    cargo miri test -p justapi-core
    ```
5.  **Run Benchmarks:**
    Run the performance suite and append the new metrics to [BENCHMARKS.md](BENCHMARKS.md). Confirm there is no `p99` latency regression >5% compared to the baseline without a corresponding entry in [DECISIONS.md](DECISIONS.md).
6.  **Roadmap Update:**
    Update the roadmap in [PLAN.md](PLAN.md) to reflect the status of active or completed phases.
7.  **Pattern/Skill Documentation:**
    If you discover a recurring pattern or gotcha, document it by updating or creating a `SKILL.md` under the `skills/` directory.

---

## Versioning & Deprecation Policy

### Semantic Versioning

JustAPI follows [Semantic Versioning 2.0.0](https://semver.org/spec/v2.0.0.html):

- **Major (X.0.0):** Breaking changes to public API. Requires migration guide.
- **Minor (0.X.0):** New features, backward-compatible. May deprecate existing APIs.
- **Patch (0.0.X):** Bug fixes, backward-compatible. No API changes.

**Current status:** Pre-1.0. Minor versions may contain breaking changes (documented in CHANGELOG.md with migration notes).

### Deprecation Process

When a public API is deprecated:

1. Add `#[deprecated(since = "X.Y.Z", note = "use new_api() instead")]` (Rust) or `warnings.warn(...)` (Python)
2. Document the deprecation in CHANGELOG.md under `### Deprecated`
3. Provide a migration example in the deprecation note
4. Remove the deprecated API in the next **minor** version (not patch)

### Breaking Change Requirements

Before merging a breaking change:

- [ ] CHANGELOG.md updated with `### Changed` or `### Removed` entry
- [ ] Migration guide written (new section in docs or inline in CHANGELOG)
- [ ] Deprecation period of at least one minor version (unless critical security fix)
- [ ] All downstream examples and tests updated

---

## Resuming Work after Context Reset
If you are an agent resuming work after a context reset:
1.  Read [PLAN.md](PLAN.md) to locate the current phase and its status.
2.  Read the most recent entries in [DECISIONS.md](DECISIONS.md) to understand why recent architecture decisions were made.
3.  Skim any relevant `SKILL.md` files in the area of the codebase you are modifying.
4.  Check the last recorded numbers in [BENCHMARKS.md](BENCHMARKS.md) to know the performance target.
5.  Continue directly from where the roadmap dictates.
