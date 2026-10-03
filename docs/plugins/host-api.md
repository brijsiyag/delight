# The host API

`host(cx)` is Delight, as a plugin reaches it. Every call returns at once: those with an answer
return a GPUI `Task` to await (in a task of yours) or `.detach()`. Natively, in unit tests, the
calls do nothing, and those with an answer fail.

```rust
use delight_plugin_api::host;
use gpui::prelude::*; // `detach_and_log_err` below is GPUI's `TaskExt`

host(cx).toast("Copied to clipboard", cx);
host(cx).set_secret("api-key", key, cx).detach_and_log_err(cx); // a call with an answer nobody waits for
```

## At a glance

| Call | Needs | Does |
|---|---|---|
| `toast(message, cx)` | | A message in the footer for a moment |
| `hide(cx)` | | Hides the launcher, as Esc does |
| `set_input(text, cx)` | | Makes the launcher's input `text`, so other tools take over ([The input](#the-launchers-input)) |
| `remember_input(operation, text, cx)` | | Offers `text` later as a completion and in ⌃R ([Input history](#input-history)) |
| `open_settings(cx)` | | Opens Settings on the plugin's page, from a tool that needs a key first |
| `confirm(Confirm, cx)` | | The system's alert: go ahead or cancel ([Confirming](#confirming)) |
| `open_window(options, view, cx)` | | A window of the plugin's own ([Windows](#windows)) |
| `show_window(key, cx)` | | Brings one back that went off screen with the launcher |
| `close_window(key, cx)` | | Closes one, as its user would |
| `utc_offset_seconds(cx)` | | The Mac's time zone ([Time](#time)) |
| `open_url(url, cx)` | | Opens a web page, `mailto:` or an app's link (`zoommtg:`); not `file:`. [Temporary](publishing.md#temporary-apis) |
| `settings(cx)`, `set_settings(&value, cx)`, `clear_settings(cx)` | | A small JSON value Delight keeps for the plugin ([Saving data](#saving-data)) |
| `secret(key, cx)`, `set_secret(key, value, cx)` | | Encrypted secrets ([Saving data](#saving-data)) |
| `http(request, cx)`, `listen_http(port, respond, cx)` | `Network` | HTTP, and a listener on `127.0.0.1` ([The network](#the-network)) |
| `dns_resolvers(cx)` | `Network` | The Mac's DNS setup, VPNs included ([DNS](#dns)) |
| `run(Command, cx)` | `Commands` | Runs a program it may run: one its manifest lists, or one the user gave it ([Commands](#commands)) |
| `request_permission(Permission, reason, cx)` | The one asked for | Asks the user for more of a permission the manifest asks for, a folder or a program; the plugin starts again with it ([Asking for more](#asking-for-more)) |
| `pick_folders(PickFolders, cx)` | `Files` | macOS's folder picker; the plugin starts again with what is picked. [Temporary](publishing.md#temporary-apis) |
| `save_file(name, contents, cx)` | | macOS's save panel; Delight writes the file ([Saving a file](#saving-a-file)). [Temporary](publishing.md#temporary-apis) |

Beside `host(cx)`: `delight_plugin_api::theme(cx)` is the app's theme as data, `settings_changed(cx)`
tells Delight the plugin's settings sections changed ([Settings](settings.md)), and the clipboard
is GPUI's own `cx.read_from_clipboard()` and `cx.write_to_clipboard(…)`, in every plugin.

## The launcher's input

- **`toast`** shows a short message beside a check mark, for under two seconds: for what an action
  did that the view doesn't show. Errors belong in the view, where they stay.
- **`set_input(text, cx)`** chains tools: a package's page offers its repository, and choosing it
  puts `repo <name>` in the input, where the repository tool takes over. One undoable edit, after
  which detection runs again. Delight applies it only while one of the plugin's own tools is shown.
- `toast`, `hide`, `set_input` and `open_settings` happen a moment later, once the launcher is done
  with what it is doing.

## Input history

`remember_input(operation, text, cx)` tells Delight that `text` was worth typing for that tool. It
comes back as a grey completion while the user types a start of it (Tab takes it), and in ⌃R's
search, and taking it brings this tool up.

- **Nothing is remembered unless a plugin says so.** Call it when the input found something: a
  lookup that answered, a search with results. Not on every keystroke.
- **Remember the form people retype**, exactly as they type it, with any keyword your `detect`
  needs: `npm react-dom`, not the package page's URL. A completion is offered only while what is
  typed is the start of it, matching case.
- **Remember what the user did**, not what a refresh found: remembering from a timer reorders the
  history behind their back.
- **Never remember a secret.** The history is a plain file, and the user sees it in ⌃R.
- A newer entry that starts with an older one of the same tool replaces it (`react` goes when
  `react-dom` comes). Inputs over 1,729 characters aren't kept.

## Confirming

```rust
let asked = host(cx).confirm(
    Confirm::new("Remove this host?", "Its API key is removed too.").continue_label("Remove").destructive(),
    cx,
);
cx.spawn(async move |this, cx| {
    if asked.await.unwrap_or(false) {
        this.update(cx, |this, cx| this.remove_host(cx)).ok();
    }
})
.detach();
```

macOS's own alert. `Ok(true)` if the user went ahead; `Ok(false)` if they cancelled, or it couldn't
show (another alert was open, nothing of Delight was on screen). `.destructive()` warns, and makes
Cancel the button ↵ presses. Ask before what can't be undone, and nothing else.

## Windows

Normal windows of the plugin's own, which the user moves, resizes and closes (✕, ⌘W), for what
doesn't fit the launcher: a whole JSON document beside it, say.

```rust
let view = cx.new(|cx| DocumentView::new(json, cx));
let opened = host(cx).open_window(WindowOptions::new("document", "Log entry").size(700., 480.), view, cx);
opened.detach_and_log_err(cx);
```

- **`key`** names the window: opening a key that is open brings that window forward and drops the
  new view. To change what it shows, keep a `WeakEntity` of its view and update that.
- **At most four** per plugin; another fails with *Delight has no room for another window from this
  plugin*, which only the plugin sees, so log it.
- The size is what it opens at; the user can make it as small as 280 × 160.
- **They hide with the launcher** (off screen, not closed) and come back with it, unless opened with
  `.hide_with_launcher(false)`. Only Delight hides them: `show_window` brings one back without the
  launcher, and a plugin that is done with a window closes it.
- **`close_window` closes one**, as ✕ does: what it showed is out of date (a new search), say. The
  view in it is let go; opening the key again opens a new window with the view it is given.
- In a window, Esc is the plugin's. Reinstalling or updating the plugin closes its windows.
- GPUI's own `cx.open_window` is refused in a plugin.

## Time

A plugin's clock is in **UTC**: `chrono::Local` is UTC in the sandbox. `utc_offset_seconds(cx)` is
the Mac's offset from UTC as it was when the plugin started, and 0 until Delight has answered (and
natively), so read it where you show a time, in `render`. It doesn't follow a daylight-saving change
until the plugin restarts.

```rust
use chrono::FixedOffset;

// An offset is always within a day, so `east_opt` has one.
let offset = FixedOffset::east_opt(host(cx).utc_offset_seconds(cx)).unwrap_or(FixedOffset::east_opt(0).unwrap());
let local = issued_at.with_timezone(&offset); // a chrono DateTime<Utc>, shown in the Mac's zone
```

## Saving data

Three places, all the plugin's own: no other plugin reads them. Updating the plugin keeps them;
deleting it in Settings removes them.

| | For | How |
|---|---|---|
| **Settings** | Choices and small state: a mode, an account's name, a list of hosts | `settings` / `set_settings`: a type of your own as JSON, up to 256 KiB |
| **Secrets** | API keys, tokens, passwords | `secret` / `set_secret`: strings, encrypted |
| **The data folder** | Anything larger: caches, downloaded lists, files | `std::fs` under `/data` |

What a tool shows while the plugin runs needs no saving: tools keep their state as long as the
plugin runs.

### Settings

```rust
#[derive(Serialize, Deserialize, Default)]
struct Saved {
    account: Option<String>,
    hosts: Vec<String>,
}

// Read once, at start, into an entity the plugin holds.
let saved = host(cx).settings::<Saved>(cx);
cx.spawn(async move |cx| {
    let saved = saved.await.ok().flatten().unwrap_or_default();
    // …
})
.detach();

host(cx).set_settings(&saved, cx).detach_and_log_err(cx); // replaces what was saved
host(cx).clear_settings(cx).detach_and_log_err(cx);       // forgets it
```

`settings` is `Ok(None)` when nothing is saved yet. **A saved value that no longer fits the type is
an error, not a default**: give new fields `#[serde(default)]`, or `unwrap_or_default()` to start
over. People change settings on the plugin's page: [Settings](settings.md).

### Secrets

```rust
host(cx).set_secret("api-key", key, cx).detach_and_log_err(cx); // an empty value deletes it
let key = host(cx).secret("api-key", cx);                        // Ok(None) if there is none
```

- Encrypted with AES-256-GCM, with one key in the login Keychain, and bound to the plugin and the
  name, so a value copied from elsewhere doesn't decrypt. Names are 1–256 bytes. The first secret
  Delight ever touches may ask for the Keychain.
- **Read them once at start** and keep them in memory, and **model the wait**: until the read
  answers, the plugin doesn't know whether there is a secret. Keep three states (loading, none,
  some), or the "add your API key" screen flashes every time the tool opens.
- Never log a secret, put it in the input history, show it whole, or `#[derive(Debug)]` a type that
  holds one. Send it only to the host it is for, over `https://`.
- A key in your code or embedded at build time isn't secret: anyone with the `.wasm` can read it.

### The data folder

The plugin's own folder is mounted at `/data`, which it always has, and `std::fs` works there (with
`Files` it has [other folders](#files) too):

```rust
std::fs::write("/data/services.tmp", &bytes)?;
std::fs::rename("/data/services.tmp", "/data/services.json")?; // whole, or not at all
```

Write to a temporary name, then rename it over the old file, so a plugin stopped halfway never
leaves half a file. It is also the working folder of programs the plugin runs. Natively `/data`
doesn't exist: test the logic on bytes.

## The network

With the `Network` permission, a plugin reaches the network through Delight: HTTP with the
[`http`](https://docs.rs/http) crate's types, a listener for a sign-in's redirect, gRPC, and the
Mac's DNS setup. It also has WASI's own sockets. Without the permission, each fails with an error
that says so. All of it is [temporary](publishing.md#temporary-apis): once embedded_gpui gives
plugins `wasi:http`, they use ordinary HTTP clients.

### HTTP

```rust
use delight_plugin_api::{host, http};

let request = http::Request::get("https://api.example.com/v1/status")
    .header("Authorization", format!("Bearer {token}"))
    .body(Vec::new())?;
let response = host(cx).http(request, cx).await?; // http::Response<Vec<u8>>
if !response.status().is_success() {
    return Err(anyhow!("the server answered {}", response.status()));
}
let status: Status = serde_json::from_slice(response.body())?;
```

- **Delight makes the request**, natively: HTTP/1.1 or HTTP/2 over TLS, with certificates checked by
  macOS, so a company's CA trusted on the Mac works. There is no switch to skip the check.
- The whole response arrives at once. **Redirects aren't followed**: a `3xx` comes back as it is.
  For a server that speaks only HTTP/2 over plain `http://`, set the request's version.
- **Give every request a timeout of your own**, and keep its `Task` until it answers: dropping it
  cancels the request.

  ```rust
  let (sent, timeout) = cx.update(|cx| (host(cx).http(request, cx), cx.background_executor().timer(Duration::from_secs(15))));
  match futures::future::select(pin!(sent), pin!(timeout)).await {
      Either::Left((response, _)) => response,
      Either::Right(_) => Err(anyhow!("no answer in 15 seconds")),
  }
  ```

- At most 17 requests are open at once per plugin. Fetching on every keystroke reaches that: wait
  until typing pauses, and replace the previous request rather than adding one.
- Use `delight_plugin_api::http`, the `http` crate the API uses, rather than another version.

### A sign-in's redirect

For OAuth in the browser: listen on `127.0.0.1`, open the provider's page with a redirect to the
listener, and take the code from the request that comes back.

```rust
let listener = cx
    .update(|cx| host(cx).listen_http(0, move |request, _| answer(&sender, &request), cx)) // 0: any free port
    .await?;
let redirect = format!("http://127.0.0.1:{}/", listener.port());
cx.update(|cx| host(cx).open_url(sign_in_url(&redirect), cx)).detach();
let target = received.await?; // the redirect's path and query, sent on by `answer`
```

- **Start the listener before opening the page**, and keep it and the sign-in in an entity the
  plugin holds, not in a view: a settings section's view is made again each time its page opens.
- `respond` answers every request with an `http::Response<Vec<u8>>` in a `Task`: a page saying the
  user can close the tab, for the redirect; 404 for anything else (`/favicon.ico`).
- **Keep the listener half a second after the redirect arrives.** Delight sends the browser your
  answer after your code has the request; dropping the listener first cuts the browser off with
  *The plugin didn't answer*.
- Only this Mac reaches it. At most two per plugin; a request's body is at most 1 MiB. Keep what
  the sign-in returns as secrets.

### gRPC

Turn on the plugin API's `grpc` feature, generate clients with
[`tonic-prost-build`](https://docs.rs/tonic-prost-build), and give them a channel through Delight:

```toml
delight-plugin-api = { git = "https://github.com/brijsiyag/delight.git", tag = "v0.0.6", features = ["grpc"] }
tonic = { version = "=0.14.6", default-features = false, features = ["codegen"] }
```

```rust
let channel = delight_plugin_api::network::grpc::channel("https://api.example.com".parse()?, cx);
let mut client = GreeterClient::new(channel);
let reply = client.say_hello(HelloRequest { name: "Delight".into() }).await?;
```

Unary, client-streaming, server-streaming and two-way calls all work, over HTTP/2, with the same
`tonic` version as the plugin API.

### DNS

The sandbox can't read the Mac's network configuration. `host(cx).dns_resolvers(cx)` gives the
resolvers macOS uses now, the default one first: the domain each answers for (a VPN's among them),
its servers and the search domains.

### WASI's sockets

`std::net`'s TCP and UDP sockets and name lookup work too. A plugin has no threads, so **never block
on a socket**: make it non-blocking and check it on a short timer, as the DNS built-in does
(`plugins/network/src/dns/query.rs`).

## Permissions

A plugin sees only its own data folder ([The sandbox](concepts.md#the-sandbox)). Anything more needs
a permission, declared in the manifest with the reason. People read each one, in the plugin's words
beside what it allows, before they install and on its page in Settings.

```rust
#[plugin(
    id = "com.example.ports",
    name = "Ports",
    icon = "assets/icon.svg",
    permissions = [
        Network("Looks up who owns an address"),
        Commands("Lists the programs listening on a port", programs = ["/usr/sbin/lsof"]),
    ],
)]
struct Ports;
```

| Permission | Shown as | Allows |
|---|---|---|
| `Network("why")` | *Network* | The internet and the local network, and listening on this Mac: [the network](#the-network) |
| `Commands("why", programs = [...])` | *Runs commands* | Running the listed programs, and those the user gives it, with any arguments: [commands](#commands) |
| `Files("why", read = [...], write = [...])` | *Files* | Reading the folders in `read`, writing (and reading) those in `write`, and the folders the user gives it: [files](#files) |

**The reason** is one sentence, at most 100 characters, saying what the plugin does with it:
*Reads your repositories from api.github.com with your token*, not *Needs network access*.

An update that asks for more than the installed version has (a new permission, another program
for `Commands`, another folder for `Files`, or writing where it read) doesn't install by itself:
people review it like a new install. One that asks for less installs as usual. Ask for what you need
from the start.

### Asking for more

While it runs, a plugin can ask the user for more of a permission its manifest asks for: a folder
for `Files`, a program for `Commands`. It writes what it wants as a manifest writes a permission:

```rust
use delight_plugin_api::Permission;

// Save what you need first: when the user allows it, the plugin starts again.
let none: [&str; 0] = [];
let projects = Permission::files(none, ["~/Projects"]);
let had = host(cx).request_permission(projects, "Lists your repositories", cx).await?;
let git = Permission::commands(["/opt/homebrew/bin/git"]);
let had = host(cx).request_permission(git, "Commits for you", cx).await?;
// Only here if the user declined (`false`), or the plugin has it already (`true`); an error if the
// manifest doesn't ask for that permission, what it names isn't there, or Delight can't ask
// (another alert or picker is open).
```

- **The system's alert** asks, with the plugin's `reason` under it: *Allow “Notes” to read and
  write the files in ~/Projects?*, *Allow “Git” to run /opt/homebrew/bin/git?*
- **When the user allows it, the plugin starts again** with it (a plugin has what it may do only
  from its start), and the call never returns. Save what you need (in `/data`, or settings) before
  you ask. The launcher then asks the plugin about the input again, so its tool comes back; its
  windows close.
- **What it has already** is answered `true` at once: no question, no restart. A folder inside one
  it has, with that access, is one it has.
- **The user sees what they gave** on the plugin's page in Settings, after what its manifest lists,
  and can remove it: the plugin starts again without it.
- `Network` is given only when the plugin is installed.

### Commands

```rust
use delight_plugin_api::Command;

let output = host(cx)
    .run(Command { program: "/usr/sbin/lsof".into(), args: vec!["-nP".into(), "-iTCP:8080".into()], ..Default::default() }, cx)
    .await?;
if output.success() {
    parse(&output.stdout);
}
```

- **Only its programs**: those its manifest lists, and those the user gave it ([asking for
  more](#asking-for-more)). Each is a path in full, anywhere: absolute (`/opt/homebrew/bin/git`) or
  in the home folder (`~/.cargo/bin/rg`), never a name looked up in `PATH`. With no programs
  (`Commands("why")`), it runs only those the user gives it.
- **What it runs is what is at that path**, whatever it is when it runs: a program in a folder
  others can write to can be replaced.
- **No shell**: `|`, `>` and `*` are plain characters. Write input to `stdin` instead of piping.
- An empty environment, the data folder as working folder, killed after 60 seconds; `stdout` and
  `stderr` come back as text, up to 16 MiB each.
- A program that runs and fails is `Ok`, with `success()` false; one it may not run, or that doesn't
  start, is an error.

Prefer the host API to a program (`http` rather than `curl`): a program's output is text meant for
people, and it changes between macOS versions.

### Files

With `Files`, a plugin works with folders on the Mac through `std::fs`, at their real paths. Each
time it starts, Delight puts its folders in its sandbox: those its manifest lists, and those the
user gave it since.

```rust
permissions = [Files("Lists your screenshots, and keeps notes on them", read = ["~/Desktop"], write = ["~/Notes"])],
```

```rust
let home = std::env::var("HOME")?;                         // /Users/you
for entry in std::fs::read_dir(format!("{home}/Desktop"))? {
    // …
}
```

- **The folders** are any paths, absolute or starting with `~/`, each in one list: `read` for those
  whose files it only reads, `write` for those it also writes in (write, add, remove), which it reads
  too. With no folders (`Files("why")`), it has only those the user gives it.
- **Nothing else is there.** A path outside its folders fails as a missing one would, and `..` or a
  symlink doesn't lead out of a folder.
- **`HOME`** is the user's home folder, so a plugin can build `~` paths. Without `Files` it isn't set.
- **The user sees every folder** on the plugin's page in Settings, with its access, and can remove
  one the plugin was given: the plugin starts again without it.

**More folders**, while it runs: ask for one by its path ([asking for more](#asking-for-more)), or
let the user pick:

```rust
use delight_plugin_api::{Access, PickFolders};

// Temporary: macOS's folder picker, until GPUI's own `cx.prompt_for_paths` works in a plugin.
let picked = host(cx).pick_folders(PickFolders::new().multiple().access(Access::Read).prompt("Choose"), cx).await?;
// Only here if the user cancelled (none), or picked folders the plugin has already; an error if
// Delight can't show the picker.
```

Folders picked that the plugin didn't have start it again with them, as a folder asked for does.
`Access::Write` reads too.

#### Saving a file

`save_file` shows macOS's save panel, in Downloads with the name suggested, and Delight writes the
file wherever the user chooses. It needs no permission: the plugin never gets that folder.

```rust
// Temporary, until GPUI's own `cx.prompt_for_new_path` works in a plugin.
match host(cx).save_file("report.csv", csv, cx).await? {
    Some(path) => host(cx).toast(format!("Saved {}", path.display()), cx),
    None => {} // cancelled
}
```

**Large files.** The contents go to Delight in one call, as base64 text, a third larger than the
file, while the whole file is in the plugin's memory too (512 MiB for everything a plugin holds). For a
file larger than about 100 MB, don't pass it to `save_file`:

- **Write it in a folder the user gave the plugin, and say where it is.** Ask for one to write in
  (or pick one), write the file there with `std::fs`, and show its path.
- **Or copy it there:** make it in `/data`, then `std::fs::copy` it into the folder the user asked
  for. `std::fs` writes as it goes, so only the disk limits the size.

