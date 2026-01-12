//! garbg - Wallpaper daemon for gar window manager

use anyhow::Result;
use clap::{Parser, Subcommand};
use rand::seq::SliceRandom;
use std::time::Duration;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use garbg::state::{detect_source_type, PlaylistState};

#[derive(Parser)]
#[command(name = "garbg")]
#[command(about = "A bespoke wallpaper daemon for the gar window manager")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Enable verbose logging
    #[arg(short, long, global = true)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Set a wallpaper from a file, directory, or URL
    Set {
        /// Path or URI to the wallpaper source (file, directory, URL, or github://user/repo/path)
        source: String,

        /// Scaling mode (fill, fit, stretch, center, tile)
        #[arg(short, long, default_value = "fill")]
        mode: String,

        /// Target monitor (default: all)
        #[arg(short = 'o', long)]
        monitor: Option<String>,

        /// Shuffle images when source is a directory
        #[arg(short, long)]
        random: bool,

        /// Auto-rotate interval (e.g., "5m", "30s", "1h"). Process stays running.
        #[arg(short, long, value_parser = parse_duration)]
        interval: Option<Duration>,
    },

    /// Advance to the next image in the playlist
    Next,

    /// Go back to the previous image in the playlist
    Prev,

    /// List images from a source (directory, GitHub, URL)
    List {
        /// Path or URI to list
        source: String,
    },

    /// Start the daemon
    Daemon {
        /// Path to config file
        #[arg(short, long)]
        config: Option<String>,

        /// Fork to background (daemonize)
        #[arg(short, long)]
        daemonize: bool,
    },

    /// Reload configuration
    Reload,

    /// Get current status
    Status,
}

/// Parse a duration string like "5m", "30s", "1h"
fn parse_duration(s: &str) -> Result<Duration, String> {
    humantime::parse_duration(s).map_err(|e| e.to_string())
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    let filter = if cli.verbose {
        EnvFilter::new("garbg=debug")
    } else {
        EnvFilter::new("garbg=info")
    };

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(filter)
        .init();

    match cli.command {
        Commands::Set { source, mode, monitor, random, interval } => {
            set_wallpaper(&source, &mode, monitor.as_deref(), random, interval)?;
        }
        Commands::Next => {
            cmd_next()?;
        }
        Commands::Prev => {
            cmd_prev()?;
        }
        Commands::List { source } => {
            list_images(&source)?;
        }
        Commands::Daemon { config, daemonize } => {
            if daemonize {
                daemonize_process(config)?;
            } else {
                tracing::info!("Starting daemon");
                run_daemon(config.as_deref())?;
            }
        }
        Commands::Reload => {
            tracing::info!("Reloading configuration");
            send_reload_command()?;
        }
        Commands::Status => {
            print_status()?;
        }
    }

    Ok(())
}

