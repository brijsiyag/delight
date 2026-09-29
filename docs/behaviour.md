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
  bolt icon and the footer drag the window (`performWindowDragWithEvent`);
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
  and active → hide, else show.
- Show: size first; present outside any GPUI update (AppKit calls back into
  GPUI synchronously): remember the frontmost app if it isn't Delight,
  activate Delight, `orderFrontRegardless` + `makeKeyWindow`; only then focus
  the input (earlier focus doesn't stick); optionally auto-paste; select all
  so typing replaces the input. The app activates like Raycast/Alfred rather
  than as a non-activating panel, which would leave modifier-only keys going
  to the previous app.
- Hide: `orderOut` (never close; state survives), close ⌃R search. Re-activate
  the previous app only if the launcher had the keyboard (not when another
  app or Settings took focus). Save the input for restore if history is on.
- Esc hides (deferred). Losing activation while visible hides if "Hide when
  focus is lost" is on (default on).
- At launch the window is shown and the app activated.
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
- Layout: padding 20, 22px bolt icon centred on the first line, gap 16; when
  expanded a ⓧ clear button on the first line; ⌃R replaces bolt and input
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
- Empty states (Sparkles icon): no candidates → "No tool fits this input" /
  "Plugins in the plugins folder add tools."; candidates but none selected →
  "No strong match" / "Pick a tool on the left, or keep typing."
- A stopped plugin: "{name} stopped: {reason}\n\nIt's off until Delight
  restarts (menu bar → Restart Delight)."
- A pane is made on first selection, one per (plugin, operation), and kept
  with its state until plugins reload. The newest input is buffered until the
  tool object arrives; an unchanged input isn't re-sent.
- → in the list moves keyboard focus into the tool's surface.
- The tool view fills the pane's height under the title (the guest wraps it
  in a full-size flex column); views are transparent, the window is the
  background.

## Footer

- 44px. Left: toasts, a green check + message for 1.6 s (stopped-plugin
  toasts 8 s, once per plugin per run). The previous attempt showed input
  statistics there (size, lines, chars, words); the rewrite drops them, and a
  tool can offer them instead.
- Right: the tool's first 4 actions as text buttons with keycaps, vertical
  dividers between, then ⚙ (Settings → General).
- **Action keys**: each action's shortcut is parsed as a GPUI keystroke. It
  is dropped (the action becomes click-only) if the keymap already binds that
  keystroke in the current focus context stack, or an earlier action took it.
  The launcher's key-down finds the matching action (same modifiers, key
  case-insensitive), performs it and stops propagation. (Quirk: actions past
  the 4 shown still answer their keys.) Labels like `⇧⌘↵`.

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
  it. Tab accepts (and prefers that entry's tool). ⌃N older / ⌃P newer, only
  if such a completion exists; otherwise they are ↓/↑. Must stay under 5 ms
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
- Editor: ⌫ ⇧⌫ ⌦ ⌥⌫ (word) ⌘⌫ (to line start; at a line start joins lines);
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
- Opened from the tray, ⌘, in the launcher, the footer ⚙, a tool header or
  plugin row ⚙, or a plugin's `open_settings` (its page).

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

Footer "Delight {version} · Plugin SDK {version}". With an update known, an
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
- Install: single-file picker, prompt "Install", no extension filter →
  inspect → failure: warning "Not installed" + error; success: "Install
  “Name” version?" with "It can: Network · Runs commands." or "It needs no
  permissions." [Install, Cancel] → write `plugins/<id>.wasm` (overwrite) →
  reload plugins.
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
  semibold), description (11.5px); a surface in a fixed 420px box. "This
  plugin has no settings." / "This plugin is no longer loaded."

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

- Role tokens, not per-component ones: `dark`; colours (label, secondary /
  tertiary label, surface, surface_elevated, fill, fill_strong, hover,
  active, selected, selection, cursor, focus_ring, separator, border, accent,
  accent_text); status tints success/warning/error/info (fg, bg); palette
  red…gray; syntax (keyword, string, number, comment, property, function,
  type, constant, punctuation); text (ui_font, mono_font, size_sm 11,
  size_base 13, size_lg 15, mono_size 12); metrics (space 4, radius_sm 6,
  radius_md 8, control_height 26, row_height 30).
- Values: macOS HIG colours and Xcode syntax colours, light and dark
  (`ui/src/theme/palette.rs`). A test checks every token is non-transparent
  in both modes.
- UI font `.SystemUIFont`; mono: first installed of SF Mono / Menlo / Monaco;
  launcher input: bundled Lilex (OFL).
- System mode follows the window appearance (Dark or VibrantDark). The
  theme is recomputed only when the mode or the appearance changes, and
  windows refresh only if it actually changed.
- Plugins get it from the host (in the sandbox the appearance is always dark
  and there are no fonts), follow changes, and redraw only when it differs.

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
  `Detection::new(operation, confidence)`; `Action::new(id,
  label).shortcut("cmd-enter")`.
- Manifest: id (reverse-DNS), name, version, description, author, icon SVG
  (full colour, square, own background for both modes), tags, operations
  (id, title, description, tags), permissions, has_settings.
