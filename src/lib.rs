use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{cmp::Ordering, fs};
use zed_extension_api::{
    self as zed, download_file, make_file_executable, set_language_server_installation_status,
    Command, DownloadedFileType, Extension, LanguageServerId, LanguageServerInstallationStatus,
    Result, Worktree,
};

// ---------------------------------------------------------------------------
// Pinned server metadata — v263.2689.0
// ---------------------------------------------------------------------------
//
// The IntelliJ LSP server is proprietary software distributed by JetBrains
// under their EULA. The extension NEVER queries third-party registries (such
// as the Open VSX API) at runtime: that pattern was rejected by the Zed
// extension registry. Instead, the server is downloaded directly from
// JetBrains' own CDN, pinned to a verified version, and only after the user has
// explicitly accepted the EULA (see the `accept_jetbrains_eula` setting).
//
// The pinned version and platform URLs live in `server-artifacts.json`,
// which was built from the official `extension/server-bundle.json` inside
// each platform's `.vsix` wrapper.  The file is embedded at compile time so
// the WASM binary carries the pin — no runtime queries needed.  A CI workflow
// (`auto-update.yml`) updates the JSON whenever JetBrains publishes a new
// build, so the pin stays current without manual maintenance.
//
// See the README section "Updating the pinned server" for how the JSON
// was originally captured and how to verify it.

use std::collections::HashMap;

#[derive(Debug, Deserialize)]
struct ServerArtifactsFile {
    version: String,
    platforms: HashMap<String, PlatformEntry>,
}

#[derive(Debug, Deserialize)]
struct PlatformEntry {
    url: String,
    #[serde(default)]
    #[allow(dead_code)]
    sha256: Option<String>,
    file_type: String,
}

fn server_artifacts() -> Result<ServerArtifactsFile, String> {
    serde_json::from_str(include_str!("../server-artifacts.json"))
        .map_err(|e| format!("failed to parse server-artifacts.json: {e}"))
}

/// Maps `zed::current_platform()` to the key used in `server-artifacts.json`.
fn platform_key(os: &zed::Os, arch: &zed::Architecture) -> &'static str {
    match (os, arch) {
        (zed::Os::Mac, zed::Architecture::Aarch64) => "mac-aarch64",
        (zed::Os::Mac, _) => "mac-x86_64",
        (zed::Os::Linux, zed::Architecture::Aarch64) => "linux-aarch64",
        (zed::Os::Linux, _) => "linux-x86_64",
        (zed::Os::Windows, zed::Architecture::Aarch64) => "windows-aarch64",
        (zed::Os::Windows, _) => "windows-x86_64",
    }
}

/// Returns `(version, download_url, file_type)` for the current platform.
fn artifact_for_platform() -> Result<(String, String, DownloadedFileType), String> {
    let data = server_artifacts()?;
    let platform = zed::current_platform();
    let key = platform_key(&platform.0, &platform.1);
    let entry = data
        .platforms
        .get(key)
        .ok_or_else(|| {
            format!(
                "IntelliJ LSP server build not available for your platform ({:?}-{:?}). \
                 You can still use the extension by downloading the server manually and \
                 setting \"server_path\". See https://blog.jetbrains.com/idea/2026/08/intellij-idea-goes-lsp/",
                platform.0, platform.1,
            )
        })?;
    let file_type = match entry.file_type.as_str() {
        "zip" => DownloadedFileType::Zip,
        "gzip-tar" => DownloadedFileType::GzipTar,
        other => {
            return Err(format!(
                "unknown file type in server-artifacts.json: {other}"
            ))
        }
    };
    Ok((data.version.clone(), entry.url.clone(), file_type))
}

// The EULA hash must match the one the real extension computes from the
// `LICENSE.txt` inside the vsix wrapper.  That file and the `EULA.txt` shipped
// inside the server archive are byte-for-byte identical for v263.2689.0
// (verified with `diff` + shasum).  If a future build ever diverges, the
// server startup will report the expected hash and the user can set the
// `eula_hash` setting to the correct value — the README documents this
// bootstrap path.  Re-verify identity on each version bump.
const SERVER_EULA_HASH: &str = "34d850193ee04897";

