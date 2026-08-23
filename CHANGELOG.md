# Changelog

All notable changes to `git-nav` are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and the project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- New `AUTHOR` column in the picker table showing the author of each
  repo's last commit (`git log -1 --format=%an`). Fetched in parallel
  alongside status / branch / remote, so wall-clock is unchanged.
- Fuzzy filter now also matches against the author name — typing your
  own name narrows to repos you touched last.

## [0.2.1] - 2026-08-23

Release-workflow-only patch. The compiled crate is byte-identical to
0.2.0, so `cargo install git-nav` picks up no changes — the point of
this release is to attach prebuilt binaries built with the fixed
pipeline below.

### Fixed

- Release workflow now cross-compiles `x86_64-apple-darwin` from the
  ARM `macos-latest` runner instead of `macos-13`. GitHub's Intel macOS
  pool had multi-hour queues on public repos, which blocked the v0.2.0
  release from finishing for 90+ minutes.

### Changed

- Bumped GitHub Actions past the Node 20 deprecation:
  `actions/checkout` → v7, `actions/upload-artifact` → v7,
  `actions/download-artifact` → v8, `softprops/action-gh-release` → v3.

## [0.2.0] - 2026-08-12

First public release. Full rewrite of an earlier Python prototype.

### Added

- Inline `ratatui` TUI (fzf-style bottom-anchored viewport, sized to fit).
- Fuzzy filter across REPO / ORG / BRANCH — case-insensitive subsequence
  match (`bsv` matches `billing-svc`).
- Parallel repo discovery and per-repo `git status` via rayon (~275ms
  end-to-end for 50 repos on a modern laptop).
- First-run interactive prompt for the root directory; TOML config at
  `$XDG_CONFIG_HOME/git-nav/config.toml`. `--reset`, `--config-path`,
  `--root PATH`, and `--depth N` for overrides.
- Shell wrapper generation: `git-nav --init {bash,zsh,fish}` prints the
  `gnav` function; `git-nav --install-shell` writes it to your rc file
  (idempotent, marker-fenced). Informational flags (`--help`,
  `--version`, `--config-path`, `--init`) pass straight through the
  wrapper without triggering `cd`.
- Non-TTY fallback: plain-text table + numeric selection on stdin.
- CI workflow (rustfmt, clippy, cargo-audit, tests) and tag-triggered
  release workflow (macOS + Linux, x86_64 + aarch64) on GitHub Actions.
- Dependabot config for `cargo` and `github-actions`;
  [`SECURITY.md`](SECURITY.md) with the private advisory flow.
- Dual-licensed MIT OR Apache-2.0. Published to
  [crates.io](https://crates.io/crates/git-nav).

[Unreleased]: https://github.com/oskarizu/git-nav-rs/compare/v0.2.1...HEAD
[0.2.1]: https://github.com/oskarizu/git-nav-rs/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/oskarizu/git-nav-rs/releases/tag/v0.2.0
