# Pitfalls

The problems earlier plugins ran into, Delight's built-ins and third-party plugins alike (a
calendar, a logs search, incidents, a Kubernetes browser, a service catalogue), with what each
looked like, why it happened, and what to do instead. Each was hit in a real plugin.

Read it once before you start, and again before you publish.

**Contents:** [Building](#building) · [The manifest and ids](#the-manifest-and-ids) ·
[Detection](#detection) · [A tool's life](#a-tools-life) · [The time limit](#the-time-limit) ·
[Drawing](#drawing) · [Text fields and keys](#text-fields-and-keys) · [Scrolling](#scrolling) ·
[Settings](#settings) · [Windows](#windows) · [Storage and secrets](#storage-and-secrets) ·
[The network](#the-network) · [Commands](#commands) · [Time and the system](#time-and-the-system) ·
[Logs](#logs) · [Versions and publishing](#versions-and-publishing) ·
[Testing](#testing)

## Building

**A second GPUI.** *You see:* type errors between two identical-looking types (`expected
gpui::Div, found gpui::Div`), or a plugin whose views Delight can't show. *Why:* Cargo treats a
git dependency spelled differently (another branch, a `rev` for the same commit, crates.io's
`gpui`) as another crate, and Zed's repository has two packages named `gpui`. *Do:* copy the
`gpui` line from [Getting started](getting-started.md#cargotoml) exactly, commit `Cargo.lock`, and
check with `cargo tree -d -i gpui`.

**GPUI's derives need a crate called `gpui`.** *You see:* `#[derive(IntoElement)]` failing to find
`gpui::…`. *Why:* the derives expand to `gpui::` paths, and the re-export
`delight_plugin_api::gpui` has another name. *Do:* depend on `gpui` directly, as above.

**"No method named …" for a method GPUI has.** *You see:* `detach_and_log_err`, `on_click` or
`background_spawn` not found. *Why:* each is a trait's (`TaskExt`, `StatefulInteractiveElement`,
`AppContext`), and `on_click` also needs the element to have an `.id(…)`. Code copied from another
GPUI version fails in other ways too (`cx.update` in async code returns the value, not a
`Result`, on this branch). *Do:* `use gpui::prelude::*;` in every file, and write against the
pinned GPUI's own source (`~/.cargo/git/checkouts/zed-*/`), not another version's docs.

**GPUI component libraries.** *You see:* a second GPUI, as above, after adding gpui-component
(gpui-kit). *Why:* it builds on crates.io's GPUI snapshots, and its highlighting is native-only.
*Do:* draw with GPUI and your own code until embedded GPUI is released.

**Patches don't travel.** *Why:* Cargo applies `[patch]` only from the workspace being built, so
GPUI's `async-task` patch doesn't reach a plugin through its dependencies. Without it the plugin
builds, but on other code than Delight's. *Do:* repeat the `[patch.crates-io]` entry.

**C needs the WASI SDK, and Delight's settings don't reach you.** *You see:* a C compile failure
for `wasm32-wasip2` (tree-sitter, `ring` inside rustls, compression crates), or a built-in missing
in a development build because its C wasn't built. *Why:* clang and libc for WebAssembly come from
the WASI SDK, and Cargo reads `.cargo/config.toml` only from the workspace being built and its
parents. *Do:* set `WASI_SDK_PATH` in your own `.cargo/config.toml` ([C code](getting-started.md#c-code)),
or avoid C: a logs plugin wrote a small JSON highlighter in Rust instead of tree-sitter.

**Your own HTTP stack.** *You see:* the plugin listed as *Doesn't start* (a `wasi:http` import:
wstd's client, waki, Spin), crates that don't build (tokio, reqwest), a client that freezes the
plugin (ureq over WASI sockets blocks its only thread), or certificates refused behind a company
proxy (webpki's bundled roots). *Why:* embedded_gpui doesn't link `wasi:http` yet, and a plugin has
one thread. *Do:* `host(cx).http` ([The network](host-api.md#the-network)). Running `curl` through `Commands`
isn't the answer either.

**Debug builds.** *You see:* a `.wasm` of about 250 MB that is slow to start. *Do:* build
`--release` with `lto`, `opt-level = "s"` and `strip`: a few megabytes.

**Building against a local Delight checkout.** *You see:* a plugin the released Delight refuses
(*built for protocol 0.4, and this Delight runs 0.0 to 0.3*), or an update offered and failing.
*Why:* a `[patch]` to a checkout builds against that checkout's plugin API, which may be newer than
any release. *Do:* publish only builds against a release's tag, with the patch removed, and use the same tag
for `delight-plugin-api` and `delight-manifest`.

**Stale `.wasm` files.** *You see:* a build that finishes in a second and a file the old size, or
an old plugin published or loaded next to the new one after a rename. *Why:* a failed build leaves
the previous file, and renamed crates leave theirs in `target/`. *Do:* check the build succeeded and
the file is new; delete old outputs after a rename; publish files by name, not `target/…/*.wasm`.

**Delight's `target/` folder.** A development build of Delight loads every `.wasm` in
`delight/plugins/target/wasm32-wasip2/release` as a built-in: a plugin built there to share
Delight's build cache turns into one. Use your own target folder.

**Disk.** *You see:* *No space left on device*. *Why:* each GPUI build for WebAssembly is several
GB, and a native `cargo test` adds a multi-GB `target/debug`. *Do:* check `df -h ~` before a big
build; Cargo's `incremental` folders and `target/debug` are safe to delete.

**What you embed is public, and heavy.** Data built into the plugin (`include_str!`, a `build.rs`
that copies a file in) can be read by anyone with the `.wasm`: an API key there isn't secret.
A `build.rs` that quietly builds without its git-ignored file ships a plugin without it from a
fresh clone. And embedded data costs: a 1.2 MB JSON took a plugin from 5.8 to 7.1 MB and paused it
on first use, with 70% of it unused. Embed only what you use, fail the build when a needed file is
missing, and keep keys out.

## The manifest and ids

**An id is forever.** *You see:* after changing a plugin's id, installed copies don't update, and
the data folder, settings, secrets, history and on/off switch are left behind (users have to sign
in again). *Why:* everything Delight keeps for a plugin is keyed by its id; a new id is a new
plugin. The same for an operation's `id`: changing it loses that tool's remembered inputs. *Do:*
choose ids carefully once, and never change them after a release.

**The file is named by the id.** *You see:* *Its file isn't named by its id* for a `.wasm` copied
into Delight's plugins folder by hand. *Why:* Delight finds plugins as `<id>.wasm`, while Cargo
names the output after the crate (`service-hub` builds `service_hub.wasm`). *Do:* install with
**Install Plugin**, which names the file.

**One version per plugin.** The version is the crate's `version`. In a workspace where every crate
says `version.workspace = true`, every plugin moves together, and Delight sees a new version of
each. *Do:* give each plugin its own version, and bump it for every change you publish.

## Detection

**A slow `detect`.** *You see:* the launcher lagging as you type, or *Name stopped: it didn't
answer within 3s*. *Why:* `detect` runs on every keystroke, the launcher waits for every plugin's
answer before it shows the list, and a call over three seconds stops the plugin. One plugin copied
the whole input on each keystroke; another nearly shipped a refetch from `detect` that would have
retried on every key while offline. *Do:* decide from the text alone, reject early (a logs plugin
gives up above 8,192 characters, Formats looks at the first characters of input over 256 KiB),
never fetch from `detect`, and leave the work to the tool.

**The tool vanishes while its input is half typed.** *You see:* `meet ada@exa` shows nothing until
the address is complete. *Why:* `detect` only accepted complete input. *Do:* keep detecting the
half-typed form (`meet <anything>` at 0.9) and have the tool say what it needs; fetch only once the
input is valid.

**Too sure.** *You see:* your tool opens for inputs that belong to another, or another plugin's
tool keeps winning. *Why:* two plugins both answered above 0.8 for the same shape of input (a JSON
object, a URL). *Do:* stay at 0.8 or below unless the input starts with your own keyword or is a
format only you read ([How sure to be](concepts.md#how-sure-to-be)). Confidence is 0 to 1, not 0 to
100: anything over 1 counts as 1.

**The tool never shows.** *Why:* `detect` answered nothing, or 0, for that input. *Do:* test `detect`
on it in a unit test, and check the operation you return is the one you meant.

## A tool's life

**Hidden tools keep running.** *You see:* a tool polling a server every 60 seconds with the
launcher hidden, another polling a build server every 5 seconds with the user's token, animations
slowing other tools. *Why:* a hidden tool draws nothing and gets no input, but its timers and tasks
go on. *Do:* start polling and animations in `Tool::on_shown`, stop them in `on_hidden`.

**Nothing stale is refreshed when the launcher comes back.** *You see:* an incident list from an
hour ago when the launcher reopens with `incidents` still typed. *Why:* an unchanged input isn't sent
again. *Do:* refresh in `on_shown`; never rely on getting the same input twice.

**The wrong screen flashes on open.** *You see:* the "add your API key" screen for a moment every
time, though a key is saved. *Why:* the key was only known once a check against the server
answered. *Do:* keep three states (loading, none, some) and use the stored secret as soon as it is
read.

**Work that silently never happens.** *Why:* dropping a GPUI `Task` cancels it, and a call whose
`Task` is ignored loses its error. *Do:* keep tasks in a field (replacing it cancels the old one:
the "newest input wins" pattern) or `.detach()` them, and log the failures of calls nobody awaits.

**Everything restarts with an update.** Installing or updating a plugin restarts it: its tools open
afresh and its windows close. Keep what matters in settings, secrets or `/data`, not only in a view.

**The footer is asked on every notify.** `list_actions` runs after every `cx.notify()` of the tool,
hovers included, each a round trip to the app. Keep it cheap, and put state that changes often (the
hovered row) in a child view. Actions are fieldless variants: one per action, no data inside.

**`set_input` is ignored.** Delight applies it only while one of the plugin's own tools is shown,
and logs a warning otherwise.

## The time limit

**Tasks don't escape the one second.** *You see:* *Name stopped* after a big paste, though the work
was "in a task". *Why:* a plugin has one thread, and a turn runs every task that is ready, so a
task that computes for two seconds is a two-second turn (*wasm trap: interrupt*). *Do:* cap the
work, or split it into pieces with a short timer between them (a timer of 0 ms is due at once, in
the same turn). Waiting for the network doesn't count: only computing does.

**Pieces that never pause.** *You see:* *wasm trap: interrupt* in a loop that was split into slices
of 25 ms, after *input query unanswered within the query budget* in the log; a logs plugin reading
hundreds of small files hit it on the second search, when the files came from its data folder.
*Why:* it paused only inside an item too big for one slice, so items that each finished within
theirs ran one after another; and awaiting a future that is already ready (a file read from
`/data`, a cached answer) doesn't end the turn either. *Do:* give the turn one budget for all the
items in it, measured from when the turn began, and pause once it is used up, between items too;
keep it near 10 ms, under the 20 ms below; and look at the clock every few lines, not every few
hundred, when a line can be large.

**`Plugin::new` is a turn too.** Parse big embedded data when it is first needed (`OnceLock`), not
at start.

**Long turns make typing lag.** *You see:* *input query unanswered within the query budget* in the
log, and keys lost in the plugin's text field. *Why:* while a plugin's field has the keyboard,
Delight asks it questions and waits at most 20 ms; a plugin busy parsing a big response, or
redrawing on every mouse move, misses them. *Do:* keep turns short in tools with fields, and don't
redraw on mouse moves that change nothing.

**Blocking stops the plugin.** `std::thread::sleep`, a blocking socket read or waiting on a lock
holds the only thread. Await timers and tasks instead.

## Drawing

**Dark text on the dark launcher: the most common mistake.** *You see:* text you can't read in one
appearance: black text on the dark launcher, white text on a light one, a light button whose label
is white. *Why:* a colour chosen by hand (`rgb(0x333333)`, `gpui::white()`) suits one appearance only, and
text on a background of its own keeps the colour around it (the theme's `text`, which Delight
draws a plugin's text in by default) where that background needs another. *Do:* take every colour from
`delight_plugin_api::theme(cx)`, give every background the text colour made for it
(`accent_text` on `accent`, `text` on `fill` or `surface`), and look at every view in light and in
dark before you publish ([Use the theme, always](ui.md#use-the-theme-always)).

**Icons draw nothing.** *Why:* `Plugin::assets` returns `None` by default, so the plugin's GPUI has
no files to draw `svg()` from. *Do:* return an `AssetSource` of your own that serves them
([Icons and images](ui.md#icons-and-images)).

**A colour logo comes out one colour.** *Why:* GPUI's `svg()` draws a mask in the text's colour.
*Do:* render a colour SVG yourself (`resvg`, at twice the size it shows) into a GPUI `RenderImage`
and draw it with `img(…)`. An SVG's `<text>` never shows (no fonts): turn letters
into outlines, and check the gaps between letters survive.

**Rounded cards with square corners showing.** *Why:* GPUI clips children to a rectangle, not to
rounded corners, so a row's hover fill or a coloured stripe sticks out. *Do:* round the first row's
top and the last row's bottom too; give a coloured edge to each row rather than to the card.

**Clicks that land on two things.** *You see:* a click on a menu that opens over a chart also picks
the bar under it; a button over the chart starts a drag that never ends. *Why:* raw mouse handlers
on a canvas see every click in their bounds, whatever is drawn on top. *Do:* hit-test with GPUI's
hitboxes, which respect `occlude()`; count a click only when it is released over the target.

**Clipped and overflowing layouts.** *You see:* a 26 px control in a 24 px row cut off at the top;
a label wrapping out of a fixed-height row; rows squashed in the short pane (about 560 × 385); a
long name running out of its card; a 5,000-character word slowing every frame. *Do:* let heights
follow content, put `flex_shrink_0` on rows in a scrolling column, cut long text before layout and
keep labels to one line (`.truncate()`), and test with absurd content: a stress plugin found most of
these.

**The plugin stops when a page gets big.** *You see:* *display list has 120000 primitives, more
than the 100000 allowed*. *Why:* GPUI lays out and draws every element. *Do:* show the first few
hundred rows and say how many more, or use `uniform_list`.

**Animations freeze, and the launcher lags while one is on screen.** *You see:* a spinner stopped
part way round, or a view that is drawn only partly and catches up when the mouse moves or a key is
pressed (taking a screenshot made one draw); and Delight lagging while the animation shows, worst
while the mouse moves over it. A Zoho People page hit both after Refresh, whose button turned into a
turning spinner. *Why:* embedded_gpui doesn't deliver animation frames to a plugin yet. GPUI's
`with_animation` (and `window.request_animation_frame`) asks the window for its next frame, and a
plugin's window passes that request to no one: its `schedule_frame` does nothing and it sets no
frame waker, so the frame waits for whatever gives the plugin its next turn (input, a timer, an
answer from Delight). Each of those turns then redraws the animating view whole and sends it to
Delight: the Zoho page rebuilt about fifty rows on every mouse move. *Do:* no continuous animation
in a plugin for now: a still spinner, or *Refreshing…*. A view that must move can drive itself with
its own timer (`cx.background_executor().timer(…)`, then `cx.notify()`), which does wake the plugin:
at a modest rate, only while the tool is shown, and on a small view rather than the whole page.

**GPUI's desktop calls.** `cx.open_url`, `cx.open_window`, `window.prompt`, file pickers,
`cx.hide`, fonts, images on the clipboard and gradients don't work in a plugin, and
`window.viewport_size()` is the whole launcher. The [table in Drawing the UI](ui.md#what-gpui-cant-do-in-a-plugin)
says what to use instead.

## Text fields and keys

**"prepaint has not been performed on …".** *You see:* `panicked at …/gpui/src/elements/text.rs:
…: prepaint has not been performed on …`, then the plugin stops. *When:* a page with a
text field scrolls inside the plugin; or text next to a field changes (*Checking the key…*
turning into *Signed in as …* beside the key field), with no scrolling at all. *Why:* not found in
GPUI yet. *Do:* never put a field in a scroll container of your own (the Formats tools keep theirs
above the scrolling result), give a settings field a section of its own whose rows don't change,
and say what happened with a toast rather than a row beside the field.

**A field can't be made where you'd expect.** A text field needs a `Window`, and `open_tool` and
`settings_sections` have none. Make it the first time the view draws, and keep it.

**No editing keys, no ⌘V, no ←/→ in controls.** *Why:* Delight's keymap doesn't reach a plugin.
*Do:* bind the keys your fields and controls need with `cx.bind_keys`, once per plugin, in
`Plugin::new`.

**A tool can't take the keyboard.** A tool gets keys only once it has the
keyboard (→ from the list, Tab, or a click); it can't take the keyboard from the input. Handle only
the keys you use and let the rest go (`cx.stop_propagation()` for those you handled): a tool that
swallowed ← broke going back to the list, and one that handled a key in its page and also as an
action did it twice.

**Footer keys.** Only ↵, ⌘↵ and ⌥1–⌥9 exist; any other key, one the launcher already uses, or one an
earlier action took makes the action click-only, with a line in the log. A ⌥ key no action claims
types a character (`¡`) into the input: claim it with a `.hidden()` action.

**`on_focus_lost` is for clicks.** It is called for a click elsewhere in the launcher, not for the
keyboard moving or the launcher hiding: close menus there, and in `on_hidden`.

## Scrolling

**A pane that won't scroll, with its content cut off.** *You see:* a long table ends at the bottom
of the pane, its last rows missing, and the pane doesn't scroll. *Why:* in a scrolling column, a
child that clips (`overflow_hidden`, as a card with rounded corners does) may shrink below its
content's height, so it squeezes to fit the pane and there's never more to scroll. *Do:*
`.flex_shrink_0()` on each child of a scrolling column, or on a wrapper around it.

**Back at the top after switching tools.** *You see:* a tool shows its old scrolled picture for a
moment, then jumps to the top. *Why:* a hidden tool is taken out of its window, and GPUI drops the
scroll offset of every scroll area that has no handle. *Do:* a `ScrollHandle` in the view and
`.track_scroll(&self.scroll)` on every scroll area, not only the outer one.

**Drawing that depends on the scroll position doesn't update.** Scrolling alone doesn't run
`render` again: `cx.notify()` from `on_scroll_wheel` for what has to follow it.

## Settings

**A gap, or a clipped card.** *Why:* a section's height is declared, because the app can't measure
a plugin's view. *Do:* draw rows of a fixed height and compute the section's height from the same data as the
rows, avoid content
that wraps, and call `settings_changed(cx)` when the rows, a title or a footer change.

**State lost, or a form stuck one row tall.** *Why:* `settings_sections` is called again and again,
and a section keeps the first view made for its id: new views returned later are dropped. *Do:* keep
state in an entity the plugin holds, change what a section shows through it, and don't start
fetches in a section view's constructor ([Settings](settings.md)).

**Slow scrolling with many sections.** Every section is a surface redrawn on every scroll frame,
off screen too: 24 sections took 10–14 ms a frame. Keep them few.

## Windows

**GPUI's `cx.open_window` is refused** (*plugin windows mirror host windows*): open windows with
`host(cx).open_window`. At most four per plugin; the fifth fails with *Delight has no room for
another window from this plugin*, which reaches only the plugin, so log it. Opening an open key
brings it forward and drops the new view: keep a `WeakEntity` of the view to change what it shows.
`show_window` and `close_window` fail once the user has closed the window. Esc in a window is the
plugin's: close the window with `close_window` if Esc should.

## Storage and secrets

**Settings that won't load.** *You see:* *the saved settings don't fit the type the plugin reads
them as*. *Why:* the type changed. *Do:* new fields with `#[serde(default)]`, or
`unwrap_or_default()` to start over. They are at most 256 KiB.

**Secrets in the wrong places.** Tokens in settings, in the input history, in a log line, in a
`Debug` print, or sent to a URL taken from a server's reply (a build server's token once went where
the server's own answer pointed). Keep them as secrets, send them only to the host they are for,
over `https://`.

**Half-written files.** Write to a temporary name in `/data`, then rename it over the old file.

## The network

**The request cap.** *You see:* *a plugin has at most 17 requests open*. *Why:* a request was sent
at every step that looked valid while the user typed (`@exa`, `.co`, `.com`). *Do:* wait about
400 ms after typing stops, keep the request's `Task`, and replace the previous one.

**No timeout.** `host(cx).http` waits as long as the server does. Race every request with a timer
([HTTP](host-api.md#http)).

**Self-signed certificates.** *You see:* the TLS check failing for a server reached by its IP
address. *Why:* macOS checks certificates, and there is no switch to skip it. *Do:* give the server
a certificate macOS trusts, or have the user trust it in Keychain.

**The sign-in's browser tab says "The plugin didn't answer".** *Why:* the plugin dropped its
`HttpListener` as soon as the redirect arrived, before Delight had sent the browser the page.
*Do:* keep the listener half a second longer; start it before opening the sign-in page; answer
only the request with the query string (404 for `/favicon.ico`); keep it in an entity the plugin
holds, not in a view.

**Blocking sockets.** WASI's sockets work with `Network`, but a blocking read holds the only
thread: make sockets non-blocking and check them on a short timer, as the DNS built-in does. The
sandbox can't see the Mac's resolvers either: use `host(cx).dns_resolvers`.

## Commands

**A program that works in Terminal fails here.** No shell (`|`, `;` and `$(…)` are plain text), an
empty environment (no `PATH`, no `HOME`), the data folder as working folder, 60 seconds at most,
16 MiB of output per stream. A failing program is `Ok` with `success()` false. Programs outside
`/bin`, `/sbin`, `/usr/bin` and `/usr/sbin` (Homebrew's) can't be listed, and programs run as the
user: `lsof` sees only the user's processes. Prefer a host API or a crate to parsing a program's
output.

## Time and the system

**Times off by hours, and the wrong "today".** *Why:* `chrono::Local` is UTC in the sandbox. *Do:*
`Utc::now()` with a `FixedOffset` from `host(cx).utc_offset_seconds(cx)`, read where you show times
(it is 0 until Delight answers), and know it doesn't follow a daylight-saving change until the
plugin restarts.

**No process id, no environment.** A plugin can't tell its own process, or Delight's, apart from
others, and environment variables are empty: a `.env` parser that expands `$VARS` expands them to
nothing.

## Logs

**`debug!` lines never show.** A plugin logs at Info and above only. `println!` goes only to a
development build's terminal; `eprintln!` lines are logged as warnings. Use `log::info!` and up.

**Lines that look alarming and aren't.** *method call failed: entity dropped*, *overlay for unknown
surface …* and *scene for unknown surface …* after a window, a pane or a settings page closes: the
plugin's last frame arrived after its surface went. *skipping duplicate font "System Font …"* at
start.

**The plugin won't load.** Settings lists it with a warning: *Doesn't start* (`Plugin::new`
panicked, ran over its second, or the plugin imports something Delight doesn't link, such as
`wasi:http`), or *Not a plugin for this Delight* (see below). Its log lines say where.

**A call fails with *this plugin doesn't have the Network permission*.** Add the permission, with
its reason, to `#[plugin(permissions = …)]`.

## Versions and publishing

**Moving to a newer release breaks the build.** Before 1.0, a new plugin API can change what
plugins compile against. Move the tag of every Delight
crate together, fix the errors, rebuild every plugin, and tell your users which Delight
they need.

**A refused plugin.** *You see:* *Not a plugin for this Delight*, with *built for protocol X, and
this Delight runs A.0 to B* in its details. Built for a newer minor: update Delight. Another major:
rebuild the plugin.

**An installed copy hides the built-in you are working on.** In a development build, an installed
plugin with a built-in's id replaces it, and plugins start only at launch or when installed: your
rebuilt built-in "doesn't change". Delete the installed copy, and restart Delight after rebuilding.

**Links named after the plugin, not its id.** *You see:* *Couldn't use this link* for `…/logs.xml`.
*Why:* files at a location are named by id: `com.example.logs.xml`. Link to the list, or to
`<id>.xml`.

**Copies that never update.** A copy installed from a build without `update`, or from another
location, looks where its own build says. To move a location, publish a release at the old one
that names the new one.

**A version spent on a test.** A location takes each version once, and a newer one only: a test
release uses up its number. Bump the version first, and try with a location of your own.

**Updates aren't signed.** The SHA-256 proves a file matches its manifest, not who made it, and an
update that fails to start leaves no working copy behind. Publish over `https://`, from a location
only you can write, and try each build in Delight before publishing it.

**Permission changes wait.** An update that asks for a new permission, or changes the programs
`Commands` lists, isn't installed automatically: users review it. Ask for what you need from the
start.

## Testing

**Two Delights.** A development build exits with *Delight is already running (process N)* while the
installed app runs: quit it first.

**Judging speed in a debug build.** A debug Delight idles at 60% CPU and its Settings lag. Judge
performance with `cargo run --release -p delight-app`.

**Tests that pass by luck.** A test that took "the first" of a `HashMap` failed at random; a 30 ms
GPUI timer never fired in GPUI's test executor; a network test that made one request missed a leak
that forty showed. Test orders explicitly, and test limits by going past them.
