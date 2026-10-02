# What Delight does

The behaviour the rewrite reproduces, taken from the previous attempt
(`~/Desktop/delight`, branch `feat/wasm-plugins`; paths below are relative to
its `crates/`). It records *what* happens and the rules behind it, not how
that code was organised. Where the rewrite changes something on purpose,
`docs/plan.md` says so.

## Launcher window

- Centred horizontally, 22% down the primary display (fallback screen
  1440×900).
- Sizes: empty bar 640×56, a pill (radius 28); with input a panel 800×540,
  radius 24; ⌃R history search keeps the width at height 540.
- GPUI `WindowKind::PopUp`, no titlebar, transparent background, movable, not
  resizable or minimisable.
- Restyled to AppKit `Borderless` after opening (GPUI makes a titled window,
  and macOS would draw its own frame). After the restyle GPUI's view must be
  made first responder again, or every key press beeps.
- Backdrop is a native view under GPUI's content: `NSGlassEffectView` on
  macOS 26+, else `NSVisualEffectView` (material Popover, BehindWindow,
  Active) with a 9-slice rounded mask. Custom because GPUI's blur keeps
  macOS's corner radius.
- Corners: content view and glass layer get `cornerRadius`, `masksToBounds`,
  `cornerCurve = continuous`, then `invalidateShadow`.
- `hidesOnDeactivate = false`, non-opaque, system shadow,
  `movableByWindowBackground = false` (so text can be selected). Only the
  logo and the footer drag the window (`performWindowDragWithEvent`);
  footer buttons stop mouse-down propagation.
- Painted: a mostly opaque tint (light `gray(.97, .86)`, dark
  `gray(.13, .88)`) so contrast doesn't depend on what's behind; a 1px
  border, on Liquid Glass a white rim at α .2 (dark) / .6 (light).
- Resizing keeps the top edge fixed and the window centred, animated except
  the first sizing, skipped under 0.5pt, and always deferred (`defer_in`):
  resizing mid-frame makes GPUI miss it. The radius switches pill ↔ 24 with
  the height.
- No Dock icon or app menu (`Accessory` policy, `LSUIElement`).
- `NSApp.appearance` follows the Appearance setting so the native parts
  match.

### Show and hide

- Global hotkey, default ⌘⇧Space, configurable. Each press toggles: visible
  and active → hide, else show (also while it fades away).
