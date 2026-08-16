# pictkura

**English** ｜ [日本語](README.ja.md)

A small, fast desktop photo manager. It does two things well: importing from a camera
card and browsing what you already have — even when that is tens of thousands of files.

![Library](docs/images/grid.jpg)

## Goals

- **Cross-platform** — Tauri v2 (Windows / macOS / Linux)
- **Import from a card to wherever you want** — sorted into folders by capture date
- **No waiting** — a deliberately small feature set, spent on speed instead

---

## Installing

Grab the latest build from [Releases](https://github.com/Harusame64/pictkura/releases).

| Platform | File | Notes |
|---|---|---|
| **Windows 10/11 (x64)** | `pictkura_<version>_x64_<lang>.msi` | Installs per-machine, so it asks for administrator rights. Japanese and English installers are separate files |
| **Windows, no installer** | `pictkura_<version>_x64-portable.zip` | Unzip and run `pictkura.exe`. Use this if you cannot become an administrator |
| **macOS 11+ (Apple Silicon)** | `pictkura_<version>_arm64.zip` | Unzip and move `pictkura.app` wherever you like. **See the note below before the first launch** |

Windows also needs the **WebView2 runtime**, which is already present on Windows 11 and
on up-to-date Windows 10.

### Windows shows warnings because the app is not signed

pictkura is **not code-signed on Windows** (a signing certificate costs money). Nothing is
broken, but you will see the security warnings below; just proceed and the app works normally.
**SmartScreen can appear for each version you download**, while the **UAC prompt (admin
approval) appears every time you install**.

**Installing the MSI**

1. **SmartScreen** (when you open the downloaded MSI — not at download time): you may see
   "**Windows protected your PC**." Click **More info**, then the **Run anyway** button that
   appears. SmartScreen judges **each downloaded file**, so clearing one MSI does not cover the
   next release, and the same file can prompt again on another PC. Within one PC and one file,
   though, it usually stops asking after you've run it once.
2. **User Account Control (UAC)**: next, Windows asks whether to allow **an app from an unknown
   publisher** to make changes and shows **Publisher: Unknown** (because it isn't signed). Click
   **Yes**. This prompt appears **every time you install**.

**Portable ZIP**

No install is needed, but the **first time you run** the extracted `pictkura.exe`, the same
**SmartScreen** prompt ("Windows protected your PC") may appear. Click **More info** → **Run
anyway**.

**Either way: Smart App Control**

If **Smart App Control** is on — it can be, on a clean install of Windows 11 — it may block an
unsigned app **outright**, with no **Run anyway** to click. This applies to **both** the MSI and
the portable ZIP, since it judges the code, not how it reached you. There is no way around it
from the app's side: you would need a PC without SAC, or a signed build. Switching SAC off does
lift the block, but it is **one-way** — Windows cannot turn it back on without a reinstall — so
we don't suggest it just to run this app.

> Authenticode signing would reliably replace **"Publisher: Unknown"** with the publisher name,
> and makes SmartScreen fire less often — but it does **not** categorically remove it: a newly
> signed binary that hasn't built up reputation can still be flagged. (The UAC prompt itself
> still appears on every install either way.) The certificate is paid, so it is **deferred for
> v0.1**. The macOS build is likewise unsigned (below).

### macOS: the first launch needs a few extra steps

The macOS build is **not signed with an Apple Developer ID**, so Gatekeeper stops the
first launch. The app is neither damaged nor infected.

**First, move `pictkura.app` to wherever you want to keep it** (`/Applications`, say).
This is not just tidiness: launched from where it was unzipped, macOS copies the app
into a read-only temporary location and runs it from there (App Translocation).

After that, **the steps depend on your macOS version**.

#### macOS 15 (Sequoia) and later

Double-clicking shows a dialog saying macOS *"could not verify pictkura is free of
malware,"* offering only **Move to Trash** and **Done**. Nothing there lets you
continue, so:

1. Click **Done**.
2. Open **System Settings** → **Privacy & Security** and **scroll to the bottom**.
3. Find the line saying `pictkura` was blocked, and click **Open Anyway**.
4. Authenticate when asked. The app starts.

That line only appears *after* a blocked double-click, so keep the order.

#### macOS 11 to 14

**Right-click** (or control-click) `pictkura.app` → **Open** → **Open** in the dialog.

> **macOS 15 removed this bypass.** Conversely, the System Settings steps above do not
> apply to macOS 11–12, where that pane goes by a different name. Follow the section
> matching your version.

#### Any version (Terminal)

```
xattr -dr com.apple.quarantine /path/to/pictkura.app
```

---

Only the first launch needs this; afterwards it opens by double-clicking. The same
block happens whether the app ships as a `.zip` or a `.dmg` — the archive format has
nothing to do with it.

There is no Intel (x86_64) build, and no Linux build.

---

## Getting started

### 1. Add a folder to the library

On first launch, use **"フォルダを追加" (Add folder)** at the bottom of the left pane, or
pick one of the drives. Scanning starts immediately and the grid fills in by date.

> From the second launch onwards, pictkura reads the NTFS change journal and only visits
> **files that changed since last time**. The ⚡ in the top right shows how much it skipped.

### 2. Import from a card

![Import](docs/images/import.jpg)

Press **"USBから取り込み" (Import from USB)**. Pick a source folder on the left and its
photos appear on the right.

- **Include subfolders** — picks things up even when DCIM has per-date folders inside
- **Hide already imported** — uses the same check as the importer, so what you see matches
- **Destination** — "変更" (Change) at the bottom takes any folder you like
- **Sorting** — files are filed by capture date using the folder pattern you chose

Progress, time remaining, and **the photo currently being copied** are shown while it runs.
File sizes are verified after each copy.

### 3. Choose how folders are named

![Settings](docs/images/settings.jpg)

⚙ (Settings) → "取り込み先のフォルダ構成". Pick one of nine presets, or choose
**"自分で決める" (Custom)** and write your own.

| Token | Becomes |
|---|---|
| `{year}` | 4-digit year (`2026`) |
| `{month}` | 2-digit month (`08`) |
| `{day}` | 2-digit day (`13`) |
| `/` | a folder level |

The **resulting folder name** is shown as you type. `..`, absolute paths and characters
that are illegal in file names are stripped, so nothing can be written outside the
destination no matter what you type.

Where a folder name carries a date, it is written **year-first in every language**, so that
sorting by name sorts by time. If you keep the year folders yourself, point the destination
at that year's folder and pick the preset that adds only `2026-08-16/`.

### 4. Find things

Use the search box, or the command palette with **Ctrl + K**.

| Query | Meaning |
|---|---|
| `okinawa` | substring match on file name, folder name and camera |
| `camera:α7` | filter by camera (same as the left pane) |
| `folder:trip` | filter by folder name |
| `2019-08` / `2019-08-11` | filter by capture date. **A bare `2019` is treated as text**, not a date — it cannot be told apart from a file name |
| `year:2019` | filter to a single year |
| `★` | favourites only |

All conditions are ANDed. Results are ordered by capture date, newest first.

### 5. Look at them

![Viewer](docs/images/viewer.jpg)

| Key | Action |
|---|---|
| `←` `→` | previous / next |
| `Space` | slideshow (play / pause for video) |
| `I` | capture info (camera, lens, aperture, shutter, ISO, GPS) |
| `F11` | full screen |
| `Esc` | close |

The controls fade out when the mouse stops and come back when it moves. Right-click for
open / open with / show in folder / move to trash. **Deleting always goes through the
recycle bin** — pictkura never removes a file outright.

---

## Supported formats

"**Grid**" means a thumbnail appears; "**View**" means the full-size image opens.

### Photos

| Format | Grid | View | Notes |
|---|:--:|:--:|---|
| `jpg` `jpeg` | ✅ | ✅ | decoded while downscaling, so large photos stay quick |
| `png` `webp` | ✅ | ✅ | |
| `avif` | ✅ | ✅ | decoder bundled (rav1d); no OS extension required |
| `heic` `heif` `hif` | ✅ | ✅ | iPhone's default. **Pixels are decoded by the OS** (see caveats) |
| `bmp` `gif` `tif` `tiff` | ✅ | ✅ | tiff is re-wrapped as JPEG because browsers cannot draw it |
| `svg` | ✅ | ✅ | drawn from the original, so it stays sharp at any zoom |

### RAW (no demosaicing)

pictkura does **not** develop RAW files. It pulls out the display JPEG the camera wrote
for its own screen — a few milliseconds per file, with the camera's own colour rendering.

| Format | Image | Notes |
|---|:--:|---|
| `cr2` `cr3` `nef` `nrw` `arw` `sr2` `raf` `orf` `rw2` `pef` `srw` `dng` `rwl` | ✅ | includes Apple ProRAW (which is DNG) |
| `3fr` `iiq` `erf` `dcr` `kdc` `x3f` | ✅ | bodies without an embedded JPEG are assembled from the uncompressed preview |
| `mrw` (Minolta) | ⚠️ | its thumbnail has its leading bytes overwritten by design |
| Blackmagic CinemaDNG | ⚠️ | contains no preview at all |

### Video

| Format | Grid | Plays in app | Notes |
|---|:--:|:--:|---|
| `mp4` `m4v` `mov` `webm` | ✅ | ✅ | codec support comes from the OS (see HEVC below) |
| `avi` `mts` `m2ts` `mkv` `3gp` `wmv` `mpg` `mpeg` | ✅ | ❌ | thumbnail, duration and date still show; playback opens your default player |

Video thumbnails are borrowed from the OS (**Windows only** for now). Duration, dimensions
and capture time are read from the container header — not a single pixel is decoded.

---

## Caveats and known gaps

| Topic | Detail |
|---|---|
| **Files that only exist in the cloud** | pictkura **never downloads them on its own**. OneDrive "online-only" files still appear in the grid, with dimensions and capture date taken from what the OS already knows (duration is not available). The real file is fetched only when you actually look at it |
| **HEIC / HEVC need OS components** | On Windows that means "HEIF Image Extensions" (free) plus "HEVC Video Extensions" (paid) for the pixels. **No HEVC decoder is bundled** — patent licensing applies to shipping a decoder, so pictkura uses the one the OS already has |
| **Video thumbnails are Windows-only** | macOS (QuickLook) and Linux (distro thumbnailers) are not wired up yet |
| **`.m2ts` / `.avi` do not play in-app** | the browser engine cannot handle those containers. They still appear in the grid |
| **How the capture date is decided** | EXIF → OS properties → **date in the file name** → file modification time. Screenshots and saved images without EXIF still land on the right day if their name carries a date |
| **No UI to rebuild the search index** | delete `pictkura.db` in the settings folder to rebuild (thumbnails are regenerated too) |
| **No sort order for results** | always newest capture date first |
| **Imports cannot be cancelled** | once started, an import runs to completion |
| **No RTL layout** | Arabic and other right-to-left languages are not supported |

---

## Design notes

| Principle | How |
|---|---|
| Detect changes by size + mtime only | never hash a file (`scanner.rs`) |
| No base64 | image bytes stream through a `media://` custom protocol |
| Keep the DOM small | virtual scrolling with TanStack Virtual |
| Fast writes | SQLite in WAL mode, batched inserts in one transaction |
| No layout shift | width and height live in the DB, so tiles are sized before the image arrives |
| Instant first paint | the embedded EXIF thumbnail is used as-is, without re-encoding |
| No blank tiles when scrolling | the thumbnail queue prioritises whatever is on screen |
| Serve while scanning | directory scans run outside the DB lock |
| No full transfer at startup | a date→count index, fetching only the days in view |
| Don't walk the disk at startup | the NTFS USN journal supplies only what changed |
| Every search is an index seek | FTS5, with CJK expanded to bigrams for substring matching |
| Don't develop RAW | use the display JPEG the camera embedded |
| SIMD for thumbnails | `fast_image_resize` for scaling, JPEG decoded at a reduced scale (2.5× overall) |
| Don't pull files out of the cloud | ask the OS for what it already knows (measured: 3,166 files in 20s, zero network) |

---

## Layout

```
crates/pictkura-core/   core library (config / scanner / DB / import / thumbnails / search)
src-tauri/              the Tauri app (commands, media:// protocol)
ui/                     front end (React + Vite + TanStack Virtual)
docs/                   manual and screenshots
```

The development journal (`plan.md`, in Japanese) lives in a private companion
repository, so comments in the source that point at `plan.md` refer to a file that
is not in this repository.

## Building

You need a **C compiler** and **NASM** in addition to Rust.

- **C compiler** — SQLite (`rusqlite` bundled), libwebp and libjpeg-turbo are built from
  source. On the Windows GNU target that means MinGW-w64's `gcc`
- **NASM** — assembly for rav1d (AVIF) and libjpeg-turbo. **Removing it makes decoding
  six times slower**, so don't. NASM is an x86 assembler, so it is **not needed on Apple
  Silicon** — there both libraries hand their `.S` files to clang instead

```bash
# core tests
cargo test -p pictkura-core

# build the UI, then run the app
npm --prefix ui install && npm --prefix ui run build
cargo run -p pictkura --release

# measure response times at scale with synthetic data (no real images)
cargo run --release --bin bench -- --count 1000000
```

### Producing the distributables

```powershell
# Windows: MSI (ja-JP and en-US) plus the portable ZIP
pwsh tools/release.ps1
```

```bash
# macOS (Apple Silicon): pictkura.app inside a ZIP
bash tools/release-macos.sh
```

Both build the UI first and then the bundle, in that order. Building the UI first
matters: `ui/dist` is a build artifact, so skipping it would quietly ship whatever
happened to be on disk. `cargo tauri build` needs the Tauri CLI
(`cargo install tauri-cli --version "^2.0"`).

The macOS bundle target comes from `src-tauri/tauri.macos.conf.json`, which Tauri merges
over `tauri.conf.json` automatically — the default `msi` target cannot be built there.
The `.app` is **ad-hoc signed only** (that is what the linker does on arm64); there is no
Developer ID signing or notarisation step, which is why the ZIP ships with instructions
for getting past Gatekeeper.

Pushing a `v*` tag runs both of these in CI and attaches all four files to a GitHub
Release. Neither platform publishes on its own: a separate job waits for both and checks
that all four are present, so a failure on one OS cannot produce a half release.

Settings live in `%APPDATA%/dev.harusame.pictkura/pictkura.toml` on Windows, and in
`~/Library/Application Support/dev.harusame.pictkura/` on macOS.

### Regenerating the third-party licence list

After adding a dependency, rebuild the bundled `THIRD-PARTY-LICENSES.txt`:

```bash
cargo install cargo-about --features cli
cargo about generate about.hbs -o THIRD-PARTY-LICENSES.txt
node ui/scripts/licenses.mjs >> THIRD-PARTY-LICENSES.txt
```

## Quality gates

Every milestone goes through two independent reviews (Claude `/code-review` and OpenAI
Codex) and the confirmed findings are fixed before merging.

## Licence

pictkura is released under the [MIT licence](LICENSE). Copyright notices and licence texts
for the third-party software it bundles and uses are collected in
[THIRD-PARTY-LICENSES.txt](THIRD-PARTY-LICENSES.txt).

The photographs in the screenshots are **CC0 / public domain** works from Wikimedia
Commons; the list is in [docs/images/SOURCES.tsv](docs/images/SOURCES.tsv).
