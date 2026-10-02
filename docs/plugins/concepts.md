# How a plugin runs

What Delight does with a plugin, from starting it to drawing its tools, and the rules it runs
within. Knowing this explains most of the API.

```text
Delight.app (native GPUI)                      plugin.wasm (its own GPUI, in wasmtime)
──────────────────────────                     ───────────────────────────────────────
reads the manifest from the .wasm ───────────▶ (nothing runs yet)
starts the plugin ───────────────────────────▶ Plugin::new
input changes ───────────────────────────────▶ Plugin::detect          → tools and confidences
ranks every plugin's answers, picks a tool
first time a tool is picked ─────────────────▶ Plugin::open_tool       → a view, kept
shows the tool's surface ◀──────────────────── the view's drawing
input changes, tool shown ───────────────────▶ Tool::on_input_changed
footer ◀───────────────────────────────────── Tool::list_actions
key or click on an action ───────────────────▶ Tool::perform_action
                           ◀───────────────── host(cx): toast, hide, set_input, http, …
```

## The manifest comes first

The `#[plugin]` and `#[derive(Operations)]` macros write what the plugin is (its id, name, version,
icon, permissions, tips and tools) into a custom section of the `.wasm`. Delight reads it **without
running the plugin**: to show it before installing, to list its tools and permissions in Settings,
and to set up its sandbox. So none of it can be decided at run time, and a plugin can't have
tools it didn't declare.

## Starting

Delight starts every plugin that is on when it launches: the built-ins, then the installed ones.
Each is its own WebAssembly instance with its own copy of GPUI. `Plugin::new` runs once, and the
plugin lives until Delight quits, the plugin is reinstalled or updated, or it stops. Delight keeps
compiled plugins in `~/Library/Caches/Delight`, so starts after the first are quick.

An installed plugin with the id of a built-in replaces it.

## Detection

Each change of the input (after 30 ms without another) is sent to every running plugin's
`detect`, all at once. Blank input sends nothing. Each plugin answers with its tools that fit and
a confidence for each:

- Confidence is clamped to 0…1, and 0 means *doesn't fit*: the tool isn't listed.
- **0.5 or more is *Recommended*.** The highest recommended tool is selected by itself, so its
  view is up before the user picks anything.
- **Below 0.5 is an *Other Match***: listed after a divider, picked by hand.
- Ties keep the order of the plugins, then of each plugin's tools.
- Delight has no rules of its own about inputs: which tool comes first is only the confidences.

### How sure to be

Confidence is how sure the plugin is that *this* input is for its tool, compared with every other
plugin. **Keep it at 0.8 or below**; go above only when the input is certainly yours:

| Confidence | When | Examples |
|---|---|---|
| **0.8 – 1** | Certainly for this tool: the input starts with a keyword the plugin owns, or is a format nothing else reads | `weather london`, `npm react`, `case hello`; a JWT (`eyJ…` with a header that decodes); `<svg` |
| **0.5 – 0.8** | Likely: it looks like what the tool takes | a host name for a DNS lookup; a JSON object for a JSON formatter |
| **below 0.5** | Possible: offered, never opened by itself | any line of words for a text converter; any JSON object for a tool that signs one |

A keyword is the plugin's promise: typing `weather` means *the weather tool*, so the tool can be
sure. Guessing from the input's shape is never that sure, because other plugins read the same shape:
a JSON object is JSON to a formatter, a payload to a JWT signer and variables to a `.env` converter.
Two plugins that both answer 0.95 for the same shape fight over it, and the user gets whichever
loaded first.

**`detect` runs on every keystroke.** Return quickly: look at the start of the input, cap the
work by size (the built-ins skip parsing over 256 KiB and answer from the first characters), and
leave the real work to the tool. A slow `detect` makes the launcher wait for it.

When a remembered input is taken from the history (a completion, or ⌃R), the tool that remembered
it is preferred, whatever the confidences.

## A tool's life

- **Opened once.** The first time one of its tools is selected, Delight calls `open_tool` and
  keeps the view, with all its state, until the plugin stops or restarts. Picking it again later
  shows the same view.
- **Only the tool shown gets the input.** When the input changes, the selected tool's
  `on_input_changed` is called; a tool selected later gets the input it hasn't seen when it is
  shown. An unchanged input isn't sent again.
- **Shown and hidden.** One tool is shown at a time, and none while the launcher is hidden. A
  hidden tool keeps its state and its last picture, but draws nothing and gets no keys or clicks.
  `Tool::on_shown` and `on_hidden` say when this happens: refresh anything that goes stale (a list
  from a server) in `on_shown`, since the launcher coming back with the same input sends nothing
  else.
- **Focus.** → in the tool list moves the keyboard into the tool's first control, and ← on it goes
  back. A click elsewhere in the launcher calls `Tool::on_focus_lost`, so a menu the tool holds
  open can close.
- **Actions.** The footer shows the tool's actions, and their keys work while the input or the tool
  has the keyboard. See [Tools and actions](tools.md).

## One thread, and a time limit

A plugin has one thread. Everything it does runs there: calls from Delight, its tasks
(`cx.spawn`, `cx.background_executor().spawn`), timers and drawing.

- **A turn may take one second.** Each round of work (a call from Delight, or the plugin's ready
  tasks and a frame) that runs longer than that stops the plugin. Spawning a task lets `detect` or
  `on_input_changed` return at once, and dropping the `Task` cancels work a newer input made
  pointless, but a task that computes for two seconds still stops the plugin: a turn runs every
  task that is ready. Cap the work (the built-ins look at the first 256 KiB, or the first 2,000
  lines), or split it into pieces with a short timer between them.
- **`Plugin::new` is a turn too.** Parse big data the plugin carries when it is first needed, not
  at start.
- **Never block.** Waiting on a lock, a channel or `std::thread::sleep` blocks the only thread.
  Await instead: everything Delight offers through `host(cx)` returns a GPUI `Task`, and timers are
  `cx.background_executor().timer(duration)`.
- **Memory** may grow to 512 MiB, and one frame's drawing to 100,000 primitives. Past either, the
  plugin stops.

## The sandbox

A plugin is a WASI component that sees almost nothing of the Mac:

| | A plugin |
|---|---|
| Files | Only its own data folder, at `/data`. Nothing else, not even the input's files |
| Network | Only with the `Network` permission: HTTP through Delight, WASI's own sockets and name lookup |
| Programs | Only with the `Commands` permission, and only the programs it lists |
| Clipboard | GPUI's own `cx.read_from_clipboard` and `cx.write_to_clipboard`, for every plugin |
| Environment | Empty. No processes, no process id |
| Clock | Wall-clock time, in UTC: the Mac's time zone comes from `host(cx).utc_offset_seconds` |
| stdout, stderr | Into Delight's log, marked with the plugin's name |

[Permissions](host-api.md#permissions) has the details.

## When a plugin stops

A panic, a turn over its second, too much memory or drawing, or a call it doesn't answer within
three seconds stops the plugin. Delight then shows *"Name stopped: why. It's off until Delight
restarts."* in place of its tools, toasts once, and carries on with every other plugin. Installing
the plugin again starts it again. The reason is in the log.

## Talking to the app

`host(cx)` is the app as the plugin sees it: [The host API](host-api.md).