fn set_wallpaper(
    source: &str,
    mode: &str,
    _monitor: Option<&str>,
    random: bool,
    interval: Option<Duration>,
) -> Result<()> {
    use garbg::config::ScaleMode;
    use garbg::ipc::{is_daemon_running, send_command_blocking, Command};

    let scale_mode: ScaleMode = mode.parse()?;

    // Normalize GitHub URLs first
    let normalized_source = normalize_github_url(source);

    // If daemon is running, delegate to it (especially for interval-based rotation)
    if is_daemon_running() {
        let interval_secs = interval.map(|d| d.as_secs());

        let cmd = Command::Set {
            source: normalized_source.clone(),
            mode: Some(scale_mode),
            monitor: None,
            interval_secs,
            shuffle: random,
        };

        let response = send_command_blocking(&cmd)?;

        if response.success {
            if let Some(secs) = interval_secs {
                println!("Slideshow scheduled: {} (every {}s, shuffle: {})",
                    normalized_source, secs, random);
            } else {
                println!("Wallpaper set via daemon: {}", normalized_source);
            }
            return Ok(());
        } else if let Some(err) = response.error {
            anyhow::bail!("Daemon error: {}", err);
        }
    }

    // Daemon not running - handle locally
    if interval.is_some() {
        eprintln!("Note: Daemon not running. Using foreground rotation (blocks terminal).");
        eprintln!("      For background rotation, first run: garbg daemon -d");
        eprintln!();
    }

    // Get list of images from the source
    let images = list_images_from_source(&normalized_source)?;

    if images.is_empty() {
        anyhow::bail!("No images found in source: {}", source);
    }

    // Determine the image to display and whether to save state
    let (resolved_source, mut state) = if images.len() > 1 {
        // Directory/collection - create playlist state
        let mut imgs = images;
        if random {
            let mut rng = rand::thread_rng();
            imgs.shuffle(&mut rng);
        }

        let state = PlaylistState::new(
            normalized_source.clone(),
            detect_source_type(&normalized_source),
            imgs,
            random,
            scale_mode,
        );

        let first_image = state.images[0].clone();
        (first_image, Some(state))
    } else {
        // Single file - no playlist state needed
        let single = if normalized_source.starts_with("github://") {
            github_to_raw_url(&normalized_source)?
        } else {
            images[0].clone()
        };
        (single, None)
    };

    // Save state if we have a playlist
    if let Some(ref mut s) = state {
        s.save()?;
        tracing::info!(
            "Playlist created: {} images, shuffled: {}",
            s.len(),
            s.shuffled
        );
    }

    // Set the initial wallpaper
    set_single_wallpaper(&resolved_source, scale_mode)?;

    // If interval specified and daemon not running, enter foreground rotation loop
    if let Some(interval_duration) = interval {
        if let Some(mut playlist_state) = state {
            tracing::info!(
                "Starting rotation every {:?} (Ctrl+C to stop)",
                interval_duration
            );

            loop {
                std::thread::sleep(interval_duration);

                // Reload state in case next/prev was called externally
                playlist_state.reload()?;

                // Advance to next
                let next_img = playlist_state.next().to_string();
                playlist_state.save()?;

                set_single_wallpaper(&next_img, playlist_state.mode)?;
                tracing::info!(
                    "Rotated to [{}/{}]: {}",
                    playlist_state.current_index + 1,
                    playlist_state.len(),
                    next_img
                );
            }
        } else {
            tracing::warn!("--interval requires a directory source with multiple images");
        }
    }

    Ok(())
}

/// Set a single wallpaper (used by set, next, prev)
fn set_single_wallpaper(source: &str, mode: garbg::config::ScaleMode) -> Result<()> {
    use garbg::media::ImageLoader;
    use garbg::x11::Connection;

    tracing::info!("Setting wallpaper: {}", source);

    let mut conn = Connection::new()?;

    let image = if source.starts_with("http://") || source.starts_with("https://") {
        fetch_image_from_url(source)?
    } else {
        ImageLoader::load_file(source)?
    };

    let (width, height) = conn.screen_dimensions();
    let scaled = garbg::media::scale_image(&image, width as u32, height as u32, mode);
    conn.set_wallpaper(&scaled)?;

    tracing::info!("Wallpaper set: {} ({}x{}, mode: {:?})", source, width, height, mode);

    Ok(())
}

/// Advance to the next image in the playlist
fn cmd_next() -> Result<()> {
    let mut state = PlaylistState::load()?
        .ok_or_else(|| anyhow::anyhow!("No active playlist. Use 'garbg set <directory>' first."))?;

    let next_image = state.next().to_string();
    state.save()?;

    set_single_wallpaper(&next_image, state.mode)?;

    tracing::info!(
        "Next [{}/{}]: {}",
        state.current_index + 1,
        state.len(),
        next_image
    );

    Ok(())
}

/// Go back to the previous image in the playlist
fn cmd_prev() -> Result<()> {
    let mut state = PlaylistState::load()?
        .ok_or_else(|| anyhow::anyhow!("No active playlist. Use 'garbg set <directory>' first."))?;

    let prev_image = state.prev().to_string();
    state.save()?;

    set_single_wallpaper(&prev_image, state.mode)?;

    tracing::info!(
        "Prev [{}/{}]: {}",
        state.current_index + 1,
        state.len(),
        prev_image
    );

    Ok(())
}

/// List images from a source (directory, GitHub, etc.)
fn list_images_from_source(source: &str) -> Result<Vec<String>> {
    // Check for GitHub URLs and convert to github:// format
    let normalized = normalize_github_url(source);

    if normalized.starts_with("github://") {
        list_github_directory(&normalized)
    } else if normalized.starts_with("http://") || normalized.starts_with("https://") {
        // For HTTP, could be a directory index - try to parse
        list_http_directory(&normalized)
    } else {
        // Local path
        list_local_directory(&normalized)
    }
}

/// Convert github:// URI to raw.githubusercontent.com URL
fn github_to_raw_url(uri: &str) -> Result<String> {
    let path = uri.strip_prefix("github://")
        .ok_or_else(|| anyhow::anyhow!("Invalid GitHub URI"))?;

    let parts: Vec<&str> = path.splitn(3, '/').collect();
    if parts.len() < 3 {
        anyhow::bail!("GitHub URI must include a file path: github://user/repo/path/to/file");
    }

    let (user, repo, file_path) = (parts[0], parts[1], parts[2]);

    Ok(format!(
        "https://raw.githubusercontent.com/{}/{}/HEAD/{}",
        user, repo, file_path
    ))
}