/// Executable names the server ships under, per platform.
const SERVER_BINARIES: [&str; 2] = ["intellij-server", "intellij-server.exe"];

/// Shown when the user has not accepted the JetBrains EULA.
const EULA_GATE_MESSAGE: &str = concat!(
    "The IntelliJ LSP server is proprietary software by JetBrains. Before it can be\n",
    "downloaded and run you must read and accept the JetBrains EULA:\n",
    "https://www.jetbrains.com/legal/docs/toolbox/user/\n",
    "(the exact license also ships as EULA.txt inside the server bundle).\n",
    "\n",
    "To accept, add this to your Zed settings.json and reload the window:\n",
    "\n",
    "{\n",
    "  \"lsp\": {\n",
    "    \"intellij-server\": {\n",
    "      \"settings\": {\n",
    "        \"accept_jetbrains_eula\": true\n",
    "      }\n",
    "    }\n",
    "  }\n",
    "}",
);

/// Shown when neither automatic nor manual mode is configured.
#[allow(dead_code)]
const MANUAL_MODE_MESSAGE: &str = concat!(
    "The IntelliJ LSP server is not installed, and this build of the extension has\n",
    "no pinned automatic download configured (it deliberately does not fetch the\n",
    "server from third-party registries such as the Open VSX API). Either:\n",
    "\n",
    "1. Download the server once (see https://blog.jetbrains.com/idea/2026/08/\n",
    "   intellij-idea-goes-lsp/) and point the extension at the extracted\n",
    "   `intellij-server` executable via the \"server_path\" setting, or\n",
    "2. Configure automatic download by setting \"server_version\" and\n",
    "   \"server_download_url\" in \"lsp\".\"intellij-server\".\"settings\" to a\n",
    "   version and JetBrains CDN URL you trust.\n",
    "\n",
    "Both options also require \"accept_jetbrains_eula\": true.",
);

/// Settings the user can configure under `lsp.intellij-server.settings`.
#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
struct IntellijServerSettings {
    /// Explicit consent to the JetBrains EULA. No download or execution
    /// happens unless this is `true`.
    accept_jetbrains_eula: bool,
    /// Path to an already-extracted `intellij-server` executable (manual mode).
    server_path: Option<String>,
    /// Override the pinned server version (automatic mode).
    server_version: Option<String>,
    /// Override the pinned JetBrains download URL (automatic mode).
    server_download_url: Option<String>,
    /// EULA acceptance hash override (advanced — see README).
    eula_hash: Option<String>,

    // --- JetBrains server settings (mapped 1:1 from the real extension) ---
    /// `intellij.additionalJvmArgs` — JVM options for the language server
    /// process (e.g. `["-Xmx4g"]`). Passed via the `IJ_JAVA_OPTIONS`
    /// environment variable, which the JetBrains launcher reads on startup.
    #[serde(rename = "intellij.additionalJvmArgs")]
    additional_jvm_args: Option<Vec<String>>,

    /// `intellij.dataSharing` — independent consent axis for telemetry.
    /// Valid values: `"full"`, `"anonymous"`, `"none"`.
    /// Defaults to `none` (no telemetry) when absent.  This is deliberately
    /// **not** coupled to `accept_jetbrains_eula`; data sharing requires its
    /// own explicit opt-in, exactly as in JetBrains' own client.
    #[serde(rename = "intellij.dataSharing")]
    data_sharing: Option<String>,

    /// `intellij.region` — region for JetBrains product terms / data
    /// processing.  Passed via `INTELLIJ_REGION` env var when set.
    #[serde(rename = "intellij.region")]
    region: Option<String>,

    /// `intellij.projects` — monorepo project entries (array of `{ type, path }`
    /// objects).  Forwarded to the server via initialization options.
    #[serde(rename = "intellij.projects")]
    projects: Option<serde_json::Value>,

