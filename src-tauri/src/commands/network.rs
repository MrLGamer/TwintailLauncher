use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs,
    io::{Read, Seek, SeekFrom, Write},
    net::{TcpListener, TcpStream},
    path::Path,
    sync::OnceLock,
    thread,
    time::Instant,
};
use tauri::{AppHandle, Manager};

static WALLPAPER_SERVER_PORT: OnceLock<Result<u16, String>> = OnceLock::new();

#[derive(Serialize, Clone)]
pub struct NetworkStatus {
    pub status: String, // "online", "slow", "offline"
    pub latency_ms: Option<u64>,
    pub message: String,
}

#[tauri::command]
pub async fn check_network_connectivity() -> NetworkStatus {
    let endpoints = ["https://store.steampowered.com", "https://one.one.one.one", "https://twintaillauncher.app"];

    let mut best_latency: Option<u64> = None;
    for endpoint in endpoints {
        let start = Instant::now();
        match fischl::utils::check_network_status(endpoint.to_string()).await {
            Ok(response) => {
                let latency = start.elapsed().as_millis() as u64;
                if response.status().is_success() || response.status().as_u16() == 204 || response.status().as_u16() == 405 {
                    // Return immediately if any endpoint is fast
                    if latency < 5000 {
                        log::debug!("Network check: online ({}ms via {})", latency, endpoint);
                        return NetworkStatus { status: "online".to_string(), latency_ms: Some(latency), message: "Connection is good".to_string() };
                    }
                    // Track best latency across slow endpoints
                    if best_latency.is_none() || latency < best_latency.unwrap() { best_latency = Some(latency); }
                }
            }
            Err(_) => { continue; }
        }
    }

    // If any endpoint responded (but all were slow), report slow with best latency
    if let Some(latency) = best_latency {
        log::warn!("Network check: slow (best {}ms, all endpoints responded slowly)", latency);
        return NetworkStatus { status: "slow".to_string(), latency_ms: Some(latency), message: "Connection is slow".to_string() };
    }

    log::warn!("Network check: offline, all endpoints unreachable");
    NetworkStatus { status: "offline".to_string(), latency_ms: None, message: "Unable to connect to the internet".to_string() }
}

fn live_background_hash(url: &str) -> String {
    format!("{:x}", Sha256::digest(url.as_bytes()))
}

fn live_background_cache_path(app: &AppHandle, url: &str) -> Result<std::path::PathBuf, String> {
    let parsed_url = reqwest::Url::parse(url).map_err(|e| format!("Invalid wallpaper URL: {e}"))?;
    if !matches!(parsed_url.scheme(), "http" | "https") {
        return Err("Wallpaper URL must use HTTP or HTTPS".to_string());
    }

    let extension = parsed_url
        .path_segments()
        .and_then(|segments| segments.last())
        .and_then(|filename| filename.rsplit_once('.').map(|(_, extension)| extension))
        .filter(|extension| matches!(extension.to_ascii_lowercase().as_str(), "mp4" | "webm"))
        .unwrap_or("video");
    let hash = live_background_hash(url);
    let cache_dir = app.path().app_cache_dir().map_err(|e| e.to_string())?.join("wallpapers");
    Ok(cache_dir.join(format!("{hash}.{extension}")))
}

fn serve_cached_wallpaper(mut stream: TcpStream, cache_dir: &Path) -> Result<(), String> {
    let mut request = [0; 16384];
    let request_len = stream.read(&mut request).map_err(|e| e.to_string())?;
    let request = String::from_utf8_lossy(&request[..request_len]);
    let mut lines = request.lines();
    let request_line = lines.next().ok_or("Missing request line")?;
    let mut request_parts = request_line.split_whitespace();
    let method = request_parts.next().ok_or("Missing request method")?;
    let filename = request_parts.next().ok_or("Missing request path")?.trim_start_matches('/');

    let valid_filename = filename
        .rsplit_once('.')
        .map(|(hash, extension)| hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit()) && matches!(extension, "mp4" | "webm"))
        .unwrap_or(false);
    if !matches!(method, "GET" | "HEAD") || !valid_filename {
        stream.write_all(b"HTTP/1.1 404 Not Found\r\nConnection: close\r\nContent-Length: 0\r\n\r\n").map_err(|e| e.to_string())?;
        return Ok(());
    }

    let cache_path = cache_dir.join(filename);
    let mut file = fs::File::open(&cache_path).map_err(|_| "Wallpaper cache file not found".to_string())?;
    let total_len = file.metadata().map_err(|e| e.to_string())?.len();
    let range = lines
        .find_map(|line| line.strip_prefix("Range: ").or_else(|| line.strip_prefix("range: ")))
        .and_then(|value| value.strip_prefix("bytes="))
        .and_then(|value| value.split_once('-'))
        .and_then(|(start, end)| {
            let start = start.parse::<u64>().ok()?;
            let end = if end.is_empty() { total_len.saturating_sub(1) } else { end.parse::<u64>().ok()? };
            (start < total_len).then_some((start, end.min(total_len.saturating_sub(1))))
        });
    let (start, end, status) = range.map(|(start, end)| (start, end, "206 Partial Content")).unwrap_or((0, total_len.saturating_sub(1), "200 OK"));
    let content_len = end.saturating_sub(start) + 1;
    let mime_type = if filename.ends_with(".webm") { "video/webm" } else { "video/mp4" };
    let content_range = range.map(|_| format!("Content-Range: bytes {start}-{end}/{total_len}\r\n")).unwrap_or_default();
    let headers = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {mime_type}\r\nAccept-Ranges: bytes\r\nContent-Length: {content_len}\r\n{content_range}Connection: close\r\n\r\n"
    );
    stream.write_all(headers.as_bytes()).map_err(|e| e.to_string())?;

    if method == "GET" {
        file.seek(SeekFrom::Start(start)).map_err(|e| e.to_string())?;
        let mut remaining = content_len;
        let mut buffer = [0; 65536];
        while remaining > 0 {
            let chunk_len = remaining.min(buffer.len() as u64) as usize;
            let read_len = file.read(&mut buffer[..chunk_len]).map_err(|e| e.to_string())?;
            if read_len == 0 { break; }
            stream.write_all(&buffer[..read_len]).map_err(|e| e.to_string())?;
            remaining -= read_len as u64;
        }
    }
    Ok(())
}

