// Opt-in H.264-capable CEF runtime ("codec pack").
//
// TwintailLauncher releases bundle the official CEF binaries, which exclude
// proprietary codecs (H.264/AAC) for patent-licensing reasons - so .mp4 live
// backgrounds (WuWa/PGR/Endfield CDNs) fall back to static art. Arch Linux
// builds the same CEF versions from source with proprietary_codecs=true and
// keeps every version in an immutable, publicly served archive.
//
// At the user's explicit request we download that package (plus libjpeg-turbo,
// whose libjpeg.so.8 soname Arch's libcef links against) onto the user's
// machine, verify pinned SHA-256 hashes, and stage the runtime files in a
// per-CEF-version directory under the app data dir. On the next startup
// maybe_reexec_with_pack() re-execs the launcher with LD_LIBRARY_PATH pointing
// at that directory: LD_LIBRARY_PATH takes precedence over the binary's
// $ORIGIN RUNPATH, and CEF resolves its resources (pak files, locales, V8
// snapshot) relative to the loaded libcef module, so the whole codec-enabled
// runtime is used without modifying the installation. The launcher itself
// never distributes a decoder - each user obtains it directly from Arch.
//
// The frontend needs no coordination: imagePreloader's canPlayType probe
// detects H.264 support at startup and enables .mp4 backgrounds automatically.

#[cfg(target_os = "linux")]
mod imp {
    use std::fs;
    use std::io::{Read, Write};
    use std::path::{Path, PathBuf};

    /// Must match the CEF version pinned via cef-dll-sys in Cargo.toml
    /// (currently `cef = "=148.0.0"` -> CEF binary 147.0.10). If the pin moves,
    /// update VERSION + both package entries; a stale pack dir is then simply
    /// ignored by version mismatch and the status reports "not installed".
    pub const VERSION: &str = "147.0.10";

    // archive.archlinux.org is immutable: a released package file never changes,
    // which is what makes hash pinning sound.
    const CEF_PKG_URL: &str = "https://archive.archlinux.org/packages/c/cef/cef-147.0.10-1-x86_64.pkg.tar.zst";
    const CEF_PKG_SHA256: &str = "4bbbf900f6fcdb4993dafe1531c77039fd0bbda9b476172d4a4751f30bcbc65b";
    const JPEG_PKG_URL: &str = "https://archive.archlinux.org/packages/l/libjpeg-turbo/libjpeg-turbo-3.2.0-2-x86_64.pkg.tar.zst";
    const JPEG_PKG_SHA256: &str = "14e743e58bbab35665d3e59ea71bd411f81a01d995e0101442f5f4c82c9de0ab";