    /// `intellij.buildTool` — global build tool override (e.g. `"gradle"`,
    /// `"maven"`, `"bazel"`, or `""` to disable all).  Forwarded to the server
    /// via initialization options, mapped per worktree folder.
    #[serde(rename = "intellij.buildTool")]
    build_tool: Option<String>,

    /// `intellij.jdkForSymbolResolution` — path to a JDK home for symbol
    /// resolution.  Sent as `defaultSdk` in initialization options.
    #[serde(rename = "intellij.jdkForSymbolResolution")]
    jdk_for_symbol_resolution: Option<String>,
}

fn read_settings(server_name: &str, worktree: &Worktree) -> IntellijServerSettings {
    let settings = zed::settings::LspSettings::for_worktree(server_name, worktree)
        .ok()
        .and_then(|settings| settings.settings)
        .unwrap_or_else(|| serde_json::json!({}));
    serde_json::from_value(settings).unwrap_or_default()
}

/// Normalise data-sharing value to the casing the server expects
/// (`full`, `anonymous`, `none`).  Returns `None` for `none` (which means
/// "don't set the env var at all" — the server defaults to no telemetry).
fn normalised_data_sharing(raw: Option<&str>) -> Option<&str> {
    match raw.map(|s| s.trim().to_lowercase()).as_deref() {
        Some("full") => Some("full"),
        Some("anonymous") => Some("anonymous"),
        _ => None, // `none` or anything else → omit env var → server defaults to none
    }
}

/// Returns the env-vars block for the server process, mirroring the real
/// JetBrains VSCode extension's `buildLaunchEnvironment` logic.
fn server_launch_env(settings: &IntellijServerSettings) -> Vec<(String, String)> {
    let mut env: Vec<(String, String)> = Vec::new();

    if let Some(args) = &settings.additional_jvm_args {
        if !args.is_empty() {
            env.push(("IJ_JAVA_OPTIONS".to_string(), args.join(" ")));
        }
    }

    if let Some(level) = normalised_data_sharing(settings.data_sharing.as_deref()) {
        env.push(("INTELLIJ_DATA_SHARING".to_string(), level.to_string()));
    }
    // When `none`, omitting the env var is *by design*: the server's default
    // is `dataSharing=NONE` (seen in launch logs), and the real extension
    // explicitly deletes the var when "none" was chosen.  No telemetry is sent
    // unless the user explicitly opts in to "full" or "anonymous".

    if let Some(region) = settings.region.as_deref().filter(|r| !r.is_empty()) {
        env.push(("INTELLIJ_REGION".to_string(), region.to_string()));
    }

    env
}

/// Returns the command-line arguments required to start the server. The EULA
/// is checked by recent server builds before the LSP initialize request, so
/// the accepted hash must be supplied on the command line as well as in the
/// initialization options.
fn server_launch_args(eula_hash: &str) -> Vec<String> {
    vec![
        "--stdio".to_string(),
        "--eula".to_string(),
        eula_hash.to_string(),
    ]
}

struct IntelliJLspExtension {
    cached_binary_path: Option<String>,
}

fn is_server_binary(file_name: &str) -> bool {
    SERVER_BINARIES.contains(&file_name)
}

fn server_version_dir(version: &str) -> String {
    format!("intellij-server-{}", version)
}

/// Finds the server executable below `dir`, bounded to 4 levels of nesting.
fn find_binary_in(dir: &str, depth: u32) -> Option<String> {
    if depth > 4 {
        return None;
    }
    for entry in fs::read_dir(dir).ok()?.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_binary_in(&path.to_string_lossy(), depth + 1) {
                return Some(found);
            }
        } else if is_server_binary(&entry.file_name().to_string_lossy()) {
            return Some(path.to_string_lossy().to_string());
        }
    }
    None
}