/// Convert GitHub web URLs to github:// format
/// Handles:
///   https://github.com/user/repo/tree/branch/path -> github://user/repo/path
///   https://github.com/user/repo/blob/branch/path -> github://user/repo/path
///   https://github.com/user/repo -> github://user/repo
fn normalize_github_url(source: &str) -> String {
    // Check if it's a GitHub web URL
    if source.starts_with("https://github.com/") || source.starts_with("http://github.com/") {
        let path = source
            .trim_start_matches("https://github.com/")
            .trim_start_matches("http://github.com/");

        let parts: Vec<&str> = path.split('/').collect();

        if parts.len() >= 2 {
            let user = parts[0];
            let repo = parts[1];

            // Check for /tree/branch/path or /blob/branch/path
            if parts.len() >= 4 && (parts[2] == "tree" || parts[2] == "blob") {
                // Skip "tree" or "blob" and branch name, take the rest as path
                let file_path = parts[4..].join("/");
                if file_path.is_empty() {
                    return format!("github://{}/{}", user, repo);
                }
                return format!("github://{}/{}/{}", user, repo, file_path);
            }

            // Just user/repo
            return format!("github://{}/{}", user, repo);
        }
    }

    // Return as-is if not a GitHub URL
    source.to_string()
}

/// List images in a local directory
fn list_local_directory(path: &str) -> Result<Vec<String>> {
    use garbg::media::ImageLoader;
    use std::path::Path;

    let path_str = shellexpand::tilde(path);
    let dir_path = Path::new(path_str.as_ref());

    if dir_path.is_file() {
        // Single file, return as-is
        return Ok(vec![path_str.to_string()]);
    }

    if !dir_path.is_dir() {
        anyhow::bail!("Path is not a file or directory: {}", path);
    }

    let mut images = Vec::new();
    for entry in std::fs::read_dir(dir_path)? {
        let entry = entry?;
        let entry_path = entry.path();
        if entry_path.is_file() && ImageLoader::is_supported_format(&entry_path) {
            images.push(entry_path.to_string_lossy().to_string());
        }
    }

    images.sort();
    Ok(images)
}

/// List images from a GitHub directory
fn list_github_directory(uri: &str) -> Result<Vec<String>> {
    // Parse github://user/repo/path format
    let path = uri.strip_prefix("github://")
        .ok_or_else(|| anyhow::anyhow!("Invalid GitHub URI"))?;

    let parts: Vec<&str> = path.splitn(3, '/').collect();
    if parts.len() < 2 {
        anyhow::bail!("GitHub URI must be github://user/repo[/path]");
    }

    let user = parts[0];
    let repo = parts[1];
    let dir_path = parts.get(2).unwrap_or(&"");

    // Call GitHub API to list contents
    let api_url = format!(
        "https://api.github.com/repos/{}/{}/contents/{}",
        user, repo, dir_path
    );

    tracing::debug!("Fetching GitHub directory: {}", api_url);

    let client = reqwest::blocking::Client::builder()
        .user_agent("garbg/0.1")
        .build()?;

    let response = client.get(&api_url).send()?;
    let status = response.status();

    if !status.is_success() {
        // If it's a file, not a directory, return the direct raw URL
        if status.as_u16() == 404 || dir_path.contains('.') {
            // Likely a file path, return as single image
            let raw_url = format!(
                "https://raw.githubusercontent.com/{}/{}/HEAD/{}",
                user, repo, dir_path
            );
            return Ok(vec![raw_url]);
        }
        anyhow::bail!("GitHub API error {}: {}", status, api_url);
    }

    let text = response.text()?;

    // Parse JSON response
    let contents: Vec<serde_json::Value> = serde_json::from_str(&text)?;

    let image_extensions = ["png", "jpg", "jpeg", "gif", "webp", "bmp"];

    let mut images = Vec::new();
    for item in contents {
        if item["type"].as_str() == Some("file") {
            if let Some(name) = item["name"].as_str() {
                let ext = name.rsplit('.').next().unwrap_or("").to_lowercase();
                if image_extensions.contains(&ext.as_str()) {
                    if let Some(download_url) = item["download_url"].as_str() {
                        images.push(download_url.to_string());
                    }
                }
            }
        }
    }

    Ok(images)
}