- It comes and goes as Spotlight does on macOS 26 (measured frame by frame
  from a 57 fps recording): coming, it fades in over 140 ms (ease in-out)
  while it narrows from 4% wider than it rests to 0.5% narrower at 158 ms
  (ease out), then eases back to its width by about 300 ms; its height takes
  no part (Spotlight's own swing, 10% wider to 1.3% narrower, was too much). Going, it fades out over 120 ms while it grows 8% all round, then
  goes off screen, and its tool's view goes after that. Spotlight also blurs
  as it goes, which only a private filter does. Reduce Motion leaves only the
  fades. A launcher already up (another app has the keyboard) only takes the
  keyboard back. A resize while it comes or goes starts from where it rests.
- Show: size first; present outside any GPUI update (AppKit calls back into
  GPUI synchronously): remember the frontmost app if it isn't Delight,
  activate Delight, `orderFrontRegardless` + `makeKeyWindow`; only then focus
  the input (earlier focus doesn't stick); optionally auto-paste; select all
  so typing replaces the input. The app activates like Raycast/Alfred rather
  than as a non-activating panel, which would leave modifier-only keys going
  to the previous app.
- Hide: `orderOut` once it has faded (never close; state survives), close ⌃R
  search. Re-activate the previous app only if the launcher had the keyboard
  (not when another app or Settings took focus). Save the input for restore if
  history is on.
- Esc hides (deferred). Losing activation while visible hides if "Hide when
  focus is lost" is on (default on) and the footer's pin isn't.
- With that off, a click on the launcher while another app has the keyboard
  presents it again, as the hotkey does (so Esc and typing reach it): macOS
  doesn't, as GPUI makes it a non-activating panel and AppKit keeps that for
  clicks after the borderless restyle.
- Opening Settings (⌘,, the menu bar, a plugin's `open_settings`) hides the
  launcher first, fading as any hide does but without going back to the
  previous app, and then opens or brings forward the settings window, so it
  never comes up under the launcher.
- At launch the window is made and styled but not shown: the hotkey or "Open Delight" in the menu bar shows it.
- **Plugin windows** (`host(cx).open_window`): a normal, resizable window with a plugin's view in it,
  four at most from one plugin; asking for the key of one that is open brings it forward. ⌘W
  and ✕ close it (Esc is the plugin's, since 2026-10-01). The launcher and the plugins' windows
  are one group for focus: while the keyboard is in any of them nothing hides; when it goes to
  something else (another app, Settings) and "Hide when focus is lost" is on, the launcher goes,
  and so do the windows that hide with it (off screen, not closed); the hotkey brings back the
  launcher and every window that hid, where they were. Esc or the hotkey on the launcher hides
  those windows with it. A window hides with the launcher unless the plugin opened it with
  `WindowOptions::hide_with_launcher(false)`: that one stays up until its user closes it. Only
  Delight takes a plugin's window off screen: the plugin can bring one back without the launcher
  (`host(cx).show_window(key)`) and close it (`close_window(key)`, as ✕ does), never hide it.
  Reloading the plugins closes the windows of the ones that start again.
- **Logs**: everything the app logs goes to the console and to a file in `~/Library/Logs/Delight`, one
  `delight-YYYYMMDD-HHMMSS.log` for each run (a new one when a run's file passes 8 MiB), the last six
  kept. The menu bar's "Open Logs": with one file it opens in the Mac's text editor (the app that
  opens a `.txt` file); with several it zips them as `Delight-logs-….zip` in Downloads and shows the
  zip in Finder; with none it opens the folder.
  What a plugin logs (its `log` lines, which it sends to its stderr) is logged by the app as
  `plugin::<name>` at the level the plugin gave, so it is in the console and the files with the rest.
- **GPUI deadlock (fixed upstream)**: a window becoming key while the app
  is inactive makes AppKit report `isKeyWindow == NO` inside
  `windowDidBecomeKey:`, and GPUI then calls `resignKeyWindow`. crates.io
  GPUI 0.2.2 did that holding its window-state lock, so the notification
  re-entered and deadlocked the main thread; the previous attempt swizzled
  `windowDidBecomeKey:` on `GPUIPanel`/`GPUIWindow` to avoid it, and its
  Settings activated the app before opening its window. The GPUI the
  rewrite uses releases the lock first, and showing the launcher from the
  hotkey was tested without the workaround, so the rewrite has none.

### Single instance, tray, login

- `flock` on `Application Support/Delight/delight.lock` holding the pid; a
  second instance exits with "Delight is already running (process N)". A
  restarted instance (`--restarted`) polls for the lock every 50 ms for up to
  10 s.
- Tray icon (`tray-icon`): the logo SVG rasterised to 36×36 as a template
  image, tooltip "Delight". Menu: "Open Delight" (the hotkey as accelerator,
  updated when it changes) · separator · "Settings…" · "Check for Updates…"
  (disabled until Sparkle loads; "Update Available — X.Y.Z…" when found) ·
  "Restart Delight" · separator · "Quit Delight". Clicks reach the main
  thread through a channel.
- Open at login: a per-user LaunchAgent `dev.delight.app` (ProgramArguments
  = current exe, RunAtLoad, ProcessType Interactive, LimitLoadToSessionType
  Aqua), rewritten at every launch only when it differs (follows a moved
  app). Chosen over `SMAppService` because it works whatever the signature.

## Input

- A multiline text editor, up to 4 lines (84px) then scrolls; bundled Lilex
  13px, line height 21. The placeholder is a tip, a different one at random
  each time the launcher shows: Delight's own about its keys (⌃R, Tab
  completions, ⌘K, ⌘,, ⌘1–⌘9, the launcher shortcut; the history's only while
  it's on), and the plugins' own, up to 5 each in their manifests
  (`#[plugin(tips = […])]`, at most 60 characters each), while the plugin has a
  tool on. A plugin's tips say what to type and what it gives ("Paste JSON to
  format it"), not its keys, which the footer shows; one or two each. (The previous attempt said "What you got this time?".)
- Layout: padding 20, 22px logo centred on the first line, gap 16; when
  expanded a ⓧ clear button on the first line; ⌃R replaces logo and input
  with the History icon and a search field.
- ↵ never inserts a newline (typed `\n`/`\r` are dropped unless an IME is
  composing), so ↵ is free for footer actions. ⇧↵ / ⌥↵ insert a newline
  keeping indentation. Pasted `\r\n` becomes `\n`.
- On change: if text is blank and no files, forget the picked tool; reset
  and recompute the ghost completion; re-detect after 30 ms (a newer change
  cancels the pending one).
- ⌘K or ⓧ clears text and files and refocuses the input. ⌘, opens Settings.
- **Pasted files**: ⌘V is captured before the editor. If the input is
  focused and the pasteboard has `public.file-url` items (Finder copy), their
  paths are appended, deduped; the text is unchanged. Otherwise the editor
  pastes text. Files show as 26px tags under the text (folder/file icon, name
  truncated at 220px, full path tooltip, ✕). Backspace in an empty input
  removes the last file. Files alone expand the panel. Sizes come from the
  host's `fs::metadata`.
- **Auto-paste on open** (off by default): replace the input with the
  clipboard text only if it differs from the last auto-pasted text, is
  non-blank and ≤ 1 MiB.

## Detection and the tool list

- A plugin has one or more **operations** (tools). `detect(input)` returns
  `(operation id, confidence)` pairs; confidence is clamped to 0…1.
- Blank text and no files: no candidates, no plugin calls. Otherwise every
  enabled, running plugin's `detect` is called concurrently. An error or trap
  marks that plugin stopped (first reason kept) and it contributes nothing.
- **Ranking** (`core/src/classifier.rs`): plugins in registry order
  (built-ins by id, then installed files by path; an installed plugin
  replacing a built-in moves to the end). Keep detections with confidence >
  0 whose operation id is in the manifest (unknown ids logged and dropped);
  per (plugin, operation) keep the best. Sort by confidence descending, ties
  by (plugin index, operation index).
- **Recommended** = confidence ≥ 0.5. No host-side rules.
- **Selection**: a preferred tool (set by accepting a completion or a ⌃R
  pick) selects that exact candidate, else that plugin's first candidate.
  Otherwise the picked tool stays selected while listed. Otherwise the first
  candidate if recommended. Otherwise nothing is selected.
- List: 200px wide. Caption "Recommended", or "No recommendations for this
  input"; before the first non-recommended candidate a divider and "Other
  Matches". A row: logo badge 20, title (truncated), keycap for its ⌘1–⌘9
  shortcut (looked up from the keymap at render time, so only bound ones
  show). Selected row: accent bg + accent text while the list has focus,
  `fill_strong` otherwise; hover bg on others. Click selects and focuses the
  list. A printable key (no ⌘/⌃) in the list moves focus to the input and
  types it. ↑ on the first row goes back to the input.

## Tool pane

- Header: logo badge 20, title (semibold), plugin name in tertiary colour if
  it differs from the title, ⚙ if the plugin has settings (opens its page).
  In the rewrite the plugin name is a link to its page (accent on hover,
  tooltip "Open its settings").
  In the rewrite, when the plugin has an update (plan step 16), at the right a
  small text button "↻ Update to X" ("Review Update…" when it asks for new
  permissions, "Retry Update" after a failure), which does what its page's
  button does; "Downloading… 1.2/3.4 MB" while it comes. A look for one that
  failed shows nothing here (it would read as the tool's own error); the
  plugin's page says so.
- Empty states (Sparkles icon): no candidates → "No tool fits this input" /
  "Plugins in the plugins folder add tools."; candidates but none selected →
  "No strong match" / "Pick a tool on the left, or keep typing."
- A stopped plugin: "{name} stopped: {reason}\n\nIt's off until Delight
  restarts (menu bar → Restart Delight)."
- A pane is made on first selection, one per (plugin, operation), and kept
  with its state until its plugin starts again. Only the selected tool's view is
  shown in its plugin; the others, and every tool while the launcher is hidden, are
  hidden (`Surface::set_hidden`): they keep their state and last picture but draw
  nothing and get no input, so tools of one plugin never take each other's clicks
  or scrolling. What a tool keeps in GPUI's element state rather than its own (an
  untracked scroll offset) starts over when it is shown again. The newest input is buffered until the
  tool object arrives; an unchanged input isn't re-sent. The tool is told whenever its view is shown
  or hidden (`ToolApi::visibility_changed`; `Tool::on_shown` / `on_hidden`), and once it arrives
  if it's shown: a tool showing what goes stale fetches it again then, as the launcher coming back
  with the same input tells it nothing else.
- → in the list moves keyboard focus into the tool's surface.
- The tool view fills the pane's height under the title (the guest wraps it
  in a full-size flex column); views are transparent, the window is the
  background.

## Footer

- 44px. Left: toasts, a green check + message for 1.6 s (stopped-plugin
  toasts 8 s, once per plugin per run). The previous attempt showed input
  statistics there (size, lines, chars, words); the rewrite drops them, and a
  tool can offer them instead.
- Right: the tool's first 4 actions that aren't hidden as text buttons with keycaps, vertical
  dividers between, then a pin (Lucide `pin`), only while "Hide when another
  app is used" is on, in the muted icon colour with no background but the
  hover's: on (the pin's head filled, `pin-filled`), the launcher and the plugins'
  windows stay up while another app is used, until the launcher is next
  hidden (Esc, the hotkey, opening Settings), which turns the pin off. The
  setting itself doesn't change. (The previous attempt had ⚙ there,
  Settings → General; the rewrite replaced it on 2026-10-01: ⌘, and the menu
  bar open Settings.)
- **Action keys**: an action may have only ↵, ⌘↵ or ⌥1 to ⌥9 (`enter`,
  `cmd-enter`, `alt-1`…`alt-9`), so every tool's keys are the same ones. Any
  other keystroke a plugin sends is refused (logged): the action becomes
  click-only. Likewise if the keymap already binds that keystroke in the
  current focus context stack, or an earlier action took it. The plugin API
  types them (`Shortcut::Enter`, `CmdEnter`, `Option(1..=9)`, `ClickOnly`).
  The launcher's key-down finds the matching action (same modifiers, key
  case-insensitive), performs it and stops propagation. Every action answers
  its key, whether it has a button or not: those past the 4 shown, and hidden
  ones (`Action::hidden()`), which never take a button, for keys
  a tool wants without spending one (Service Hub's ⌥1–⌥3 environments). A key
  no action has goes on to the input (⌥1 types `¡` there). Labels like `⌘↵` and
  `⌥1`.

## Input history

- File `Application Support/Delight/input-history.json`, apart from settings
  because inputs often hold tokens: `{ input_to_restore, remembered: [{
  plugin_id, operation_id, text }] }`, newest last.
- Limits: 10,000 inputs; inputs over 1,729 chars are neither remembered nor
  restored; remembered text ≤ 4 MiB total (oldest dropped); completions look
  at the newest 200; searches return at most 200.
- Only plugins remember inputs (`remember_input(operation, text)`); nothing
  is automatic. Rejected: blank, too long, invalid ids. An exact repeat of
  the tool's last entry is skipped. The same tool's older entries that are a
  prefix of the new text are removed (`con` before `config-manager`); other
  tools' entries stay. Then the caps apply.
- The input is saved on hide and restored when the launcher is created.
- **Ghost completion**: among the newest 200, entries strictly longer than
  and case-sensitively starting with the text, deduped, newest first; the nth
  is shown, first line only, greyed after the cursor, only when the cursor is
  at the end, nothing selected, non-empty, no IME composing. Any edit clears
  it. Tab or → accepts all of it (and prefers that entry's tool); ⌥Tab or ⌥→
  its next word, with the spaces or punctuation before it, and the rest stays
  shown (the last word prefers the tool too). ⌃N older / ⌃P newer, only
  if such a completion exists; otherwise they are ↓/↑. In the empty input
  nothing shows by itself, but ⌃N or ⌃P shows the newest remembered input
  (instead of the tip) and then steps older / newer; any edit ends it. Must stay under 5 ms
  per keystroke with 10k entries.
- **⌃R search**: if history is off, toast "The input history is off: turn it
  on in Settings → General". Otherwise the input becomes "Search the history",
  prefilled with the input if it's one line, selected. Matches: entries
  containing every whitespace-separated word, case-insensitive, over all
  entries, newest first, max 200; blank query → newest 200. Rows: plugin
  badge (Puzzle if the plugin is gone), text on one line with `\n` shown as
  " ⏎ ", plugin name right; selected row accent. Empty: "Nothing remembered
  yet: tools remember inputs worth coming back to" / "No remembered input has
  these words". ↑↓ / ⌃P⌃N move; ↵ / Tab / click confirm (sets the input and
  prefers the tool); Esc or ⌃R cancel, restoring the input.
- Turning history off erases the file.

## Keys

The keymap is compiled in (JSON with comments, Zed-style contexts, later
binding wins in a context); not user-configurable.

- Global: ⌘Q quit.
- Editor: ⌫ ⇧⌫ ⌦ ⌥⌫ (word) ⌘⌫ (to line start; at a line start joins lines), ⌃U (as in a terminal: to line start, never joins lines);
  ←→↑↓, ⌃P/⌃N as ↑/↓, ⌥←/→ by word, ⌘← Home ⌃A line start, ⌘→ End ⌃E line
  end, ⌘↑/⌘↓ document start/end, ⇧ + each to select, ⌘A; ⌘C ⌘X ⌘V; ⌘Z,
  ⇧⌘Z; ⌃⌘Space character palette. Multiline: ⇧↵ ⌥↵ newline. With a
  completion: Tab accepts.
- Launcher: Esc dismiss; ⌘K clear; ⌘, settings; ⌃R history; Tab / ⇧Tab move
  focus (input → list → tool); ⌘1–⌘9 select tool n. In the editor at end of
  input: ↓ focuses the list (selecting the first tool if none). With a
  completion: ⌃N / ⌃P.
- History search: ↑↓ ⌃P⌃N move; ⌃R cancel; ↵ Tab confirm; Esc cancel.
- Tool list: ↑↓ ⌃P⌃N; → into the tool.
- Segmented control: ←/→; on the first segment ← goes back to the list.
- Settings: ⌘W / Esc close.
- Mouse in the editor: click, drag, ⇧-click extend, double-click word,
  triple-click line.

## Settings window

- 560×600 centred, title "Delight Settings", transparent titlebar, traffic
  lights at (14,14), not resizable or minimisable, opaque background (GPUI's
  blur is a washed-out grey in dark mode). One window, reused and activated.
- Toolbar tabs General (Settings icon) and Plugins (Puzzle icon): 72px wide,
  18px icon, 11px label, selected accent on `fill_strong`. Body scrolls,
  padding 20/18. Switching tab drops a plugin page.
- Opened from the tray, ⌘, in the launcher, a tool header or plugin row ⚙,
  or a plugin's `open_settings` (its page).

### General

Rows of `title (11px detail) … control`:

1. "Open Delight" — "Shortcut, anywhere": a shortcut recorder + "Reset".
2. "Appearance": segmented Auto / Light / Dark.
3. "Open at login" — "Start Delight when you log in".
4. "Hide when focus is lost".
5. "Input history" — "Restore the last input, and complete inputs tools
   remembered. Turning it off erases it."
6. "Auto-paste clipboard" — "Put the clipboard's text into the input when
   Delight opens".

Footer "Delight {version} · Plugin SDK {version}" (the rewrite: "Plugin API", each
a link to github.com/brijsiyag/delight, the plugin API's at "Writing a plugin";
accent on hover, "Open on GitHub"). With an update known, an
accent row "Delight X is available" + "Update…".

**Shortcut recorder**: 150×28; click to record; held modifiers show live as
accent keycaps, "Press a shortcut" when none; keystrokes are intercepted so
⌘W / ⌘, are recorded, not run; modifier-only presses ignored; plain Esc stops.
A shortcut needs ⌘, ⌥ or ⌃ (a global hotkey on a plain key would stop it
typing anywhere) and must be accepted by macOS: register the new one before
unregistering the old, so a refused one leaves the old working. Error text
(11px, error colour, max 220px) keeps recording. Registration failure at
startup falls back to the default.

### Plugins

- Install row: Puzzle icon, "Plugins are .wasm files built with delight-sdk"
  ("Starting plugins…" while loading), "Install…".
- Install: file picker (one or several files), prompt "Install", no extension
  filter → the **install window** (its own window; Settings doesn't open),
  for each file in turn: the plugin's icon, name, author and version, its id,
  a notice "Replaces the built-in|installed “x” v." if it does, its
  description, its permissions (each with its reason; "Needs no permissions")
  and its tools, with [Cancel, Install] (↵ installs, Esc leaves that one, ⌘W
  closes the window and ends the batch); a file that isn't a runnable plugin
  shows why, with [OK]. With several files the footer says "2 of 3". Install
  writes `plugins/<id>.wasm` (overwrite), then the next file. Files given
  while the window is open join the end of its list. When the last one is
  done, or the window closes, the plugins are reloaded once (not once per
  file).
- **Reloading** starts only what changed: a plugin running from a file whose
  size and modification time are the same goes on (its tools keep what they
  show, and a sign-in waiting in it goes on); new, changed and stopped ones
  start, removed ones and their windows go. The launcher keeps the open tools
  of the plugins that went on. If another reload is asked for meanwhile, only
  the last one's result is used.
- **Drop on the menu bar icon**: `.wasm` files dragged onto the icon open
  the install window, as picked files do (files that aren't `.wasm` are
  ignored; other drags aren't accepted). The icon's view is given the
  drop-target methods at runtime (`tray_drop.rs`), because `tray-icon` has no
  drop support.
- **Long text** never breaks a page: names, titles, versions, authors and ids
  show on one line, ending in "…" (line breaks and tabs in them become
  spaces); a description takes at most 4 lines on the plugin page and in the
  install window (3 beside a tool, 2 for a notice or a settings row's detail,
  3 for a card's footer); a permission's reason is one line closed and at
  most 4 open, its "Allows" text 3, each program a chip on one line cut with
  "…". Anything that spills out of its lines (stacked accents) is clipped.
  Text is cut to a length (a title 120–160 characters, a description 400–700)
  before it is laid out, so a huge string costs the same as a long one.
- Every tool and every settings card a plugin has is shown. The launcher looks
  up a key (⌘1–⌘9) only for the first nine tools.
- Plugin row: badge 26; name + "Built-in|Plugin · v{version} · N tool(s)";
  description (truncated); permissions line; stopped line "Stopped: {reason}
  — off until Delight restarts"; ⚙ if it has settings; 🗑 for installed ones
  (confirm "Delete “name”?" — "This removes the plugin, its settings, its
  data and its saved secrets. It can't be undone."); an on/off switch.
  Delete removes the file, its settings, data folder, remembered inputs,
  secrets and disabled flag, then reloads. Built-ins can only be turned off.
- Broken plugins (fail to load): ⚠, file name, one-line summary ("Couldn't
  be read", "Not a Delight plugin", "Stopped while starting", "Invalid plugin
  id", …), "Copy details" ("The Delight plugin {name} doesn't load:
  {summary}.\n\n{detail}\n"), 🗑 → confirm → remove → reload.
- Plugin page: "‹ All Plugins" (accent, 12px); badge 36, name (15px
  semibold), description (11.5px); then Tools, Tips, Permissions, and the
  plugin's own settings: a titled card per section it names
  (`settings_sections`: id, title, height, footer note), which the app draws
  like its own cards (a tint of the text colour over the page, no border,
  hairlines between rows), with the plugin's rows on a surface inside (the
  plugin says how tall: a surface can't). A plugin without settings shows
  no cards. "This plugin is no longer loaded."
  **Permissions** are rows (in the install window too): icon (no tile), name and the
  plugin's reason in one line; click a row for the reason in full, what the
  permission allows and, for `Commands`, the programs as chips. All rows are
  closed at first; which are open is kept per plugin while Settings is open.

## Settings storage

- `Application Support/Delight/settings.json`, pretty JSON, missing fields
  default, unknown ignored, an invalid file logged and replaced by defaults;
  writes are atomic (temp file + rename). Fields: `appearance`
  (system/dark/light), `launcher_shortcut` (`cmd-shift-space`, GPUI syntax),
  `hide_on_blur` (true), `input_history` (true), `paste_clipboard_on_open`
  (false), `open_at_login` (false), `plugin_dir` (null = `<app
  dir>/plugins`, no UI), `disabled_plugins` (sorted set).
- Applying a change reacts only to what changed: appearance → theme and
  NSApp appearance; login → LaunchAgent; history off → erase; plugin_dir →
  reload; disabled → re-detect; then refresh windows.
- Per plugin: `plugins/<id>.wasm`; `plugin-settings.json` (`{id: any JSON}`,
  null removes); `plugin-data/<id>/` (mounted as `/data`); `plugin-secrets.json`
  (0600). Caches in `~/Library/Caches/Delight/`: compiled plugins, and
  `Clipboard/` for copied files.
- Plugin ids: `[A-Za-z0-9._-]`, 1–128 chars, not starting with `.` (they name
  files and folders). An installed plugin replaces a built-in with the same
  id.

## Theme

- Role tokens, not per-component ones (`delight_protocol::Theme`): `dark`; text, text_muted,
  text_faint; surface, card, fill, hover, border, separator; accent, accent_text, selection,
  focus_ring; success, warning, error, attention, and `tint_opacity` for a colour's quiet
  background; `background` (what a tool's view is drawn on) and `window` (the launcher around
  it); syntax (property, string, number, constant, comment, type, keyword, punctuation); fonts
  (`font`, `mono_font`) and sizes (text 13, small 11, large 15, mono 12; radius 8, small 6).
- Values: macOS HIG colours and Xcode syntax colours, light and dark, defined once:
  `Theme::light` and `Theme::dark` in `crates/protocol/src/theme.rs`. A test checks every colour
  is set in both. The UI kit defines none: it reads the theme.
- UI font `.SystemUIFont`; mono: first installed of SF Mono / Menlo / Monaco;
  launcher input: bundled Lilex (OFL).
- The app chooses (`crates/app/src/theme.rs`): the Appearance setting, or in System mode the
  window appearance (Dark or VibrantDark). It chooses again only when the mode or the appearance
  changes, and windows refresh only if the theme actually changed.
- Plugins get the whole theme from the app (in the sandbox the appearance is always dark and there
  are no fonts), and again whenever it changes: each plugin's host object notifies, the plugin
  asks, and its views are drawn again. The plugin API draws every plugin view (tool, settings
  section, window) with the theme's text colour, size and font, so text a plugin doesn't colour
  is the theme's, not GPUI's default black.

## Shared components (delight-ui)

Rules: stateless builders are `RenderOnce` (the editor is the one stateful
component); controls are controlled (report the new value, the caller owns
state); id first in constructors; theme read in `render`; callbacks
`Rc<dyn Fn>`; disabled = opacity .5 + stop mouse-down + no `on_click`; one
size table (Small 20/11/12/8, Medium 26/12/14/10, Large 32/13/16/12 for
height / text / icon / padding).

- Button: Primary (accent + shadow), Secondary (default, `fill_strong`), Text
  (borderless, accent label); optional icon and keycap shortcut; radius 6.
- IconButton: icon + optional short label + tooltip; `.selected()` accent
  toggle; square.
- Icon: a Lucide subset generated by one macro (enum + embedded files, so a
  name can't miss its file); inherits text size and colour.
- LogoBadge: tool logo in its own colours, radius = size × 0.22, rasterised
  once at 128px with resvg and cached by content hash (GPUI would upscale the
  declared size, blurry); invalid SVG → empty square.
- Keycap: 18px, radius 4, 11px medium; Plain / OnAccent / Accent; macOS
  glyphs (↵ ⌫ ⌦ ⎋ ⇥ ↑↓←→), modifiers in ⌃⌥⇧⌘ order; `keystroke_for(action)`
  looked up at render time.
- SegmentedControl, Switch (32×19, 15px knob), Group (inset, radius 8,
  hairlines inset 10), Caption (11px semibold), Divider, Tooltip.
- CodeBlock: tree-sitter-highlight with each grammar's own query (JSON,
  YAML); captures map to tokens by longest prefix; JSON needs its key pattern
  re-appended last (the last matching pattern wins); first 2,000 lines then
  "… N more lines. Copy the output to get all of it."; colours resolved at
  draw time.
- TextEditor (GPUI has no text input): `String` with UTF-8 offsets (UTF-16
  only at the IME boundary, untrusted offsets clamped); single/multi-line,
  soft wrap; events Changed, CompletionAccepted, Focus, Blur; key-context
  flags `multiline`, `showing_completion`, `start_of_input`, `end_of_input`;
  every edit goes through one `replace` (undo, clear completion, reset goal
  x, autoscroll, pause blink); undo stores diffs, merges consecutive typing /
  backspace / delete, breaks on cursor moves, 200 steps / 16 MiB, IME commit
  is one step; grapheme-aware moves, words = alphanumeric or `_`, ↑/↓ by
  visual row with a goal column (past the ends → start/end); selection quads
  per row with a 6px tail past newlines; 2px cursor inset to the font, blink
  500 ms, solid 300 ms after a key.

## Plugin API (what authors see)

- `Plugin`: manifest; `detect(&Input) -> Vec<Detection>`; `tool_view(operation)`
  (made once per operation, kept); `has_settings`; `settings_view`.
- Tool view: `view()`, `update(&ToolContext)` (after creation and on every
  input change; keep it quick), `actions()` (footer, in order),
  `perform(action)` (the view does the copying, toasting, hiding).
- `Input { text, files }` (cheap clones; paths the sandbox can't open);
  `Detection::new(operation, confidence)`; `Action::new(id, label,
  shortcut)`, then `.primary()`, `.attention()` or `.hidden()` (a key, no
  button).
- Manifest: id (reverse-DNS), name, version, description, author, icon SVG
  (full colour, square, own background for both modes), tags, operations
  (id, title, description, tags), permissions, has_settings.
- Permissions and their labels: Network "Network", Commands "Runs commands",
  InputFiles "Reads pasted files", Clipboard "Reads the clipboard" (the previous
  attempt's; the rewrite has no Clipboard permission: every plugin reads and writes the
  clipboard, from any view, through GPUI's own calls).
- Host, no permission (done so far in the rewrite: `secret` / `set_secret`, UTC
  offset, `open_settings` as `show_settings`, `settings` / `set_settings` as a
  JSON value the app keeps per plugin in `plugin-settings.json`, read and written
  by the plugin as a type of its own, at most 256 KiB, `null` removes, and
  `set_input` as `set_launcher_input`, applied only while the plugin's own tool is
  selected): data
  folder `/data`; secret / set_secret (empty deletes); encrypt / decrypt
  (bound to this plugin); remember_input; set_input (undoable, drops files,
  re-detects: for chaining tools); toast; hide; copy_text; copy_file (name +
  bytes; Finder-style file, images also as image data; only the file name is
  used; the folder is wiped on each copy); open_url; open_settings (own
  page); add_font (before first use); theme (+ observe); UTC offset. No app pid: passing the right PID to a
  process tool is the user's business.
- Host, gated: read_input_file (InputFiles; the path must be one of the
  *current* input files); read_clipboard (the previous attempt's Clipboard; the
  rewrite: none, see above); run (Commands: a program
  the manifest lists, an absolute path directly in /bin, /sbin, /usr/bin or
  /usr/sbin; any args, stdin written then closed, no shell, empty environment,
  the plugin's data folder as working folder, killed after 60 s, waited off the
  main thread; output status / stdout / stderr as text, 16 MiB each). Settings
  and the install sheet show the listed programs under "Runs commands".
- set_input, toast, hide and open_settings are deferred: the launcher may be
  mid-update.
- `export_plugin!` does nothing natively, so a tool's unit tests run on the
  host. The SDK re-exports `gpui` (the one pinned revision) and `wstd` (its
  own driven copy).

## Built-in tools

Three plugins, each an umbrella its later tools join: **Formats**
(`plugins/formats`, `formats`), **Network** (`plugins/network`,
`network`) and **Graphics** (`plugins/graphics`, `graphics`).
Since 2026-10-02; before, each tool was its own plugin (`delight_json`,
`delight_yaml`, `delight_svg`, `delight_dns`). Each tool has its own icon, and
each plugin: a Lucide glyph (lucide-static 1.48.0, the UI kit's set; its licence in
each plugin's `assets/`) in white on a rounded square of an Apple system colour —
JSON `braces` orange (Formats has the same icon as its JSON tool), YAML `list-tree` pink,
Base64 `binary` green; DNS `globe` teal; SVG `pen-tool` on a cyan-to-blue gradient. .env and JWT are their names
instead, ".env" on brown and "JWT" on indigo, in Lilex (OFL, `formats/assets/OFL-lilex.txt`) as
outlines: logos are drawn without fonts. Two plugins have drawings of their own (since
2026-10-02): Network the internet's globe, meridians and parallels in white on a teal gradient;
Graphics a white painter's palette with red, orange, green and blue paints on an
indigo-to-violet gradient, large enough to read at the sidebar's 20 pt.
State (modes, options, a prefix or a secret) is in memory only. A result has no
caption above it: the tabs, or the tool's title, say what it is. Tabs take their
own height (26px), not a fixed row's, which cut off their top.

A tool that takes text (the .env prefix, a JWT's secret) has one field, always
shown, above its scrolling pane (never in it: GPUI's branch panics when a surface
with a text field scrolls); every change applies at once, with nothing to save.
GPUI has also panicked when text changed next to a field (the plan's "Watch out
for"), which a live result does: not seen with these yet.

### Formats

The input is parsed as JSON once, for every tool that takes JSON.

- **JSON**, operation "JSON". Detect: trimmed text starts with
  `{`, `[` or `"`; over 256 KiB a container gets 0.8 unparsed; object/array
  0.92; a JSON string holding JSON 0.9; other values nothing; parse error on
  a container 0.6 ("to show where it breaks"). View: segmented Format /
  Minify / Escape / Unescape, with indent 2↔4 (Format, Unescape) and "Sort keys
  A→Z" (Format, recursive) at their right; key order otherwise preserved;
  conversion in a background task, newest wins. Format unwraps a JSON string
  holding JSON; Escape compacts valid JSON first; Unescape accepts with or
  without quotes. Errors "Invalid JSON: line L, column C: …" with a "Near"
  excerpt (up to 3 lines, `{:>5} │ ` gutter, caret). Actions: "Copy
  formatted/minified/escaped/unescaped" on ↵, plus "Copy minified" on ⌘↵ in
  Format; toast "{label} — copied to clipboard".
- **YAML ⇄ JSON**, one tool (two before 2026-10-02): the converted text alone,
  the way the input reads: `{` or `[` first goes JSON → YAML (unless it fails
  as JSON and parses as YAML, as `{a: 1}` does), anything else YAML → JSON.
  Detect: a JSON container (or `{`/`[` over 256 KiB) at 0.5 so
  JSON ranks first. Else a shape check over the first 400 non-blank,
  non-comment lines: structured lines (`key:` / `key: value` with a key
  without whitespace or `//` and `:` followed by space or end; `- ` items;
  `---`; indented continuations) must be ≥ 1 and ≥ 80%. ≥ 2 → 0.85; one
  line → 0.6 if the value is one word or quoted/empty, else 0.35. Over 256 KiB
  × 0.9 unparsed; otherwise it must parse to a mapping or sequence. Rejects
  env files, `curl` lines, prose, broken JSON. Conversion (yaml-rust2 ⇄
  serde_json): scalar keys → strings, `.inf`/`.nan` → strings, aliases
  resolved, several documents → an array, errors with line/column; JSON →
  YAML with multiline strings, no leading `---`, trailing newline. Actions
  "Copy JSON" / "Copy YAML" on ↵.
- **Base64**: segmented Encode / Decode, picked from the input (and again
  whenever it changes), the result's size at their right ("UTF-8 · 55 bytes",
  "11 bytes → 16 characters"). Decode takes either alphabet, padded or not,
  ignoring line breaks; decoded JSON (object or array) is shown formatted.
  Detect (compact text of 8 characters or more): decodes to readable UTF-8
  0.85; to data 0.45, only with padding, `+` or `/`, or 24+ characters of
  mixed case and digits (words are Base64 too); else Encode at 0.2, for any
  text. Errors "Not text: N bytes of binary data", "Not Base64: …". Actions:
  encoding "Copy Base64" ↵, "Copy URL-safe" (unpadded) ⌘↵; decoding "Copy
  decoded" ↵ (as decoded), "Copy formatted JSON" ⌘↵.
- **.env ⇄ JSON**, one tool (two before 2026-10-02), the way the input reads:
  `{` or `[` first goes to variables, anything else to JSON.
  JSON → variables: one per line; nested keys joined with `_`, upper
  case, camelCase split (`database.timeoutMs` → `DATABASE_TIMEOUT_MS`), array
  items by index (`FEATURES_0`), null and empty containers empty; a value with
  spaces, quotes, `#`, `$` or line breaks double-quoted with `\` escapes.
  Only an object converts. A "Prefix" field at the top right (160px); the
  prefix is upper-cased and joined with `_`; the field is there only this way.
  Detect: a JSON object 0.4 (over 256 KiB 0.35). Actions "Copy .env" ↵, "Copy
  with export" ⌘↵.
  Variables → JSON: an object of strings, in order (a later duplicate wins).
  Blank lines and `#` comments skipped, `export` dropped, `NAME = value`
  allowed; double quotes with `\n`, `\r`, `\t` and `\` before any other
  character, single quotes literal, both may span lines; an unquoted value ends at ` #`. No `$`
  expansion. Errors "Line N: …". Detect: of the first 400 meaningful lines,
  ≥ 80% `NAME=value` (letters, digits, `_ . -`): two or more 0.85; one only if
  its name is upper case with a value, 0.6. Action "Copy JSON" ↵.
- **JWT**: for an HMAC token, the field "Secret, to verify the signature" at the
  top; then, each under its caption, the header as JSON ("Header"), the registered claims
  as rows (Subject, Issuer, Audience, Issued, Not before, Expires, ID), times
  in the user's zone (the app's UTC offset) with "3 h ago" / "in 20 h 48 m",
  Expires green while valid and red once past, Not before red until it
  comes ("Claims"); the other claims as JSON ("Other claims"). Detect: starts `eyJ`,
  two dots, no whitespace, and the header decodes with an `alg`: 0.95. As the
  secret is typed: "Signature verified with this secret" or "The signature
  doesn't match this secret" (nothing while it's empty); HS256, HS384 and
  HS512 only, other algorithms are read, not verified. The secret is kept, and
  the next token verified with it, until the plugin stops. Actions "Copy
  payload" ↵, "Copy header" ⌘↵.
- **JSON → JWT**: signs a JSON object as the payload, header `alg` and
  `typ`. Segmented HS256 / HS384 / HS512 and the field "Secret to sign with";
  the token under them, signed again as either changes ("Type a secret to sign
  the JSON with." while it's empty). Detect: a JSON object 0.3. Action "Copy
  token" ↵.

### Graphics

- **SVG Preview**. Detect: starts with `<svg` 0.95;
  starts with `<` and `<svg` in the first 1,024 chars 0.9. resvg without
  fonts (`<text>` isn't drawn); zoom = min(520/w, 200/h, 8), rendered at 2×;
  canvas full width × 240, radius 12; backdrops checkerboard (default, 8pt,
  own rounded corners), white, #1C1C1E; "W × H px". Errors "Can't show this
  SVG: …", "This SVG has nothing to draw". Actions "Copy PNG" on ↵, "Copy
  data URI" on ⌘↵ (↵ if there's no PNG).

### Network

- **DNS lookup**, needs Network (it runs no programs).
  Target:
  strip scheme, path, query, fragment, userinfo; `[v6]:port`, `host:port`;
  IP, or a hostname with ≥ 2 valid labels and an alphabetic TLD ≥ 2,
  lowercased. Detect: IP 0.8, host 0.75, URL host 0.45 (so URL tools win).
  400 ms debounce; "Looking up X…". The Mac's resolvers from the app
  (`host(cx).dns_resolvers`: macOS's configuration store and `/etc/resolver/`,
  what `scutil --dns` shows): the longest matching
  scoped domain (VPN split DNS), else the first unscoped. Queries sent by the
  plugin itself over UDP (hickory-proto's messages; non-blocking, 2 s each) for
  A, AAAA, CNAME, MX, NS, TXT, SOA concurrently; the system resolver (WASI's
  name lookup) at the same time; PTR for up to 4 addresses. Report: notices
  only for what's wrong (the query failed, NXDOMAIN, no A/AAAA, another
  status; apps get other addresses, "Apps get X instead (the system
  resolver): check /etc/hosts, VPN or proxy settings", or the system resolver
  fails); then one table, Type (a badge: A/AAAA accent, CNAME orange, MX
  green, others grey) · Name · Value (wraps) · TTL (d/h/m when exact): the
  CNAME chain, A, AAAA, MX, NS, TXT, SOA (primary, admin, serial), PTR of the
  addresses; under it "via SERVER (default resolver | scoped to *.domain) · N
  ms". IP
  targets: the PTR rows, and "IPv4 · private · via …" under them. Actions
  "Copy addresses" / "Copy names" on ↵, "Copy dig command" on ⌘↵.

## Secrets

- One random 256-bit master key in the login Keychain (service
  `dev.delight.app`, account `encryption-key-v1`), touched only at the first
  secret operation; created only if nothing is saved yet (never silently mint
  a new key over existing ciphertext: error instead).
- AES-256-GCM (`ring`), `"v1." + base64url_nopad(nonce ‖ ciphertext ‖ tag)`,
  fresh nonce each time. AAD `delight-secret-v1\0{plugin}\0{key}` for secrets
  and `delight-plugin-v1\0{plugin}` for encrypt/decrypt, so moved, modified
  or foreign values fail.
- `plugin-secrets.json` `{plugin: {key: ciphertext}}`, 0600; keys 1–256
  bytes; deleting a plugin forgets its secrets.

## Updates, restart, release

- Sparkle 2.9.6 loaded dynamically from `Contents/Frameworks` (dev builds run
  without it); delegate reports found / not found to the tray and General;
  daily automatic checks; feed
  `github.com/brijsiyag/delight/releases/latest/download/appcast.xml`, EdDSA
  key in Info.plist.
- Plugin updates (new in the rewrite; plan step 16), each plugin on its own:
  each installed plugin that names its location has `<location>/<id>.xml`
  read 30 s after launch and every 24 h, and when its page's ↻ is
  clicked. Nothing about updates is in General. An installed plugin that
  names its location has an "Update" card first on its page, under the
  description, of one row with a 12 pt line: what the last look found and a
  button on the left, and at the right a small checkbox "Update
  automatically" (updates that ask for no new permissions install on their
  own), per plugin, on by default.
  With nothing newer: "Last checked: Today at 4:34 PM" (when a look last read
  the manifest since Delight started; macOS's medium date and short time,
  relative, in the user's language and 12/24-hour setting), "Not checked
  yet" before the first, "Checking…" while one runs, "It is up to date"
  (green) for 5 s after one finds nothing newer, or why it failed (red, cut
  to the line, all of it in a tooltip); the button is a small ↻ ("Check for
  updates"). With a newer version: "Version X is available" (accent)
  [Update]; while it downloads, a thin bar and "Downloading X… 1.2/3.4 MB"
  (megabytes of a million bytes; of how many once the server says), no
  button (it installs as soon as the download is done); "Version X asks for
  new permissions" [Review…], which opens the install window on the
  downloaded file; "Version X didn't install: why" [Try Again]. Automatic
  updates skip a plugin whose tool is shown or whose window is open. The
  tool's header in the launcher offers the update too (Tool pane).
- Deleting a plugin, or a file that doesn't load, from its page keeps it
  selected until the sidebar no longer lists it (a plugin stops a moment
  later); then the page below it takes its place, or the one above when it
  was last. Picking another page meanwhile cancels that.
- Installing (new in the rewrite; plan step 16): under the sidebar's list, a
  small split button, 24 pt tall with a hairline border: "+ Install Plugin"
  opens the macOS file picker (several `.wasm` files), and its chevron opens a
  menu above it: "From a File…" (".wasm files on this Mac") and "From a
  Link…" ("Plugins published at a URL"); a click elsewhere closes it. From a
  Link opens the install window (520×540) on one page, laid out as a plugin's
  page there (20 pt margins): a header with a 44 pt accent-tinted tile and a
  globe, "Install from a Link" and "Plugins published on GitHub, or at any web
  address"; the Link field, styled as Settings' search field (placeholder
  `https://github.com/Meesho/delight-plugins/releases/latest/download/plugins.xml`);
  then a card that, while empty, shows a puzzle icon and "Paste a link to see
  its plugins". A link is looked at 300 ms after it stops changing (a paste: at
  once), with no button: the card says "Looking at the link…", then the
  caption "N Plugins" with Select All / Select None over a card with one row
  per plugin, a checklist line with no logo (a link carries none): checkbox,
  name with its version beside it ("0.0.3 → 0.0.5" for an update), description
  in two lines under them, and a pill at the right, "Update" (accent) or
  "Installed" (grey, the row dimmed, can't be picked); the checkbox and the
  pill line up with the name. New and newer ones start picked. A bad link or no plugins: the card shows a red
  warning, "Couldn’t use this link" and why, and the field's border is red.
  The footer has "N selected" on the left, then [Cancel, Download]: Download
  fetches the picked plugins one after another, each row with a thin progress
  bar under it and the megabytes so far at the right, "1.2/3.4 MB"
  (the bar green when done, and the file's size, "3.4 MB", in its place; a
  red cross and why if it failed); the button says "Downloading…"
  meanwhile, then "Review N Plugins…" ("Review Plugin…" for one), not
  "Install": it shows each one as a picked file, "1 of N", installed only on
  its own Install.
  Picking one more after downloading brings Download back, for it alone.
  Changing the link stops the downloads. ↵ is the main button; Esc closes.
- Quit hides first (saving the input). Restart in a `.app`: hide, GPUI
  `cx.restart()`. Outside a bundle: spawn the own exe with `--restarted`
  inheriting the environment, then quit (the new one waits for the lock).
  `exec` doesn't work (macOS keeps the tray and windows tied to the old
  program).
- Release (`xtask`): check versions → universal build (`lipo`) → bundle
  (Sparkle via `ditto`, Info.plist version and build = commit count, icon via
  `qlmanage` / `sips` / `iconutil`, `plutil -lint`) → sign (Developer ID for
  team S3L4RJ57GY only; Sparkle re-signed inside out; hardened runtime) →
  notarise and staple the zip → `Delight-X.Y.Z.zip` + `appcast.xml` → pkg for
  `/Applications` with `BundleIsRelocatable=false` (else Installer may update
  a copy elsewhere), signed, notarised, stapled. Downloads pinned by SHA-256.
  Entitlement `disable-library-validation`, no App Sandbox (both to revisit:
  they were for dylib plugins).
- Versions: the app version must match the built-ins' and the release tag
  (`v` + version); the SDK is versioned separately.
