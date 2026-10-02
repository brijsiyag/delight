# Working on Delight

Delight is a macOS launcher whose tools are plugins: WASM components that run
their own GPUI through embedded_gpui. This repo is a from-scratch rewrite;
`docs/plan.md` is the plan and the order of work. Read it before starting.

## How work happens

- **Small, isolated steps.** Each step in `docs/plan.md` is one logically
  separate piece that can be reviewed on its own. Finish it, check it builds,
  then stop and hand it over for review.
- **Never commit without approval.** Commit only when the user has reviewed
  the change and says to commit, and only what was reviewed.
- **embedded_gpui changes are confirmed first.** Before writing anything in
  `../embedded_gpui`, describe the change (what, why, API, files) and wait for
  a yes. Keep each one small and upstreamable, one commit each.
- **Versions change only for a release.** Never change a version on your own:
  the app's, the built-ins', the plugin API's, or a plugin's in
  `../delight-plugins`, even when a change would call for a bump. When the user
  asks for a release, the release is the version bumps: make them and stop.
  The user runs `cargo xtask release` (and a plugin's publish) themselves.

## Where things are

The umbrella folder `~/Desktop/delight-umbrella` holds this repo next to its
forks:

- `../embedded_gpui`: the fork `github.com/brijsiyag/embedded_gpui`, branch
  `delight`, where changes to embedded_gpui are made and proposed upstream.
  Delight itself depends on upstream (`zed-industries/embedded_gpui`) at a
  commit (the root `Cargo.toml`); a local `[patch]` points it at this checkout
  while a change is being worked on.
- `~/Desktop/delight` (branch `feat/wasm-plugins`, uncommitted) is the
  previous attempt. Its UI was refined, so its UI and app features (the
  launcher, the input history, the UI kit, every visual) are copied from it
  and adapted to this architecture. Never reuse its folder or code
  structure, or its plugin architecture; what connects the copied UI to the
  plugins is written for this repo. Settings is the exception: it gets a
  new design. `~/Desktop/delight/docs/wasm-rewrite-notes.md` lists what
  that attempt learned.
- `~/Desktop/delight-plugins`: the third-party plugins, ported once the plugin
  API is ready (not a git repo; don't break it).

## Rules

- Host is generic: tools draw all of their own UI. Keep the app's UI minimal.
- Prefer popular crates over our own code.
- Pin every dependency exactly (`=x.y.z` or a git `rev`), declared once in
  the root `[workspace.dependencies]`. GPUI is the exception: it is named by
  branch exactly as embedded_gpui names it, and `Cargo.lock` pins the commit
  (see the plan's Layout).
- Don't run `cargo fmt`.
- A tool is self-contained: its code, assets and tests live in its folder.
  `delight-ui` holds only components more than one place uses.
- Test plugin behaviour headless (see the plan's test crate), never by
  sending synthetic keystrokes to the GUI: they go into whatever the user is
  typing in.
- GPUI builds take many GB. Check `df -h ~` before large builds.