/// List images from an HTTP directory index
fn list_http_directory(url: &str) -> Result<Vec<String>> {
    // If URL doesn't end with /, it's probably a direct file
    if !url.ends_with('/') {
        return Ok(vec![url.to_string()]);
    }

    let client = reqwest::blocking::Client::builder()
        .user_agent("garbg/0.1")
        .build()?;

    let response = client.get(url).send()?;
    let status = response.status();

    if !status.is_success() {
        anyhow::bail!("HTTP error {}: {}", status, url);
    }

    let html = response.text()?;

    // Parse HTML for links
    let image_extensions = ["png", "jpg", "jpeg", "gif", "webp", "bmp"];
    let mut images = Vec::new();

    // Simple regex-free parsing - look for href="..."
    for part in html.split("href=\"") {
        if let Some(end) = part.find('"') {
            let href = &part[..end];
            // Skip parent links and query strings
            if href == "../" || href.starts_with('?') || href.starts_with('/') {
                continue;
            }
            let ext = href.rsplit('.').next().unwrap_or("").to_lowercase();
            if image_extensions.contains(&ext.as_str()) {
                let full_url = format!("{}{}", url, href);
                images.push(full_url);
            }
        }
    }

    Ok(images)
}

fn fetch_image_from_url(url: &str) -> Result<image::RgbaImage> {
    use garbg::media::ImageLoader;

    let client = reqwest::blocking::Client::builder()
        .user_agent("garbg/0.1")
        .build()?;

    let response = client.get(url).send()?;
    let status = response.status();

    if !status.is_success() {
        anyhow::bail!("HTTP error {}: {}", status, url);
    }

    let bytes = response.bytes()?;
    ImageLoader::load_bytes(&bytes, None)
}

/// List images from a source and print them
fn list_images(source: &str) -> Result<()> {
    let images = list_images_from_source(source)?;

    if images.is_empty() {
        println!("No images found in: {}", source);
    } else {
        println!("Found {} images in {}:", images.len(), source);
        for img in &images {
            println!("  {}", img);
        }
    }

    Ok(())
}

fn run_daemon(config_path: Option<&str>) -> Result<()> {
    use garbg::config::Config;
    use garbg::daemon::Daemon;

    // Load configuration
    let config = match config_path {
        Some(path) => Config::load(path)?,
        None => Config::load_default()?,
    };

    // Create the daemon
    let mut daemon = Daemon::new(config)?;

    // Run with a single-threaded async runtime (lightweight)
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    rt.block_on(daemon.run())
}

/// Fork the daemon to background
fn daemonize_process(config: Option<String>) -> Result<()> {
    use std::process::{Command, Stdio};

    let exe = std::env::current_exe()?;

    let mut cmd = Command::new(&exe);
    cmd.arg("daemon");

    if let Some(config_path) = config {
        cmd.arg("--config").arg(config_path);
    }

    // Detach from terminal
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    // Spawn as independent process
    let child = cmd.spawn()?;

    println!("Daemon started (PID: {})", child.id());

    // Give daemon a moment to start and verify it's running
    std::thread::sleep(std::time::Duration::from_millis(500));

    if garbg::ipc::is_daemon_running() {
        println!("Daemon is running");
    } else {
        anyhow::bail!("Daemon failed to start");
    }

    Ok(())
}

fn send_reload_command() -> Result<()> {
    use garbg::ipc::{Command, is_daemon_running, send_command_blocking};

    if !is_daemon_running() {
        anyhow::bail!("Daemon is not running. Start it with 'garbg daemon'");
    }

    let cmd = Command::Reload;
    let response = send_command_blocking(&cmd)?;

    if response.success {
        println!("Configuration reloaded");
    } else if let Some(err) = response.error {
        anyhow::bail!("Reload failed: {}", err);
    }

    Ok(())
}

fn print_status() -> Result<()> {
    match PlaylistState::load()? {
        Some(state) => {
            println!("garbg playlist status");
            println!("---------------------");
            println!("Source: {}", state.source);
            println!("Type: {:?}", state.source_type);
            println!("Images: {} total", state.len());
            println!(
                "Current: [{}/{}]",
                state.current_index + 1,
                state.len()
            );
            if let Some(current) = state.current() {
                println!("  {}", current);
            }
            println!("Shuffled: {}", state.shuffled);
            println!("Mode: {:?}", state.mode);
            println!("Last updated: {}", state.last_updated);
        }
        None => {
            println!("No active playlist.");
            println!("Use 'garbg set <directory>' to create one.");
        }
    }
    Ok(())
}