/// Returns the highest version already installed in the extension sandbox,
/// together with the path to its binary, if any.
fn find_installed_server() -> Option<(String, String)> {
    let entries = fs::read_dir(".").ok()?;
    let mut latest: Option<(String, String)> = None;
    for entry in entries.filter_map(|entry| entry.ok()) {
        let name = entry.file_name().to_string_lossy().to_string();
        let Some(version) = name.strip_prefix("intellij-server-") else {
            continue;
        };
        for candidate in [
            format!("{name}/bin/intellij-server"),
            format!("{name}/bin/intellij-server.exe"),
        ] {
            if fs::metadata(&candidate).is_ok_and(|stat| stat.is_file())
                && latest.as_ref().is_none_or(|(current, _)| {
                    compare_versions(version, current.as_str()) == Ordering::Greater
                })
            {
                latest = Some((version.to_string(), candidate));
            }
        }
    }
    latest
}

/// Numeric dot-segment comparison, so `263.2689.10` sorts after `263.2689.9`.
fn compare_versions(a: &str, b: &str) -> Ordering {
    let a_parts: Vec<&str> = a.split('.').collect();
    let b_parts: Vec<&str> = b.split('.').collect();
    for (a_part, b_part) in a_parts.iter().zip(b_parts.iter()) {
        match (a_part.parse::<u64>(), b_part.parse::<u64>()) {
            (Ok(a_num), Ok(b_num)) => match a_num.cmp(&b_num) {
                Ordering::Equal => continue,
                other => return other,
            },
            _ => match a_part.cmp(b_part) {
                Ordering::Equal => continue,
                other => return other,
            },
        }
    }
    a_parts.len().cmp(&b_parts.len())
}

/// First 16 hex chars (64 bits) of the SHA-256 digest — the EULA acceptance
/// hash the IntelliJ server expects.
fn sha256_prefix_16(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let digest = hasher.finalize();
    let mut prefix = [0u8; 8];
    prefix.copy_from_slice(&digest[..8]);
    format!("{:016x}", u64::from_be_bytes(prefix))
}

/// Full SHA-256 hex digest (64 chars) — only used by tests.
#[cfg(test)]
fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

/// Locates the `EULA.txt` bundled with an installed server, if any.
fn find_bundled_eula() -> Option<std::path::PathBuf> {
    let entries = fs::read_dir(".").ok()?;
    for entry in entries.filter_map(|entry| entry.ok()) {
        let path = entry.path();
        if path.is_dir() {
            let eula = path.join("EULA.txt");
            if eula.is_file() {
                return Some(eula);
            }
        }
    }
    None
}

impl IntelliJLspExtension {
    fn language_server_binary_path(
        &mut self,
        language_server_id: &LanguageServerId,
        settings: &IntellijServerSettings,
    ) -> Result<String> {
        // Reuse a previously resolved path if it still exists.
        if let Some(path) = &self.cached_binary_path {
            if fs::metadata(path).is_ok_and(|stat| stat.is_file()) {
                return Ok(path.clone());
            }
        }

        // Manual mode: the user downloaded the server themselves.
        // Checked before find_installed_server() so an explicit user override
        // always wins over any previously cached auto-download.
        if let Some(path) = settings.server_path.as_deref() {
            let file_name = std::path::Path::new(path)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default();
            if !is_server_binary(file_name) {
                return Err(concat!(
                    "\"server_path\" must point directly at the extracted\n",
                    "`intellij-server` executable (e.g. \"/path/to/intellij-server/bin/\n",
                    "intellij-server\"). The extension runs in a sandbox and cannot\n",
                    "extract or inspect files outside of it.",
                )
                .to_string());
            }
            self.cached_binary_path = Some(path.to_string());
            return Ok(path.to_string());
        }

        // Reuse an already-installed server from a previous session.
        if let Some((_, path)) = find_installed_server() {
            self.cached_binary_path = Some(path.clone());
            return Ok(path);
        }

        // Automatic mode: download the pinned build from JetBrains' CDN.
        let (pinned_version, url, file_type) = artifact_for_platform()?;
        let version = settings.server_version.clone().unwrap_or(pinned_version);
        let url = settings.server_download_url.clone().unwrap_or(url);

        self.download_server(language_server_id, &version, &url, file_type)
    }