    pub fn pack_dir() -> Option<PathBuf> {
        // Mirrors tauri's app_data_dir for identifier "twintaillauncher"; resolved
        // manually because maybe_reexec_with_pack runs before the tauri app exists.
        let data = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))?;
        Some(data.join("twintaillauncher").join("cef-codec-pack").join(VERSION))
    }

    pub fn installed() -> bool {
        pack_dir().is_some_and(|d| d.join("libcef.so").is_file())
    }

    pub fn active() -> bool {
        std::env::var_os("TTL_CEF_PACK").is_some()
    }

    /// Called first thing in run(). Re-execs the launcher with the codec pack's
    /// directory prepended to LD_LIBRARY_PATH. No-ops when already re-executed
    /// (marker env inherited, including by CEF helper subprocesses) or when no
    /// pack is staged. If the pack dir was deleted while the marker is set, the
    /// dynamic linker just falls through to the $ORIGIN RUNPATH -> stock CEF.
    pub fn maybe_reexec_with_pack() {
        if active() { return; }
        // Belt and braces: CEF helper processes must never re-exec.
        if std::env::args().any(|a| a.starts_with("--type=")) { return; }
        let Some(dir) = pack_dir() else { return };
        if !dir.join("libcef.so").is_file() { return; }
        let Ok(exe) = std::env::current_exe() else { return };
        let ld_path = match std::env::var("LD_LIBRARY_PATH") {
            Ok(existing) if !existing.is_empty() => format!("{}:{existing}", dir.display()),
            _ => dir.display().to_string(),
        };
        use std::os::unix::process::CommandExt;
        let err = std::process::Command::new(exe)
            .args(std::env::args_os().skip(1))
            .env("LD_LIBRARY_PATH", ld_path)
            .env("TTL_CEF_PACK", "1")
            .exec();
        // exec only returns on failure; logger isn't initialized yet.
        eprintln!("codec pack: re-exec failed ({err}), continuing with stock CEF");
    }

    async fn download_verified(url: &str, expected_sha256: &str, dest: &Path) -> Result<(), String> {
        use sha2::Digest;
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(30))
            .read_timeout(std::time::Duration::from_secs(60))
            .build()
            .map_err(|e| e.to_string())?;
        let mut response = client.get(url).send().await
            .and_then(|r| r.error_for_status())
            .map_err(|e| format!("Download failed: {e}"))?;
        let mut file = fs::File::create(dest).map_err(|e| format!("Failed to write download: {e}"))?;
        let mut hasher = sha2::Sha256::new();
        while let Some(chunk) = response.chunk().await.map_err(|e| format!("Download failed: {e}"))? {
            hasher.update(&chunk);
            file.write_all(&chunk).map_err(|e| format!("Failed to write download: {e}"))?;
        }
        let actual = format!("{:x}", hasher.finalize());
        if actual != expected_sha256 {
            let _ = fs::remove_file(dest);
            return Err(format!("Checksum mismatch for {url}: expected {expected_sha256}, got {actual}"));
        }
        Ok(())
    }

    /// Extract entries under `prefix` from a zstd-compressed Arch package into
    /// `dest`, flattening the prefix. Path safety: only relative paths derived
    /// from the entry's own name after a verified starts_with(prefix) strip.
    fn extract_prefixed(archive: &Path, prefix: &str, dest: &Path) -> Result<usize, String> {
        let file = fs::File::open(archive).map_err(|e| e.to_string())?;
        let decoder = zstd::stream::read::Decoder::new(file).map_err(|e| e.to_string())?;
        let mut tar = tar::Archive::new(decoder);
        let mut extracted = 0usize;
        for entry in tar.entries().map_err(|e| e.to_string())? {
            let mut entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path().map_err(|e| e.to_string())?.into_owned();
            let Ok(rel) = path.strip_prefix(prefix) else { continue };
            if rel.as_os_str().is_empty() { continue; }
            // Reject anything that could escape dest (Arch packages are clean,
            // but the archive travels over the network).
            if rel.components().any(|c| !matches!(c, std::path::Component::Normal(_))) {
                return Err(format!("Refusing suspicious archive path: {}", path.display()));
            }
            let target = dest.join(rel);
            if let Some(parent) = target.parent() { fs::create_dir_all(parent).map_err(|e| e.to_string())?; }
            entry.unpack(&target).map_err(|e| format!("Failed to extract {}: {e}", path.display()))?;
            extracted += 1;
        }
        Ok(extracted)
    }

    pub async fn install(cache_dir: PathBuf) -> Result<(), String> {
        let Some(final_dir) = pack_dir() else { return Err("Could not resolve data directory".into()) };
        let staging = cache_dir.join("codec-pack-staging");
        let _ = fs::remove_dir_all(&staging);
        fs::create_dir_all(&staging).map_err(|e| format!("Failed to create staging dir: {e}"))?;

        let cef_pkg = staging.join("cef.pkg.tar.zst");
        let jpeg_pkg = staging.join("libjpeg.pkg.tar.zst");
        download_verified(CEF_PKG_URL, CEF_PKG_SHA256, &cef_pkg).await?;
        download_verified(JPEG_PKG_URL, JPEG_PKG_SHA256, &jpeg_pkg).await?;

        let pack_files = staging.join("pack");
        let staging_for_task = staging.clone();
        let result = tauri::async_runtime::spawn_blocking(move || -> Result<PathBuf, String> {
            let staging = staging_for_task;
            fs::create_dir_all(&pack_files).map_err(|e| e.to_string())?;
            let n = extract_prefixed(&cef_pkg, "usr/lib/cef", &pack_files)?;
            if n == 0 || !pack_files.join("libcef.so").is_file() {
                return Err("CEF package had an unexpected layout".into());
            }
            // Arch's libcef links libjpeg.so.8; ship it in the same dir (covered
            // by the LD_LIBRARY_PATH entry) for non-Arch distros.
            extract_prefixed(&jpeg_pkg, "usr/lib", &staging.join("jpeg"))?;
            let jpeg_real = staging.join("jpeg").join("libjpeg.so.8.3.2");
            if !jpeg_real.is_file() { return Err("libjpeg-turbo package had an unexpected layout".into()); }
            fs::copy(&jpeg_real, pack_files.join("libjpeg.so.8")).map_err(|e| e.to_string())?;
            // Quick sanity check that this build actually carries the decoder:
            // scan for the ffmpeg H.264 profile string in overlapping chunks so
            // the 250+ MB library is never fully resident.
            const NEEDLE: &[u8] = b"h264 high";
            let mut f = fs::File::open(pack_files.join("libcef.so")).map_err(|e| e.to_string())?;
            let mut chunk = vec![0u8; 4 * 1024 * 1024];
            let mut carry: Vec<u8> = Vec::new();
            let mut found = false;
            loop {
                let read = f.read(&mut chunk).map_err(|e| e.to_string())?;
                if read == 0 { break; }
                carry.extend_from_slice(&chunk[..read]);
                if carry.windows(NEEDLE.len()).any(|w| w == NEEDLE) { found = true; break; }
                let keep = carry.len().saturating_sub(NEEDLE.len() - 1);
                carry.drain(..keep);
            }
            if !found {
                return Err("Downloaded CEF build does not appear to include the H.264 decoder".into());
            }
            Ok(pack_files)
        }).await.map_err(|e| format!("Codec pack task failed: {e}"))?;
        let pack_files = result?;

        if let Some(parent) = final_dir.parent() { fs::create_dir_all(parent).map_err(|e| e.to_string())?; }
        let _ = fs::remove_dir_all(&final_dir);
        fs::rename(&pack_files, &final_dir).map_err(|e| format!("Failed to finalize codec pack: {e}"))?;
        let _ = fs::remove_dir_all(&staging);
        log::info!("Codec pack {VERSION} installed at {}", final_dir.display());
        Ok(())
    }

    pub fn remove() -> Result<(), String> {
        let Some(dir) = pack_dir() else { return Ok(()) };
        if dir.is_dir() { fs::remove_dir_all(&dir).map_err(|e| format!("Failed to remove codec pack: {e}"))?; }
        Ok(())
    }
}

