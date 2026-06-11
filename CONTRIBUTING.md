# Contributing to Torch

Thanks for lighting a torch. A few ground rules keep the project healthy.

## Setup

- Rust stable, Node 20+, and a logged-in `claude` CLI (only needed for live
  runs — the entire test suite runs against stub binaries).
- `cargo test --workspace` — engine + shell tests.
- `npm install --prefix ui && npm run build --prefix ui` — frontend.
- `cargo tauri dev` (from `crates/torch-app`, with `npm i -g @tauri-apps/cli`
  or `cargo install tauri-cli`) — the desktop app against the Vite dev server.
- Opening `ui` in a plain browser runs **demo mode**: a scripted fake engine,
  useful for UI work without burning tokens.

## Before you open a PR

- `cargo fmt --all` and `cargo clippy --workspace --all-targets -- -D warnings`
  must be clean; CI enforces both.
- New engine behavior needs tests. The pattern in `crates/torch-core/tests/`
  is stub `claude` binaries that replay or fabricate stream-json — tests must
  never require a Claude login or network access.
- Keep `torch-core` UI-agnostic: no Tauri or frontend types in the engine.

## Design contributions (themes, components)

The design system is a token layer — read `ui/src/styles/tokens.css` and the
brand rules in the README before proposing changes. Hard constraints:

- The flame palette (#E8A33D → #F2C063 → #FBE9C0) is identical in every
  theme and never appears on buttons (except the Run action), charts, or
  body text. Amber means "work is happening here", nothing else.
- Red appears only via the guttered torch state and failure text.
- New themes are pure token swaps: add a `.theme-<name>` block defining the
  full custom-property set; zero layout changes.
- All animation must respect `prefers-reduced-motion`.

## License

By contributing you agree your work is dual-licensed under Apache-2.0 and MIT.