    fn download_server(
        &mut self,
        language_server_id: &LanguageServerId,
        version: &str,
        url: &str,
        file_type: DownloadedFileType,
    ) -> Result<String> {
        set_language_server_installation_status(
            language_server_id,
            &LanguageServerInstallationStatus::CheckingForUpdate,
        );

        let version_dir = server_version_dir(version);
        if fs::metadata(&version_dir).is_err() {
            set_language_server_installation_status(
                language_server_id,
                &LanguageServerInstallationStatus::Downloading,
            );

            download_file(url, &version_dir, file_type).map_err(|e| {
                format!("failed to download the IntelliJ LSP server ({version}): {e}")
            })?;
        }

        let binary = find_binary_in(&version_dir, 0)
            .ok_or_else(|| format!("server binary not found after extracting {version}"))?;
        make_file_executable(&binary)
            .map_err(|e| format!("failed to make the server binary executable: {e}"))?;
        self.cached_binary_path = Some(binary.clone());
        Ok(binary)
    }

    /// Resolves the EULA acceptance hash to send to the server: an explicit
    /// user override, the hash computed from the EULA.txt bundled with the
    /// installed server, or the pinned `SERVER_EULA_HASH` as a fallback.
    fn eula_hash_for(&self, settings: &IntellijServerSettings) -> String {
        if let Some(hash) = &settings.eula_hash {
            return hash.clone();
        }
        // Compute from the EULA.txt shipped with the installed server.
        // This auto-adapts to whatever version was downloaded — no pin drift.
        let eula_path = self
            .cached_binary_path
            .as_deref()
            .and_then(|binary| std::path::Path::new(binary).parent())
            .and_then(|bin_dir| bin_dir.parent())
            .map(|server_root| server_root.join("EULA.txt"))
            .filter(|path| path.is_file())
            .or_else(find_bundled_eula);
        if let Some(path) = eula_path {
            if let Ok(data) = fs::read(path) {
                return sha256_prefix_16(&data);
            }
        }
        SERVER_EULA_HASH.to_string()
    }
}

impl Extension for IntelliJLspExtension {
    fn new() -> Self {
        Self {
            cached_binary_path: None,
        }
    }