fn cached_wallpaper_url(cache_dir: &Path, cache_path: &Path) -> Result<String, String> {
    let port = WALLPAPER_SERVER_PORT.get_or_init(|| {
        let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(|e| e.to_string())?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        let cache_dir = cache_dir.to_path_buf();
        thread::spawn(move || {
            for stream in listener.incoming() {
                match stream {
                    Ok(stream) => {
                        let cache_dir = cache_dir.clone();
                        thread::spawn(move || { let _ = serve_cached_wallpaper(stream, &cache_dir); });
                    }
                    Err(error) => log::warn!("Wallpaper cache server error: {error}"),
                }
            }
        });
        Ok(port)
    }).clone()?;
    let filename = cache_path.file_name().and_then(|filename| filename.to_str()).ok_or("Invalid wallpaper cache filename")?;
    Ok(format!("http://127.0.0.1:{port}/{filename}"))
}

#[tauri::command]
pub fn cleanup_live_background_cache(app: AppHandle, urls: Vec<String>) -> Result<usize, String> {
    let cache_dir = app.path().app_cache_dir().map_err(|e| e.to_string())?.join("wallpapers");
    if !cache_dir.is_dir() {
        return Ok(0);
    }

    let referenced_hashes: HashSet<String> = urls.iter().map(|url| live_background_hash(url)).collect();
    let mut removed = 0;
    for entry in fs::read_dir(cache_dir).map_err(|e| format!("Failed to read wallpaper cache: {e}"))? {
        let entry = entry.map_err(|e| format!("Failed to read wallpaper cache entry: {e}"))?;
        let path = entry.path();
        let Some(filename) = path.file_name().and_then(|filename| filename.to_str()) else { continue };
        let Some((hash, extension)) = filename.rsplit_once('.') else { continue };
        if hash.len() != 64 || !hash.chars().all(|c| c.is_ascii_hexdigit()) || !matches!(extension, "mp4" | "webm") {
            continue;
        }
        if !referenced_hashes.contains(hash) {
            fs::remove_file(&path).map_err(|e| format!("Failed to remove cached wallpaper {filename}: {e}"))?;
            removed += 1;
        }
    }

    Ok(removed)
}

#[tauri::command]
pub async fn cache_live_background(app: AppHandle, url: String) -> Result<String, String> {
    let parsed_url = reqwest::Url::parse(&url).map_err(|e| format!("Invalid wallpaper URL: {e}"))?;
    let cache_path = live_background_cache_path(&app, &url)?;
    let cache_dir = cache_path.parent().ok_or("Invalid wallpaper cache path")?;

    if cache_path.is_file() {
        return cached_wallpaper_url(cache_dir, &cache_path);
    }

    fs::create_dir_all(&cache_dir).map_err(|e| format!("Failed to create wallpaper cache: {e}"))?;
    let response = reqwest::get(parsed_url)
        .await
        .map_err(|e| format!("Failed to download wallpaper: {e}"))?
        .error_for_status()
        .map_err(|e| format!("Failed to download wallpaper: {e}"))?;
    let bytes = response.bytes().await.map_err(|e| format!("Failed to read wallpaper download: {e}"))?;
    let temporary_path = cache_path.with_extension("tmp");
    fs::write(&temporary_path, bytes).map_err(|e| format!("Failed to write wallpaper cache: {e}"))?;
    fs::rename(&temporary_path, &cache_path).map_err(|e| format!("Failed to finalize wallpaper cache: {e}"))?;

    cached_wallpaper_url(cache_dir, &cache_path)
}
