# IntelliJ LSP for Zed

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

Unofficial Zed extension that brings [IntelliJ IDEA's LSP server][1] for Java &
Kotlin to the [Zed editor][2] — code completion, navigation, refactorings,
inspections, and quick-fixes for Maven, Gradle, and Bazel projects.

> **Important — licensing.** The IntelliJ LSP server is **proprietary software
> by JetBrains** (not open source). Before the extension downloads or runs it,
> you must read and accept the [JetBrains EULA][3]. This extension never
> fetches the server from third-party registries (such as the Open VSX API):
> it either downloads the pinned build directly from JetBrains' CDN, or uses a
> server you downloaded yourself. See [License](#license).

[1]: https://blog.jetbrains.com/idea/2026/08/intellij-idea-goes-lsp/
[2]: https://zed.dev
[3]: https://www.jetbrains.com/legal/docs/toolbox/user/

## Install

1. Open Zed → `Cmd+Shift+P` → `zed: extensions` → search **IntelliJ LSP**
2. **Accept the JetBrains EULA** — add this to your Zed `settings.json`
   (`~/.config/zed/settings.json` on Linux/macOS):

   ```json
   {
     "lsp": {
       "intellij-server": {
         "settings": {
           "accept_jetbrains_eula": true
         }
       }
     }
   }
   ```

3. Open a Java or Kotlin project. The server (~368 MB) is downloaded once from
   JetBrains' CDN and reused on subsequent launches.

### Install from the repository (dev extension)

1. Clone the repository:
   ```sh
   git clone https://github.com/hlucas13/intellij-lsp-zed.git
   ```
2. In Zed: `Cmd+Shift+P` → `zed: install dev extension`, select the cloned
   folder.
3. No Rust toolchain needed — the pre-built `extension.wasm` is committed to
   the repo. Re-run `git pull` and reinstall to update.

### Using a manually downloaded server

If you prefer full control, download the server from the [JetBrains
announcement][1], extract it, and point the extension at the `intellij-server`
executable:

```json
{
  "lsp": {
    "intellij-server": {
      "settings": {
        "accept_jetbrains_eula": true,
        "server_path": "/absolute/path/to/intellij-server/bin/intellij-server"
      }
    }
  }
}
```

`server_path` must point **directly at the `intellij-server` executable** (or
`intellij-server.exe` on Windows). The extension runs in a sandbox and cannot
extract archives outside of it. `server_path` takes priority over both the
sandbox cache and the pinned auto-download.

## Settings

All settings live under `lsp.intellij-server.settings` in your Zed
`settings.json`.

| Key                               | Type    | Required | Description                                                                                                                      |
| --------------------------------- | ------- | -------- | -------------------------------------------------------------------------------------------------------------------------------- |
| `accept_jetbrains_eula`           | boolean | yes      | Explicitly accept the JetBrains EULA. No download or execution happens unless this is `true`.                                    |
| `server_path`                     | string  | no       | Path to an already-extracted `intellij-server` executable (overrides auto-download).                                             |
| `server_version`                  | string  | no       | Override the pinned server version (automatic mode).                                                                             |
| `server_download_url`             | string  | no       | Override the pinned JetBrains download URL (automatic mode).                                                                     |
| `eula_hash`                       | string  | no       | EULA acceptance hash override (advanced — see Troubleshooting).                                                                  |
| `intellij.additionalJvmArgs`      | array   | no       | JVM options for the server process (e.g. `["-Xmx4g"]` to raise the 2 GB default heap).                                           |
| `intellij.dataSharing`            | string  | no       | `"full"` / `"anonymous"` / `"none"`. **Defaults to `none`** — independent consent, never inherited from `accept_jetbrains_eula`. |
| `intellij.region`                 | string  | no       | Region for JetBrains product terms / data processing.                                                                            |
| `intellij.projects`               | array   | no       | Monorepo project entries (`[{ "type": "gradle", "path": "file:///..." }]`).                                                      |
| `intellij.buildTool`              | string  | no       | Global build tool override (`"gradle"`, `"maven"`, `"bazel"`, or `""` to disable all).                                           |
| `intellij.jdkForSymbolResolution` | string  | no       | Path to a JDK home for symbol resolution.                                                                                        |

These keys are consumed by the extension and delivered to the server via
**initialization options** and **environment variables** (exactly as the real
JetBrains VS Code extension does).

## Advanced: JetBrains server settings

JetBrains' own VS Code extension delivers settings to the language server via
**initialization options** (`eulaHash`, `projects`, `buildTools`, `defaultSdk`)
and **environment variables** (`IJ_JAVA_OPTIONS`, `INTELLIJ_DATA_SHARING`,
`INTELLIJ_REGION`). This extension mirrors that behaviour 1:1 using the same
setting keys (dots included).

### Full example

A realistic `~/.config/zed/settings.json` using the IntelliJ server for Java
and Kotlin, with the EULA accepted, heap raised to 4 GB, data sharing kept
off, region set, and two monorepo sub-projects scoped for import:

```json
{
  "lsp": {
    "intellij-server": {
      "settings": {
        "accept_jetbrains_eula": true,
        "intellij.additionalJvmArgs": ["-Xmx4g", "-XX:+UseG1GC"],
        "intellij.dataSharing": "none",
        "intellij.region": "EU",
        "intellij.buildTool": "gradle",
        "intellij.projects": [
          { "type": "gradle", "path": "file:///Users/me/work/monorepo/module-a/build.gradle.kts" },
          { "type": "maven", "path": "file:///Users/me/work/monorepo/module-b/pom.xml" }
        ],
        "intellij.jdkForSymbolResolution": "/opt/homebrew/opt/openjdk@21/libexec/openjdk.jdk/Contents/Home"
      }
    }
  },
  "languages": {
    "Java": {
      "language_servers": ["intellij-server", "!jdtls"]
    },
    "Kotlin": {
      "language_servers": ["intellij-server", "!kotlin-language-server"]
    }
  }
}
```

### Individual settings explained

Other useful ones:

- `intellij.additionalJvmArgs` — `["-Xmx4g"]` (→ `IJ_JAVA_OPTIONS`; default heap is 2 GB)
- `intellij.dataSharing` — `"full"` / `"anonymous"` / `"none"` (opt-in, **defaults to `none`**; see [Data sharing](#data-sharing))
- `intellij.region` — your region for JetBrains' product terms and data processing
- `intellij.buildTool` — `"gradle"` / `"maven"` / `"bazel"` (→ `buildTools` in init options)
- `intellij.jdkForSymbolResolution` — path to a JDK home (→ `defaultSdk` in init options)

See the [official IntelliJ LSP settings documentation][4] for the full list.

## Known limitations (Zed)

- **Run and debug are not available.** JetBrains runs and debugs through a
  custom protocol (`launch.json` "IntelliJ: Launch main class" / "Attach to
  JVM"), not the Debug Adapter Protocol that Zed supports. Tests are not
  supported by JetBrains yet either.
- **Live templates and file templates** are editor-side features in VS Code;
  they are not part of the LSP protocol.
- **One backend per workspace.** Only one IntelliJ server can access a
  workspace at a time — don't use VS Code and Zed on the same folder
  simultaneously.
- **Archive integrity verification not implemented at runtime.** The
  official sha256 hash for each platform's server archive (from
  `server-bundle.json`) is stored in `server-artifacts.json` and recorded in
  `src/lib.rs` comments, but the extension does not verify it after
  downloading because `zed_extension_api` 0.7.0's `download_file` extracts
  the archive in-place and does not expose the raw bytes to the WASM sandbox
  for hashing. The archive is transported over HTTPS, and `download_file`
  reports HTTP errors; the pinned URL lives on JetBrains' own CDN. A future
  `zed_extension_api` release that supports raw download-then-extract would
  allow the extension to verify the sha256 before trusting the contents.

[4]: https://www.jetbrains.com/help/intellij-vscode/IntelliJ-lsp-settings.html
[5]: https://www.jetbrains.com/help/intellij-vscode/Project-import.html

## Disable Zed's built-in Java/Kotlin servers (optional)

The extension registers the IntelliJ server for Java and Kotlin automatically.
Zed also ships its own servers (`jdtls`, `kotlin-language-server`), so you'll
get duplicate diagnostics unless you disable them:

```json
"languages": {
  "Java": {
    "language_servers": ["intellij-server", "!jdtls"]
  },
  "Kotlin": {
    "language_servers": ["intellij-server", "!kotlin-language-server"]
  }
}
```

The `!` prefix disables the built-in server for that language.

## How It Works

1. On every launch the extension checks that you accepted the JetBrains EULA
   (`accept_jetbrains_eula`). If not, it refuses to start and prints exactly
   what to add to `settings.json` — no download, no execution.

2. It checks whether `server_path` is set; if so, it uses that binary
   immediately (explicit override wins over everything else).

3. It checks its sandbox cache for an already-installed server and reuses it.

4. If none is installed, it downloads the pinned build directly from
   JetBrains' CDN — the version and per-platform URLs come from
   `server-artifacts.json`, which is embedded in the extension at compile
   time and kept up-to-date by a biweekly CI workflow (see [Auto-update](#auto-update)).

5. The EULA acceptance hash is computed from the bundled `EULA.txt` and passed
   to the server on startup via initialization options. JetBrains settings
   (`intellij.projects`, `intellij.buildTool`, ...) are also forwarded to the
   server at startup — matching the real VS Code extension's behaviour.

6. Your project imports (Maven/Gradle/Bazel) and language features activate.

Cached versions are reused on subsequent launches.

## Auto-update

The pinned server version and download URLs live in
[`server-artifacts.json`](server-artifacts.json) — one JSON file with the
current version and a per-platform entry (URL + sha256 + archive type) for
all 6 supported platforms (macOS x86_64/ARM64, Linux x86_64/ARM64, Windows
x86_64/ARM64). This file is embedded in the extension at compile time — no
runtime queries.

Two CI workflow files keep the pin current, both running on the maintainer's
repository, never from an end-user's machine:

- **Upstream build detection + registry propagation** (`auto-update.yml`) —
  runs on the 1st and 15th of each month (a stable ~13–17 day interval). It
  queries the Open VSX API once to check whether JetBrains has published a new
  vsix. If a new version is found, it downloads the vsix package for all 6
  supported platforms from `openvsx.eclipsecontent.org`, extracts
  `extension/server-bundle.json` from each, rebuilds `server-artifacts.json`,
  rebuilds the WASM, bumps the version, commits, and pushes. It then
  propagates the update to the `zed-industries/extensions` registry by bumping
  the extension's git submodule and updating the `version` field in
  `extensions.toml` — following Zed's own documented extension-update process.
  The build-detection step is real (if infrequent, single-maintainer and
  non-scalable) traffic against Open VSX's infrastructure — it runs from
  GitHub's CI, never from an end user's machine, never triggered by an
  install, and its volume does not grow with adoption of the extension. The
  registry-propagation step involves **no Open VSX traffic at all** — it is
  pure Git operations against the extensions repository.
- **CDN health check** (`monitor.yml`) — also runs on the 1st and 15th of each
  month. It verifies the pinned JetBrains CDN URLs are still reachable. If
  the check fails, it opens an `extension-broken` issue on the extension's own
  repository.

JetBrains ships preview builds roughly every 2 weeks, and each build stays
valid for 30 days before it expires — so the 1st/15th schedule always catches
new builds with at least 13 days of margin before the previous build could
expire.

The extension itself never touches any registry API: the pin is static and
pre-committed.

## Evaluation & License

- During the preview the server is **free** — each build is valid for
  **30 days** from its release date
- After the preview, an IntelliJ IDEA Ultimate subscription will be required
- If the server stops working after ~30 days, install a newer build (clear the
  extension's cache; see Troubleshooting)

### Data sharing

JetBrains' own clients (VS Code, Cursor) additionally ask users to accept a
**data-sharing policy** and choose a region after installing the extension.
This extension keeps data sharing **disabled by default**: the server runs with
`dataSharing=NONE` and no telemetry is sent to JetBrains. If you want to opt
into telemetry, set `intellij.dataSharing` to `"full"` or `"anonymous"` — this
is a completely separate decision from EULA acceptance.

## Troubleshooting

| Problem                                       | Fix                                                                                                                                                                                                                     |
| --------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| "you must read and accept the JetBrains EULA" | Add `"accept_jetbrains_eula": true` under `lsp.intellij-server.settings` (see [Install](#install)) and reload the window.                                                                                               |
| "Bundled license agreement is not accepted"   | Reload Zed after setting `accept_jetbrains_eula` to `true`. The extension computes the hash and passes it to the server through `--eula`; if you run the server from a manual `server_path` and the hash cannot be read automatically, copy the expected hash printed by the server into the `eula_hash` setting. |
| Server won't start / evaluation expired       | Clear the server cache: `rm -rf ~/Library/Application\ Support/Zed/extensions/work/intellij-lsp-zed` (Linux: `~/.local/share/zed/extensions/work/`, Windows: `%LOCALAPPDATA%\Zed\extensions\work\`), then reload Zed    |
| Download fails                                | Check your internet connection, then retry — the extension resumes cleanly                                                                                                                                              |
| Duplicate diagnostics                         | Add the `language_servers` config above to disable Zed's built-ins                                                                                                                                                      |

## Development

```sh
# Build the extension (requires Rust + wasm32-wasip2 target)
cargo build --release --target wasm32-wasip2

# Run tests
cargo test

# Install locally (macOS; adjust path on Linux/Windows)
cp target/wasm32-wasip2/release/intellij_lsp_zed.wasm extension.wasm
cp extension.wasm extension.toml \
  ~/Library/Application\ Support/Zed/extensions/installed/intellij-lsp-zed/
```

### Project structure

| Path                                | Purpose                                                                                                    |
| ----------------------------------- | ---------------------------------------------------------------------------------------------------------- |
| `src/lib.rs`                        | Extension entry point — EULA gate, binary resolution, download/launch, init options                        |
| `server-artifacts.json`             | Pinned server version + per-platform download URLs (source of truth, updated by CI)                        |
| `extension.toml`                    | Zed extension manifest                                                                                     |
| `extension.wasm`                    | Pre-built WASM binary (so users don't need Rust)                                                           |
| `scripts/update-artifacts.py`       | CI helper — downloads all platform vsixes, extracts `server-bundle.json`, rebuilds `server-artifacts.json` |
| `scripts/bump-version.py`           | CI helper — bumps the patch version in `extension.toml`, `Cargo.toml`, and `package.json`                  |
| `.github/workflows/auto-update.yml` | Biweekly CI that detects new JetBrains builds and auto-updates the pin + releases                          |
| `.github/workflows/monitor.yml`     | Biweekly CI health check — verifies the pinned CDN URLs are reachable                                      |
| `.github/workflows/ci.yml`          | Push/PR CI — fmt, clippy, tests, wasm build                                                                |

### Updating the pinned server (manual, if CI can't)

The auto-update workflow handles this 99% of the time. If you need to do it
manually:

1. Download the new `JetBrains.intellij-server` vsix for each platform from
   [Open VSX](https://open-vsx.org/extension/JetBrains/intellij-server) in a
   browser (one manual download per platform — normal end-user usage).

2. Extract each vsix and read `extension/server-bundle.json`: it contains
   the real JetBrains CDN `url`, `version`, and `sha256` for that platform.

3. Run `python3 scripts/update-artifacts.py <vsix-version>` to rebuild
   `server-artifacts.json` from the downloaded vsixes.

4. Verify that the `EULA.txt` inside the new server bundle is byte-for-byte
   identical to the `LICENSE.txt` inside the vsix wrapper (they were for
   v263.2689.0 — re-check on every bump to avoid hash drift).

5. Rebuild the WASM: `cargo build --release --target wasm32-wasip2`, copy
   `extension.wasm`, bump versions, and release.

## Requirements

- **macOS**, **Linux**, or **Windows**
- **Zed** editor (any recent version)
- Internet connection on first launch (automatic mode only)

## Caveats

- **Library sources**: `Cmd+Click` into JDK/Spring classes won't open their
  source (Zed doesn't support `jar://` URIs yet). Navigation within your own
  code works fine.
- **First launch**: the initial project import can take a minute or two on
  large projects.

## License

The extension code is [MIT](LICENSE).

The IntelliJ LSP server is proprietary software by JetBrains, subject to its
own [EULA][3]. It is **not** bundled with or redistributed by this extension —
it is downloaded from JetBrains after you explicitly accept the EULA, or used
from a path you provide. See [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
