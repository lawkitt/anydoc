# Fork CI (lawkitt)

This fork exists for mdoc, which pins it by git rev and builds it on macOS,
Windows, iOS and Android. It does not publish packages or deploy upstream's
site. It follows the same rules as lawkitt/pdf-inspector's `docs/fork-ci.md`.

## Decisions

1. **Keep the strict Actions policy.** The repository allows GitHub-owned
   actions plus `Swatinem/rust-cache`, `softprops/action-gh-release` and
   `taiki-e/install-action`, every one pinned by full SHA. Workflows install
   Rust with `rustup toolchain install`, which reads `rust-toolchain.toml`, so
   the toolchain version lives in that one file, the same version mdoc uses
   (upstream's `dtolnay/rust-toolchain` is not allowed, and its Blacksmith
   runners exist only in upstream's account).
2. **One fork-owned `ci.yml`; no release or pages workflows.** Upstream's
   `ci.yml` is replaced in place, and `release.yml` and `pages.yml` are
   deleted, so this fork can never publish under upstream's package names
   (crates.io, npm, PyPI) or deploy its site.
3. **Jobs, all on standard GitHub-hosted runners** (free for a public repo),
   each with a `timeout-minutes`:
   - `Lint` (Linux): version sync (`scripts/check-versions.sh`),
     `cargo fmt --all --check`, and
     `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
   - `Test (ubuntu-latest | macos-latest | windows-latest)`:
     `cargo test --locked` and `cargo test --locked --no-default-features`
     (mdoc builds AnyDoc without its default `pdf` feature).
   - `Node bindings`, `Wasm bindings`, `Python bindings` (Linux): upstream's
     binding jobs, kept because they run on hosted runners and catch a core
     change that breaks a binding crate. mdoc does not use the bindings.
   - Left out: upstream's release matrix (Node addons, wheels, crate and npm
     publishing).
4. **Required checks.** After this CI is on `master` with a green run, the
   maintainer may add a ruleset requiring the jobs above. Repository settings
   are the maintainer's; agents do not change them.
5. **PRs wait for green CI**, including PRs opened before this CI existed
   (rebase them on `master` to get a run).

## When porting from upstream

This fork is a squashed snapshot of upstream and takes cherry-picks only.

- A cherry-pick that touches `.github/workflows/ci.yml` conflicts: keep the
  fork's file, and port any new check that applies to mdoc's use (Rust
  library, macOS/Windows) without new third-party actions.
- Upstream changes to `release.yml` or `pages.yml` are dropped.
- A toolchain bump in `rust-toolchain.toml` goes with mdoc's: fix any new
  clippy lints in the same commit.