    fn language_server_command(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<Command> {
        let settings = read_settings(language_server_id.as_ref(), worktree);

        // EULA gate: nothing is downloaded or executed without explicit consent.
        if !settings.accept_jetbrains_eula {
            set_language_server_installation_status(
                language_server_id,
                &LanguageServerInstallationStatus::Failed(EULA_GATE_MESSAGE.to_string()),
            );
            return Err(EULA_GATE_MESSAGE.to_string());
        }

        let binary_path = self.language_server_binary_path(language_server_id, &settings)?;
        let eula_hash = self.eula_hash_for(&settings);
        let env = server_launch_env(&settings);
        Ok(Command {
            command: binary_path,
            args: server_launch_args(&eula_hash),
            env,
        })
    }

    fn language_server_initialization_options(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<Option<serde_json::Value>> {
        let settings = read_settings(language_server_id.as_ref(), worktree);
        if !settings.accept_jetbrains_eula {
            return Err(EULA_GATE_MESSAGE.to_string());
        }

        let mut init = serde_json::json!({
            "eulaHash": self.eula_hash_for(&settings),
        });

        // Mirroring the real JetBrains VSCode extension's
        // `buildInitializationOptions`: forward projects, buildTools, and
        // defaultSdk verbatim so the server sees the same shape.
        if let Some(ref projects) = settings.projects {
            init["projects"] = projects.clone();
        }
        if let Some(ref build_tool) = settings.build_tool {
            // The real extension sends a per-worktree-folder URI → buildTool
            // mapping.  We have a single worktree, so we map the root path
            // to a `file://` URI.
            let uri = format!("file://{}", worktree.root_path());
            init["buildTools"] = serde_json::json!({ uri: build_tool });
        }
        if let Some(ref jdk) = settings.jdk_for_symbol_resolution {
            init["defaultSdk"] = serde_json::Value::String(jdk.clone());
        }

        Ok(Some(init))
    }
}

zed::register_extension!(IntelliJLspExtension);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_binary_in_empty_dir() {
        let dir = std::env::temp_dir().join("intellij-lsp-test-empty");
        let _ = fs::create_dir_all(&dir);
        assert!(find_binary_in(&dir.to_string_lossy(), 0).is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_find_binary_in_nested() {
        let dir = std::env::temp_dir().join("intellij-lsp-test-nested");
        let bin_dir = dir.join("nested").join("bin");
        let _ = fs::create_dir_all(&bin_dir);
        fs::write(bin_dir.join("intellij-server"), b"fake").unwrap();
        let found = find_binary_in(&dir.to_string_lossy(), 0);
        assert!(found.is_some());
        assert!(found.unwrap().ends_with("intellij-server"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_find_binary_in_finds_exe_too() {
        let dir = std::env::temp_dir().join("intellij-lsp-test-exe");
        let bin_dir = dir.join("bin");
        let _ = fs::create_dir_all(&bin_dir);
        fs::write(bin_dir.join("intellij-server.exe"), b"fake").unwrap();
        let found = find_binary_in(&dir.to_string_lossy(), 0);
        assert!(found.is_some());
        assert!(found.unwrap().ends_with("intellij-server.exe"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_is_server_binary() {
        assert!(is_server_binary("intellij-server"));
        assert!(is_server_binary("intellij-server.exe"));
        assert!(!is_server_binary("intellij-server.bat"));
        assert!(!is_server_binary("java"));
    }

    #[test]
    fn test_server_version_dir() {
        assert_eq!(server_version_dir("1.2.3"), "intellij-server-1.2.3");
    }

    #[test]
    fn test_compare_versions() {
        use Ordering::*;
        assert_eq!(compare_versions("263.2689.0", "263.2689.0"), Equal);
        assert_eq!(compare_versions("263.2689.1", "263.2689.0"), Greater);
        assert_eq!(compare_versions("263.2689.0", "263.2689.1"), Less);
        assert_eq!(compare_versions("263.2689.10", "263.2689.9"), Greater);
        assert_eq!(compare_versions("263.2689.9", "263.2689.10"), Less);
        assert_eq!(compare_versions("264.0.0", "263.2689.10"), Greater);
        assert_eq!(compare_versions("263.2689.0", "263.2689.0.1"), Less);
    }

    #[test]
    fn test_sha256_prefix_16_known_vector() {
        assert_eq!(sha256_prefix_16(b"ACCEPT_ME"), "c79ea8172fb984df");
    }

    #[test]
    fn test_sha256_prefix_16_is_hex() {
        let hash = sha256_prefix_16(b"anything");
        assert_eq!(hash.len(), 16);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_sha256_prefix_16_deterministic() {
        let data = b"same content";
        assert_eq!(sha256_prefix_16(data), sha256_prefix_16(data));
    }

    #[test]
    fn test_sha256_hex_known_vector() {
        assert_eq!(
            sha256_hex(b"ACCEPT_ME"),
            "c79ea8172fb984df90625215a6e79461e0d978040373cd2d264307434b059daf"
        );
    }

    #[test]
    fn test_settings_default_when_missing() {
        let settings: IntellijServerSettings =
            serde_json::from_value(serde_json::json!({})).unwrap();
        assert!(!settings.accept_jetbrains_eula);
        assert!(settings.server_path.is_none());
        assert!(settings.server_version.is_none());
        assert!(settings.server_download_url.is_none());
        assert!(settings.eula_hash.is_none());
        assert!(settings.additional_jvm_args.is_none());
        assert!(settings.data_sharing.is_none());
        assert!(settings.region.is_none());
    }

    #[test]
    fn test_settings_accepts_eula() {
        let settings: IntellijServerSettings =
            serde_json::from_value(serde_json::json!({ "accept_jetbrains_eula": true })).unwrap();
        assert!(settings.accept_jetbrains_eula);
    }

    #[test]
    fn test_settings_ignores_unknown_fields() {
        let settings: IntellijServerSettings = serde_json::from_value(serde_json::json!({
            "accept_jetbrains_eula": true,
            "some_future_option": "x",
        }))
        .unwrap();
        assert!(settings.accept_jetbrains_eula);
    }

    #[test]
    fn test_settings_reads_server_fields() {
        let settings: IntellijServerSettings = serde_json::from_value(serde_json::json!({
            "server_path": "/opt/intellij-server/bin/intellij-server",
            "server_version": "263.2689.0",
            "server_download_url": "https://example.com/server.sit",
            "eula_hash": "deadbeefdeadbeef",
        }))
        .unwrap();
        assert_eq!(
            settings.server_path.as_deref(),
            Some("/opt/intellij-server/bin/intellij-server")
        );
        assert_eq!(settings.server_version.as_deref(), Some("263.2689.0"));
        assert_eq!(
            settings.server_download_url.as_deref(),
            Some("https://example.com/server.sit")
        );
        assert_eq!(settings.eula_hash.as_deref(), Some("deadbeefdeadbeef"));
    }

    #[test]
    fn test_settings_reads_additional_jvm_args() {
        let settings: IntellijServerSettings = serde_json::from_value(serde_json::json!({
            "intellij.additionalJvmArgs": ["-Xmx4g", "-Dfoo=bar"],
        }))
        .unwrap();
        assert_eq!(
            settings.additional_jvm_args,
            Some(vec!["-Xmx4g".to_string(), "-Dfoo=bar".to_string()])
        );
    }

    #[test]
    fn test_normalised_data_sharing_defaults_to_none() {
        assert_eq!(normalised_data_sharing(None), None);
        assert_eq!(normalised_data_sharing(Some("none")), None);
        assert_eq!(normalised_data_sharing(Some("NONE")), None);
        assert_eq!(normalised_data_sharing(Some("")), None);
        assert_eq!(normalised_data_sharing(Some("garbage")), None);
        assert_eq!(normalised_data_sharing(Some("full")), Some("full"));
        assert_eq!(
            normalised_data_sharing(Some("anonymous")),
            Some("anonymous")
        );
    }

    #[test]
    fn test_data_sharing_never_defaults_to_sharing() {
        // None → no env var set → server gets no INTELIJ_DATA_SHARING → defaults to none
        let settings = IntellijServerSettings::default();
        let env = server_launch_env(&settings);
        let has_data_sharing = env.iter().any(|(k, _)| k == "INTELLIJ_DATA_SHARING");
        assert!(
            !has_data_sharing,
            "data sharing env must be absent by default"
        );

        // Explicit "none" → also absent
        let settings = IntellijServerSettings {
            data_sharing: Some("none".into()),
            ..Default::default()
        };
        let env = server_launch_env(&settings);
        let has_data_sharing = env.iter().any(|(k, _)| k == "INTELLIJ_DATA_SHARING");
        assert!(
            !has_data_sharing,
            "explicit none must also omit the env var"
        );
    }

    #[test]
    fn test_env_includes_jvm_args_and_region() {
        let settings = IntellijServerSettings {
            additional_jvm_args: Some(vec!["-Xmx4g".into()]),
            region: Some("EU".into()),
            ..Default::default()
        };
        let env = server_launch_env(&settings);
        assert!(env
            .iter()
            .any(|(k, v)| k == "IJ_JAVA_OPTIONS" && v == "-Xmx4g"));
        assert!(env.iter().any(|(k, v)| k == "INTELLIJ_REGION" && v == "EU"));
        assert!(!env.iter().any(|(k, _)| k == "INTELLIJ_DATA_SHARING"));
    }

    #[test]
    fn test_launch_args_include_eula_hash() {
        assert_eq!(
            server_launch_args(SERVER_EULA_HASH),
            vec!["--stdio", "--eula", SERVER_EULA_HASH]
        );
    }
}
