# ProjectRem

A local voice assistant front-end: a **Rust + egui** desktop app that drives a
**Python sidecar** process. Accounts, connections and bots live in a
**Cloudflare Worker**; models are downloaded straight from Hugging Face.

```
┌─────────────┬────────────────────────────────────────────────┐
│  sidebar    │  page header (title + one-line description)     │
│   236 px    ├────────────────────────────────────────────────┤
│  Main       │                                                │
│  Models     │  scrolling page body                           │
│  Account    │                                                │
│  Settings   ├────────────────────────────────────────────────┤
│  Plugins    │  action bar (page-specific primary actions)    │
│  ─────────  │                                                │
│  account    │                                                │
└─────────────┴────────────────────────────────────────────────┘
```

Five tabs, a plain desktop layout, one accent colour and [Lucide](https://lucide.dev)
icons. No server rail, no channel list, no message composer.

## Quick start

Double click **`Start-Test.bat`**. It creates a `.venv`, installs the sidecar
dependencies the first time and then runs the GUI with `cargo run`.

| Script | What it does |
| --- | --- |
| `Start-Test.bat` | venv + `cargo run`, for development |
| `build.bat` | bump the version, run the tests, release build, package to `Out\<version>\`, then tag and publish a **GitHub release** |
| `push.bat` | deploy the **Cloudflare Worker**, then push the **repository code** to GitHub |

`build.bat` accepts `--no-tests`, `--no-release` and `--no-pause`.
`push.bat` accepts `--no-worker`, `--no-git` and `--no-pause`.

## The tabs

| Tab | What it does |
| --- | --- |
| **Main** | A welcome card and a live status list: version, server URL, Python sidecar, model readiness. |
| **Models** | LLM / TTS / STT cards, each with a **Download** button and a progress bar, plus **Download missing** and **Open folder** in the action bar. |
| **Account** | Signed out: a **Sign in / Register** form. Signed in: the profile, the **profile picture** picker, the **connections** list and the **Hugging Face token** field. |
| **Settings** | Audio input/output device pickers (enumerated by the sidecar), the software language, and an About panel with the version, server URL and data folder. |
| **Plugins** | The three bundled plugins with on/off switches, and **Import plugin** in the action bar. |

### Accounts

Registration and sign-in are handled by the Worker, not locally — there is no
account database in the app and no password is ever stored on this machine.
The session token is kept in `%APPDATA%\ProjectRem\session.json` so the app
opens straight on your profile.

Connecting **Discord**, **Hugging Face** or **GitHub** opens the provider's OAuth
page in the browser. The app listens on `http://localhost:53630/callback/<short>`,
receives the redirect, and hands the code to the Worker, which performs the
token exchange. **The OAuth client secrets never leave the Worker.**

### Profile picture

Once you are connected, each service offers its avatar as a candidate. Pick one,
or pick **None** to clear it. The choice is saved to your account, so it follows
you to any machine you sign in on.

### Hugging Face token

Model downloads use an anonymous quota unless you supply your own token. Paste it
into **Account → Hugging Face token** and it is stored on the Worker. The app can
only read *whether* a token exists and when it was saved — the token itself is
write-only over the API.

### The models

| Kind | Model | Repository | Size |
| --- | --- | --- | --- |
| LLM | Qwen3.5 2B (GGUF) | [unsloth/Qwen3.5-2B-GGUF](https://huggingface.co/unsloth/Qwen3.5-2B-GGUF) | ~1.22 GB |
| TTS | Kitten TTS 2 | [KittenML/kitten-tts-2](https://huggingface.co/KittenML/kitten-tts-2) | ~0.55 GB |
| STT | Whisper large-v3 turbo | [openai/whisper-large-v3-turbo](https://huggingface.co/openai/whisper-large-v3-turbo) | ~1.55 GB |

Files land in `%APPDATA%\ProjectRem\models\<model-id>\`. Partial downloads are
written as `.part` files, so an interrupted download is never mistaken for a
finished one.

## Layout

```
ProjectRem/
├── VERSION                  single source of truth for the version
├── Cargo.toml               workspace root (no version field on purpose)
├── build.bat                version bump, tests, build, package, GitHub release
├── push.bat                 deploy the Worker, push the repository code
├── Start-Test.bat           venv + cargo run
├── .env.example             template for the credentials the scripts need
├── docs/
│   ├── VERSIONING.md        the version scheme
│   └── WORKER.md            the Worker API and its KV layout
├── main/                    Rust / egui GUI
│   ├── build.rs             reads VERSION, bakes it into the binary
│   └── src/
│       ├── main.rs          eframe entry point
│       ├── app.rs           the shell: panels, state, background workers
│       ├── views.rs         one renderer per page (pure data in, action out)
│       ├── widgets.rs       cards, buttons, switches, avatars
│       ├── lucide.rs        Lucide SVG parser + painter
│       ├── icons_data.rs    generated: 61 real Lucide icons
│       ├── theme.rs         palette, metrics and global egui style
│       ├── fonts.rs         system font fallback for CJK
│       ├── i18n.rs          English / German / Japanese strings
│       ├── settings.rs      settings.json
│       ├── account.rs       Worker API client + session persistence
│       ├── oauth.rs         the localhost callback listener
│       ├── models.rs        model catalog (add models here)
│       ├── downloader.rs    background downloads with progress
│       ├── plugins.rs       plugin manifest + import
│       ├── sidecar.rs       Rust side of the Python bridge
│       ├── paths.rs         %APPDATA%\ProjectRem\...
│       └── version.rs       version constants
├── sidecar/
│   ├── main.py              JSON-lines RPC over stdin/stdout
│   └── requirements.txt     optional: sounddevice
├── tools/
│   └── fetch-lucide.mjs     regenerates icons_data.rs from Lucide
├── worker/                  Cloudflare Worker (accounts, connections, bots)
│   ├── src/index.js
│   ├── schema.md            KV key layout
│   └── wrangler.toml
└── Out/                     build.bat output, one folder per version
```

### How the UI is put together

The shell and the pages are deliberately split:

* `app.rs` owns the state and runs the background threads (Worker calls, model
  downloads, the OAuth callback). It **gathers** a small copyable struct per
  page and then hands it to a renderer.
* `views.rs` **renders** that struct and reports at most one pending action.
* `widgets.rs` holds the reusable pieces. Nothing in the UI blocks: every
  network call happens on a thread.

This keeps the borrow checker out of the way (a page never holds `&mut self`
while laying out) and makes each page easy to read on its own.

### Icons

The Lucide SVGs in `icons_data.rs` are generated:

```sh
node tools/fetch-lucide.mjs
```

They are parsed once at startup into polylines and stroked with the egui
painter, so there is no image dependency and no async texture load per icon. The
parser is covered by tests in `lucide.rs` — SVG relative commands, packed
numbers and arc flattening are all easy to get subtly wrong and obvious to see.

## The Python sidecar

The GUI starts `sidecar/main.py` as a child process and talks to it with
newline-delimited JSON:

```jsonc
->  {"id": 1, "cmd": "ping"}
<-  {"id": 1, "ok": true, "pong": true, "sidecar_version": "0.0.1"}

->  {"id": 2, "cmd": "audio_devices"}
<-  {"id": 2, "ok": true, "inputs": [...], "outputs": [...], "backend": "sounddevice"}
```

Commands: `ping`, `audio_devices`, `info`, `shutdown`.

Set `PROJECTREM_PYTHON` to override the interpreter.

## The Cloudflare Worker

`projectrem-api` stores accounts, sessions, connections, Hugging Face tokens and
bot definitions in a single KV namespace. It is the only component that knows an
OAuth client secret, which is why connecting a service never needs one on the
client side.

`push.bat` deploys it. The API and the KV key layout are documented in
[docs/WORKER.md](docs/WORKER.md).

## Credentials

No credential is committed. `.env` holds the tokens the *scripts* need (the
Cloudflare API token, the KV namespace id, the GitHub token) and is gitignored;
`.env.example` documents each one. The OAuth client secrets are Worker secrets,
set with `wrangler secret put <NAME>` and never present in the repository.

`push.bat` refuses to run if a `.env` file is staged.

## Versioning

The `VERSION` file at the repository root is the only place a version number is
written down. `main/build.rs` reads it at compile time, so the window title,
Settings → About and the `--version` argument handed to the Python sidecar can
never drift apart.

`build.bat` bumps it before every build using this scheme:

```
0.0.0 → 0.0.1 → … → 0.0.10 → 0.1.0 → … → 0.10.10 → 1.0.0 → …
```

major is unbounded, minor and patch run from 0 to 10. See
[docs/VERSIONING.md](docs/VERSIONING.md).

## Extending

* **More models or API providers** — add an entry to `catalog()` in
  `main/src/models.rs`. `ModelSource::Api` already exists for hosted models and
  needs no download.
* **More languages** — add a variant to `Language` and the three tables in
  `main/src/i18n.rs`. A test asserts every table covers every variant.
* **New plugins** — a folder with a `plugin.json`
  (`{"name": …, "version": …, "author": …, "description": …}`) imported through
  the Plugins tab.
* **New Worker endpoints** — see [docs/WORKER.md](docs/WORKER.md).

<!-- push.bat smoke test -->
