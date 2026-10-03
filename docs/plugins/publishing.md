# Publishing and versions

There are two ways to get a plugin to people: hand them the `.wasm`, or publish it at a URL, from
which Delight installs it and keeps it up to date.

## Hand over the file

People install a `.wasm` from Settings (**Install Plugin**) or drop it on Delight's menu bar icon.
It is updated only when they install a newer file the same way.

## Publish at a location

A **location** is a URL that holds the plugins' files: a GitHub release's download address, a
folder on any web server. For each plugin it holds two files, named by its id:

| File | What | At most |
|---|---|---|
| `<id>.wasm` | The plugin | 64 MiB |
| `<id>.xml` | Its manifest there: which plugin and version the file is, and its SHA-256 | 1 MiB |

```xml
<?xml version="1.0" encoding="UTF-8"?>
<plugin id="com.example.weather" version="1.2.0">
  <name>Weather</name>
  <description>The forecast for any city.</description>
  <sha256>…the .wasm's SHA-256 (shasum -a 256): 64 lowercase hex digits…</sha256>
</plugin>
```

`id` and `version` are the plugin's own, and the `.wasm` must say the same. `name` and
`description` are shown when people pick what to install.

A location can also have a **list** of its plugins, in the same form, under any file name:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<plugins>
  <plugin id="com.example.calendar" version="1.0.3">…</plugin>
  <plugin id="com.example.weather" version="1.2.0">…</plugin>
</plugins>
```

`delight_manifest::Release` and `PluginList` (`delight-manifest`, feature `files`, at the plugin
API's tag) read and write these files, so a release script never writes XML by hand.

### Say where the plugin is published

```rust
#[plugin(id = "com.example.weather", name = "Weather", icon = "assets/icon.svg",
         update = "https://github.com/example/delight-plugins/releases/latest/download")]
```

Copies installed from a build with `update` look there for updates. A copy installed before you
added it doesn't know where to look: its users install the new build once. To move a location,
publish a release at the old one that names the new one.

### How people install from it

Settings → the arrow beside **Install Plugin** → **From a Link…**, with a link to an XML file at the
location: the list (every plugin in it, to pick from) or one plugin's `<id>.xml`. Share the list:
`https://github.com/example/delight-plugins/releases/latest/download/plugins.xml`.

### How updates reach people

- Delight reads `<location>/<id>.xml` 30 seconds after it starts, then every 24 hours, and when the
  user clicks ↻ on the plugin's page.
- It downloads `<id>.wasm` only when the version there is newer (by SemVer), and checks the file's
  SHA-256, id and version before anything of it runs.
- **An update that asks for nothing new installs by itself** (no new permission, program or
  folder; fewer is fine), keeping the plugin's data, settings and secrets, unless *Update
  automatically* is off on its page. It waits while one of the plugin's tools is shown or its window
  is open.
- **One that asks for more** is shown like a new install, and waits for a yes.

### On GitHub

With a location of `https://github.com/<owner>/<repo>/releases/latest/download`, each release is a
GitHub release with the files attached (`gh release create v1.2.0 dist/*.wasm dist/*.xml`).
**Attach every plugin to every release**: `latest/download` points at the newest release only, so a
plugin left out can't be found. A draft or pre-release isn't *latest*.

### Before each release

- [ ] Bump the crate's `version`: Delight takes only a newer one, and a location each version once.
- [ ] Build in release mode, against a release's tag, with any `[patch]` to a local checkout
      removed.
- [ ] Write each `<id>.xml` from the very `.wasm` you publish (its SHA-256 changes with every
      build), and upload the `.wasm` before the `.xml` that names it.
- [ ] Install from the link yourself, and check an older copy sees the update.

Updates aren't signed: the SHA-256 proves a file matches its manifest, not who made it. Publish over
`https://`, from a location only you can write.

## Versions and compatibility

### Which Delight runs a plugin

**Delight 0.0.7 runs plugin API 0.5** (`0.5.0`, the version of the crates plugins build against).
A plugin carries the plugin API version it was built against in its `.wasm`; one built for a newer
plugin API than Delight's is refused (*Not a plugin for this Delight*) until Delight is updated.

**Before 1.0, a new minor version can break plugins**: the release notes say when they need
rebuilding. From 1.0 on, a minor only adds, and anything that breaks plugins is a new major.

### Moving to a newer release

1. Find the release's tag, `vX.Y.Z`: `git ls-remote --tags https://github.com/brijsiyag/delight`
   lists them.
2. Put it in the `tag` of every Delight crate you use: the same one for all of them.
3. Build, fix what changed, run the tests, try the plugin in that Delight. Commit `Cargo.lock`.
4. Bump the plugin's version and publish it, saying which Delight it needs.

### Why git, and not crates.io

Nothing Delight builds on is released yet, so everything is taken from git, pinned:

| Crate | From | Pinned |
|---|---|---|
| `delight-plugin-api`, `delight-manifest` | this repository | a release's tag, and the commit in `Cargo.lock` |
| `gpui` | Zed's `gpui-multi-root-embedded-rebased` branch, `version = "=0.2.2"` | the commit in `Cargo.lock` |
| `embedded_gpui`, which runs a plugin's GPUI inside Delight | for now Delight's fork, `github.com/brijsiyag/embedded_gpui` (branch `delight`): upstream's `surfaces-as-roots` and two changes Delight needs, until upstream has them | by the plugin API, so a plugin never names it |

Component libraries built on another GPUI (such as
[gpui-component](https://github.com/longbridge/gpui-kit)) can't be used yet: their elements can't go
into a plugin's views. Once embedded GPUI is released, Delight and its plugin API move to released
crates, plugins depend on versions instead of commits, and every plugin is rebuilt.

### Temporary APIs

Some of what the host API offers stands in for what embedded_gpui or WASI will give plugins
directly. When they do, these are deprecated, then removed, and **plugins that use them need
changing**:

| API | Goes when | Plugins then |
|---|---|---|
| **The network**: `host(cx).http`, `listen_http`, `network::grpc::channel` | embedded_gpui links `wasi:http` | Use ordinary HTTP and gRPC clients |
| **Opening a URL**: `host(cx).open_url` | embedded_gpui passes GPUI's own `cx.open_url` through | Call `cx.open_url` |
| **The clipboard, refreshed early**: nothing to call | embedded_gpui sends the change before the key | Nothing changes |
| **Picking folders**: `host(cx).pick_folders` | embedded_gpui passes GPUI's own `cx.prompt_for_paths` through | Call `cx.prompt_for_paths` |
| **Saving a file**: `host(cx).save_file` | embedded_gpui passes GPUI's own `cx.prompt_for_new_path` through | Call `cx.prompt_for_new_path` and write the file |

Keep each of these calls in one place in your plugin, so the change is small when it comes.