- Permissions and their labels: Network "Network", Commands "Runs commands",
  InputFiles "Reads pasted files", Clipboard "Reads the clipboard".
- Host, no permission: settings / set_settings (JSON; null removes); data
  folder `/data`; secret / set_secret (empty deletes); encrypt / decrypt
  (bound to this plugin); remember_input; set_input (undoable, drops files,
  re-detects: for chaining tools); toast; hide; copy_text; copy_file (name +
  bytes; Finder-style file, images also as image data; only the file name is
  used; the folder is wiped on each copy); open_url; open_settings (own
  page); add_font (before first use); theme (+ observe); UTC offset; app pid.
- Host, gated: read_input_file (InputFiles; the path must be one of the
  *current* input files); read_clipboard (Clipboard); run (Commands: absolute
  program, args, stdin written then closed, no shell, waited off the main
  thread; output status / stdout / stderr).
- set_input, toast, hide and open_settings are deferred: the launcher may be
  mid-update.
- `export_plugin!` does nothing natively, so a tool's unit tests run on the
  host. The SDK re-exports `gpui` (the one pinned revision) and `wstd` (its
  own driven copy).

## Built-in tools

State (modes, options) is in memory only.

- **JSON** (`tools/json`), operation "JSON". Detect: trimmed text starts with
  `{`, `[` or `"`; over 256 KiB a container gets 0.8 unparsed; object/array
  0.92; a JSON string holding JSON 0.9; other values nothing; parse error on
  a container 0.6 ("to show where it breaks"). View: segmented Format /
  Minify / Escape / Unescape; indent 2↔4 (Format, Unescape); "Sort keys A→Z"
  (Format, recursive); key order otherwise preserved; conversion in a
  background task, newest wins. Format unwraps a JSON string holding JSON
  ("Unescaped from a JSON string"); Escape compacts valid JSON first;
  Unescape accepts with or without quotes. Errors "Invalid JSON: line L,
  column C: …" with a "Near" excerpt (up to 3 lines, `{:>5} │ ` gutter, caret).
  Actions: "Copy formatted/minified/escaped/unescaped" on ↵, plus "Copy
  minified" on ⌘↵ in Format; toast "{label} — copied to clipboard".
- **YAML** (`tools/yaml`), operations "YAML → JSON" and "JSON → YAML".
  Detect: a JSON container (or `{`/`[` over 256 KiB) → JSON → YAML at 0.5 so
  JSON ranks first. Else a shape check over the first 400 non-blank,
  non-comment lines: structured lines (`key:` / `key: value` with a key
  without whitespace or `//` and `:` followed by space or end; `- ` items;
  `---`; indented continuations) must be ≥ 1 and ≥ 80%. ≥ 2 → 0.85; one
  line → 0.6 if the value is one word or quoted/empty, else 0.35. Over 256 KiB
  × 0.9 unparsed; otherwise it must parse to a mapping or sequence. Rejects
  env files, `curl` lines, prose, broken JSON. Conversion (yaml-rust2 ⇄
  serde_json): scalar keys → strings, `.inf`/`.nan` → strings, aliases
  resolved, several documents → an array ("N YAML documents → a JSON
  array"), errors with line/column; JSON → YAML with multiline strings, no
  leading `---`, trailing newline. Actions "Copy JSON" / "Copy YAML" on ↵.
- **SVG** (`tools/svg`), "SVG Preview". Detect: starts with `<svg` 0.95;
  starts with `<` and `<svg` in the first 1,024 chars 0.9. resvg without
  fonts (`<text>` isn't drawn); zoom = min(520/w, 200/h, 8), rendered at 2×;
  canvas full width × 240, radius 12; backdrops checkerboard (default, 8pt,
  own rounded corners), white, #1C1C1E; "W × H px". Errors "Can't show this
  SVG: …", "This SVG has nothing to draw". Actions "Copy PNG" on ↵, "Copy
  data URI" on ⌘↵ (↵ if there's no PNG).
- **DNS** (`tools/dns`), "DNS lookup", needs Commands + Network. Target:
  strip scheme, path, query, fragment, userinfo; `[v6]:port`, `host:port`;
  IP, or a hostname with ≥ 2 valid labels and an alphabetic TLD ≥ 2,
  lowercased. Detect: IP 0.8, host 0.75, URL host 0.45 (so URL tools win).
  400 ms debounce; "Looking up X…". Resolver from `scutil --dns` (longest
  matching scoped domain — VPN split DNS — else the first unscoped); `dig` for
  A, AAAA, CNAME, MX, NS, TXT, SOA concurrently; the system resolver (WASI
  name lookup) at the same time; PTR for up to 4 addresses. Report: status
  line; a warning when the system resolver disagrees ("check /etc/hosts, VPN
  or proxy settings"); sections Addresses, CNAME chain, Mail, Name servers,
  TXT, Zone, System resolver, Reverse, Resolver; TTL as d/h/m when exact. IP
  targets: PTR, address class (loopback / private / public). Actions "Copy
  addresses" / "Copy names" on ↵, "Copy dig command" on ⌘↵.

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