#[cfg(target_os = "linux")]
pub use imp::maybe_reexec_with_pack;

#[derive(serde::Serialize, Clone)]
pub struct CodecPackStatus {
    pub supported: bool,
    pub installed: bool,
    pub active: bool,
}

#[tauri::command]
pub fn get_codec_pack_status() -> CodecPackStatus {
    #[cfg(target_os = "linux")]
    { CodecPackStatus { supported: true, installed: imp::installed(), active: imp::active() } }
    #[cfg(target_os = "windows")]
    { CodecPackStatus { supported: false, installed: false, active: false } }
}

#[tauri::command]
pub async fn install_codec_pack<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        use tauri::Manager;
        let cache = app.path().app_cache_dir().map_err(|e| e.to_string())?;
        imp::install(cache).await
    }
    #[cfg(target_os = "windows")]
    { let _ = app; Err("Codec pack is not needed on this platform".into()) }
}

#[tauri::command]
pub fn remove_codec_pack() -> Result<(), String> {
    #[cfg(target_os = "linux")]
    { imp::remove() }
    #[cfg(target_os = "windows")]
    { Err("Codec pack is not needed on this platform".into()) }
}

#[tauri::command]
pub fn restart_launcher<R: tauri::Runtime>(app: tauri::AppHandle<R>) {
    app.request_restart();
}
