//! Daemon state management

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::time::Duration;
use tokio::net::UnixStream;
use tokio::signal::unix::{signal, SignalKind};

use crate::cache::DiskCache;
use crate::config::{Config, ScaleMode};
use crate::ipc::{Command, GarEvent, GarIpcClient, IpcServer, Response};
use crate::ipc::server::IpcClient;
use crate::media::{scale_image, scale_image_fast, AnimatedGif, AnimatedPng, AnimatedWebP, AnimationFrame, ImageLoader};
#[cfg(feature = "video")]
use crate::media::{VideoDecoder, is_video_file};
use crate::state::{detect_source_type, PlaylistState};
use crate::x11::{AnimationRenderer, Connection, Compositor, Monitor};

use super::pid;

/// Current wallpaper state for a monitor
#[derive(Debug, Clone)]
pub struct MonitorWallpaper {
    /// Monitor name
    pub name: String,
    /// Current wallpaper source
    pub source: String,
    /// Scale mode
    pub mode: ScaleMode,
}

/// Main daemon state
pub struct DaemonState {
    /// Current wallpaper per monitor
    pub monitors: HashMap<String, MonitorWallpaper>,

    /// Current workspace
    pub current_workspace: usize,

    /// Whether slideshow/animations are paused
    pub paused: bool,

    /// Configuration
    pub config: Config,

    /// Current playlist state (if any)
    pub playlist: Option<PlaylistState>,

    /// Current slideshow interval (None = no auto-rotation)
    pub slideshow_interval: Option<Duration>,
}

impl DaemonState {
    pub fn new(config: Config) -> Self {
        // Try to load existing playlist state
        let playlist = PlaylistState::load().ok().flatten();

        // Get initial slideshow interval from config
        let slideshow_interval = config.default.slideshow
            .as_ref()
            .filter(|s| s.enabled)
            .map(|s| s.interval);

        Self {
            monitors: HashMap::new(),
            current_workspace: 1,
            paused: false,
            config,
            playlist,
            slideshow_interval,
        }
    }
}

/// Active animation state (works with GIF, WebP, or other animated formats)
pub struct ActiveAnimation {
    /// Pre-scaled frames for quick rendering
    scaled_frames: Vec<image::RgbaImage>,
    /// Frame delays (parallel to scaled_frames)
    frame_delays: Vec<Duration>,
    /// Animation renderer (double-buffered)
    renderer: AnimationRenderer,
    /// Current frame index
    current_frame: usize,
    /// Max FPS
    max_fps: u32,
    /// Scale mode (for status)
    #[allow(dead_code)]
    scale_mode: ScaleMode,
    /// Source URI (for status)
    #[allow(dead_code)]
    source: String,
}

impl ActiveAnimation {
    /// Create from animation frames (scales in parallel with fast filter)
    fn from_frames(
        frames: &[AnimationFrame],
        renderer: AnimationRenderer,
        max_fps: u32,
        scale_mode: ScaleMode,
        source: String,
        screen_width: u32,
        screen_height: u32,
    ) -> Self {
        // Scale frames in parallel, limited to available CPU cores
        let num_cpus = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);
        let scaled_frames: Vec<image::RgbaImage> = std::thread::scope(|s| {
            let mut results = Vec::with_capacity(frames.len());
            // Process in batches of num_cpus to avoid spawning too many threads
            for chunk in frames.chunks(num_cpus) {
                let handles: Vec<_> = chunk
                    .iter()
                    .map(|frame| {
                        s.spawn(move || {
                            scale_image_fast(&frame.image, screen_width, screen_height, scale_mode)
                        })
                    })
                    .collect();
                results.extend(handles.into_iter().map(|h| h.join().unwrap()));
            }
            results
        });

        let frame_delays: Vec<Duration> = frames
            .iter()
            .map(|frame| frame.delay)
            .collect();

        Self {
            scaled_frames,
            frame_delays,
            renderer,
            current_frame: 0,
            max_fps,
            scale_mode,
            source,
        }
    }

    /// Get the delay for the current frame
    fn current_delay(&self) -> Duration {
        let frame_delay = self.frame_delays[self.current_frame];
        let min_delay = if self.max_fps > 0 {
            Duration::from_secs_f64(1.0 / self.max_fps as f64)
        } else {
            Duration::ZERO
        };
        frame_delay.max(min_delay)
    }

    /// Advance to next frame, returning true if looped
    fn advance(&mut self) -> bool {
        self.current_frame += 1;
        if self.current_frame >= self.scaled_frames.len() {
            self.current_frame = 0;
            true
        } else {
            false
        }
    }

    /// Get frame count
    #[allow(dead_code)]
    fn frame_count(&self) -> usize {
        self.scaled_frames.len()
    }
}

/// Main daemon struct
pub struct Daemon {
    /// X11 connection (None if disconnected, will attempt reconnect)
    conn: Option<Connection>,

    /// Daemon state
    state: DaemonState,

    /// Current animation (if playing)
    animation: Option<ActiveAnimation>,

    /// Disk cache for remote images
    cache: Option<DiskCache>,
}

impl Daemon {
    /// Create a new daemon
    ///
    /// Establishes X11 connection and initializes state.
    /// The daemon should be started by systemd after graphical-session.target
    /// is active, so X11 should already be ready.
    pub fn new(config: Config) -> Result<Self> {
        // Connect to X11 (fail fast - systemd ensures session is ready)
        let conn = Connection::new()
            .context("Failed to connect to X11. Is the graphical session active?")?;

        let (width, height) = conn.screen_dimensions();
        tracing::info!("X11 connection established (screen: {}x{})", width, height);

        // Initialize disk cache
        let cache = match DiskCache::default_dir() {
            Some(cache_dir) => {
                let max_size_mb = config.cache.max_size_mb;
                match DiskCache::new(cache_dir.clone(), max_size_mb) {
                    Ok(cache) => {
                        tracing::info!("Disk cache initialized: {} (max {}MB)", cache_dir.display(), max_size_mb);
                        Some(cache)
                    }
                    Err(e) => {
                        tracing::warn!("Failed to initialize disk cache: {}", e);
                        None
                    }
                }
            }
            None => {
                tracing::warn!("No cache directory available, caching disabled");
                None
            }
        };

        let state = DaemonState::new(config);

        Ok(Self {
            conn: Some(conn),
            state,
            animation: None,
            cache,
        })
    }

    /// Get a reference to the X11 connection, or error if disconnected
    fn conn(&self) -> Result<&Connection> {
        self.conn.as_ref().ok_or_else(|| anyhow::anyhow!("X11 connection not available"))
    }

    /// Get a mutable reference to the X11 connection, or error if disconnected
    fn conn_mut(&mut self) -> Result<&mut Connection> {
        self.conn.as_mut().ok_or_else(|| anyhow::anyhow!("X11 connection not available"))
    }

    /// Attempt to establish/re-establish X11 connection
    ///
    /// On reconnection, refreshes `DISPLAY` from the systemd user environment
    /// since the X display number may change across session restarts (e.g. `:0` -> `:1`).
    fn try_connect_x11(&mut self) -> bool {
        Self::refresh_display_env();

        match Connection::new() {
            Ok(conn) => {
                let (width, height) = conn.screen_dimensions();
                tracing::info!("X11 connection established (screen: {}x{})", width, height);
                self.conn = Some(conn);
                true
            }
            Err(e) => {
                tracing::debug!("X11 connection failed: {}", e);
                false
            }
        }
    }

    /// Refresh DISPLAY (and XAUTHORITY) from the systemd user manager environment.
    ///
    /// When the X session restarts, the display number can change (e.g. `:0` -> `:1`).
    /// systemd's user manager gets updated via `systemctl --user import-environment`,
    /// but a long-running daemon keeps the stale value in its own process environment.
    fn refresh_display_env() {
        let output = match std::process::Command::new("systemctl")
            .args(["--user", "show-environment"])
            .output()
        {
            Ok(o) if o.status.success() => o,
            _ => return,
        };

        let env_str = match std::str::from_utf8(&output.stdout) {
            Ok(s) => s,
            Err(_) => return,
        };

        for line in env_str.lines() {
            if let Some(val) = line.strip_prefix("DISPLAY=") {
                let current = std::env::var("DISPLAY").unwrap_or_default();
                if current != val {
                    tracing::info!("DISPLAY changed: {} -> {}", current, val);
                    std::env::set_var("DISPLAY", val);
                }
            } else if let Some(val) = line.strip_prefix("XAUTHORITY=") {
                let current = std::env::var("XAUTHORITY").unwrap_or_default();
                if current != val {
                    tracing::info!("XAUTHORITY changed: {} -> {}", current, val);
                    std::env::set_var("XAUTHORITY", val);
                }
            }
        }
    }

    /// Check if X11 connection is alive
    fn x11_is_alive(&self) -> bool {
        self.conn.as_ref().map(|c| c.is_alive()).unwrap_or(false)
    }

    /// Re-apply current wallpaper after X11 reconnection
    fn reapply_wallpaper(&mut self) -> Result<()> {
        // Stop any animation (it had the old connection's renderer)
        self.animation = None;

        // Re-apply from playlist if we have one
        if let Some(ref playlist) = self.state.playlist {
            if let Some(current) = playlist.current() {
                let source = current.to_string();
                let mode = playlist.mode;
                return self.set_wallpaper(&source, mode);
            }
        }

        // Otherwise try to re-apply default wallpaper
        if !self.state.config.default.source.is_empty() {
            return self.apply_default_wallpaper();
        }

        Ok(())
    }

    /// Run the daemon event loop
    pub async fn run(&mut self) -> Result<()> {
        // Check for stale PID file and clean up
        if let Some(existing_pid) = pid::check_stale_pid()? {
            anyhow::bail!("Another daemon is already running (PID: {})", existing_pid);
        }

        // Write our PID file
        pid::write_pid_file()?;

        // Ensure cleanup on exit (both normal and panic)
        let _pid_guard = PidFileGuard;

        let server = IpcServer::new().await?;
        tracing::info!("Listening on {}", server.path().display());

        // Notify systemd that we're ready (for Type=notify services)
        // This ensures gar-session.sh waits until the socket is actually listening
        if let Err(e) = sd_notify::notify(true, &[sd_notify::NotifyState::Ready]) {
            tracing::debug!("sd_notify failed (not running under systemd?): {}", e);
        } else {
            tracing::debug!("Notified systemd: READY=1");
        }

        // Set initial wallpaper from config if specified
        if !self.state.config.default.source.is_empty() {
            if let Err(e) = self.apply_default_wallpaper() {
                tracing::warn!("Failed to set initial wallpaper: {}", e);
            }
        }

        // Try to connect to gar (optional)
        let mut gar_client = self.try_connect_gar().await;

        // Reconnection state for gar
        let mut gar_reconnect_backoff = Duration::from_secs(1);
        let mut last_gar_reconnect = std::time::Instant::now();
        let gar_max_backoff = Duration::from_secs(60);

        // X11 health check and reconnection state
        let x11_health_interval = Duration::from_secs(5);
        let mut last_x11_health_check = std::time::Instant::now();
        let mut x11_reconnect_backoff = Duration::from_secs(1);
        let mut last_x11_reconnect = std::time::Instant::now();
        let x11_max_backoff = Duration::from_secs(30);
        let mut x11_needs_reconnect = false;

        // Track next slideshow time
        let mut next_slideshow: Option<tokio::time::Instant> = self.state.slideshow_interval
            .map(|d| tokio::time::Instant::now() + d);

        tracing::info!("Daemon started");
        if let Some(interval) = self.state.slideshow_interval {
            tracing::info!("Slideshow enabled: {:?} interval", interval);
        }

        // Track next animation frame time
        let mut next_animation_frame: Option<tokio::time::Instant> = None;

        // Set up signal handlers
        let mut sigterm = signal(SignalKind::terminate())?;
        let mut sigint = signal(SignalKind::interrupt())?;
        let mut sighup = signal(SignalKind::hangup())?;

        // Track whether we need to attempt gar reconnection
        let mut gar_needs_reconnect = gar_client.is_none();

        // Main event loop
        // Note: Session lifecycle is handled by systemd (PartOf=graphical-session.target)
        loop {
            // X11 health check (periodic, only if connected)
            if !x11_needs_reconnect && last_x11_health_check.elapsed() >= x11_health_interval {
                last_x11_health_check = std::time::Instant::now();
                if !self.x11_is_alive() {
                    tracing::warn!("X11 connection lost, will attempt reconnect");
                    self.conn = None;
                    self.animation = None; // Animation renderer is now invalid
                    x11_needs_reconnect = true;
                    last_x11_reconnect = std::time::Instant::now();
                }
            }

            // Compute X11 reconnection delay
            let x11_reconnect_delay = if x11_needs_reconnect {
                let elapsed = last_x11_reconnect.elapsed();
                if elapsed < x11_reconnect_backoff {
                    Some(x11_reconnect_backoff - elapsed)
                } else {
                    Some(Duration::ZERO)
                }
            } else {
                None
            };

            // Compute gar reconnection delay (if needed) before select
            let reconnect_delay = if gar_needs_reconnect {
                let elapsed = last_gar_reconnect.elapsed();
                if elapsed < gar_reconnect_backoff {
                    Some(gar_reconnect_backoff - elapsed)
                } else {
                    Some(Duration::ZERO)
                }
            } else {
                None
            };

            tokio::select! {
                // SIGTERM - graceful shutdown
                _ = sigterm.recv() => {
                    tracing::info!("Received SIGTERM, shutting down...");
                    break;
                }

                // SIGINT (Ctrl+C) - graceful shutdown
                _ = sigint.recv() => {
                    tracing::info!("Received SIGINT, shutting down...");
                    break;
                }

                // SIGHUP - reload configuration
                _ = sighup.recv() => {
                    tracing::info!("Received SIGHUP, reloading configuration...");
                    if let Err(e) = self.reload_config() {
                        tracing::error!("Failed to reload config: {}", e);
                    }
                }

                // IPC client connection
                result = server.accept() => {
                    match result {
                        Ok(stream) => {
                            if let Err(e) = self.handle_client(stream).await {
                                tracing::debug!("Client error: {}", e);
                            }
                            // Update slideshow timer if interval changed
                            next_slideshow = self.state.slideshow_interval
                                .map(|d| tokio::time::Instant::now() + d);
                            // Update animation timer if animation started
                            if self.animation.is_some() && next_animation_frame.is_none() {
                                next_animation_frame = Some(tokio::time::Instant::now());
                            } else if self.animation.is_none() {
                                next_animation_frame = None;
                            }
                        }
                        Err(e) => {
                            tracing::warn!("Accept error: {}", e);
                        }
                    }
                }

                // Animation frame timer (highest priority when active)
                _ = async {
                    match (next_animation_frame, self.state.paused, &self.animation) {
                        (Some(deadline), false, Some(_)) => {
                            tokio::time::sleep_until(deadline).await;
                        }
                        _ => {
                            std::future::pending::<()>().await;
                        }
                    }
                } => {
                    if let Err(e) = self.render_animation_frame() {
                        tracing::warn!("Animation frame render failed: {}", e);
                        // Stop animation on error
                        self.animation = None;
                        next_animation_frame = None;
                    } else if let Some(ref anim) = self.animation {
                        // Schedule next frame
                        let delay = anim.current_delay();
                        next_animation_frame = Some(tokio::time::Instant::now() + delay);
                    }
                }

                // Slideshow timer (only if enabled, not paused, and no animation)
                _ = async {
                    match (next_slideshow, self.state.paused, &self.animation) {
                        (Some(deadline), false, None) => {
                            tokio::time::sleep_until(deadline).await;
                        }
                        _ => {
                            std::future::pending::<()>().await;
                        }
                    }
                } => {
                    if let Err(e) = self.advance_slideshow() {
                        tracing::warn!("Slideshow advance failed: {}", e);
                    }
                    // Reset timer using current interval
                    next_slideshow = self.state.slideshow_interval
                        .map(|d| tokio::time::Instant::now() + d);
                }

                // gar workspace/monitor events (only if connected)
                event = async {
                    if let Some(ref mut client) = gar_client {
                        client.read_event().await
                    } else {
                        std::future::pending().await
                    }
                } => {
                    match event {
                        Ok(event) => {
                            // Reset backoff on successful event
                            gar_reconnect_backoff = Duration::from_secs(1);
                            if let Err(e) = self.handle_gar_event(event) {
                                tracing::warn!("gar event handling failed: {}", e);
                            }
                        }
                        Err(e) => {
                            tracing::debug!("gar connection lost: {}", e);
                            gar_client = None;
                            gar_needs_reconnect = true;
                            last_gar_reconnect = std::time::Instant::now();
                        }
                    }
                }

                // X11 reconnection timer (only when disconnected)
                _ = async {
                    if let Some(delay) = x11_reconnect_delay {
                        tokio::time::sleep(delay).await;
                    } else {
                        std::future::pending::<()>().await;
                    }
                } => {
                    tracing::debug!("Attempting to reconnect to X11...");
                    last_x11_reconnect = std::time::Instant::now();

                    if self.try_connect_x11() {
                        x11_needs_reconnect = false;
                        x11_reconnect_backoff = Duration::from_secs(1);
                        tracing::info!("Reconnected to X11");

                        // Re-apply wallpaper after reconnection
                        if let Err(e) = self.reapply_wallpaper() {
                            tracing::warn!("Failed to re-apply wallpaper after X11 reconnect: {}", e);
                        }
                    } else {
                        // Exponential backoff, max 30 seconds
                        x11_reconnect_backoff = (x11_reconnect_backoff * 2).min(x11_max_backoff);
                        tracing::debug!("X11 reconnection failed, next attempt in {:?}", x11_reconnect_backoff);
                    }
                }

                // gar reconnection timer (only when disconnected)
                _ = async {
                    if let Some(delay) = reconnect_delay {
                        tokio::time::sleep(delay).await;
                    } else {
                        std::future::pending::<()>().await;
                    }
                } => {
                    tracing::debug!("Attempting to reconnect to gar...");
                    last_gar_reconnect = std::time::Instant::now();

                    match self.try_connect_gar().await {
                        Some(client) => {
                            gar_client = Some(client);
                            gar_needs_reconnect = false;
                            gar_reconnect_backoff = Duration::from_secs(1);
                            tracing::info!("Reconnected to gar");
                        }
                        None => {
                            // Exponential backoff, max 60 seconds
                            gar_reconnect_backoff = (gar_reconnect_backoff * 2).min(gar_max_backoff);
                            tracing::debug!("gar reconnection failed, next attempt in {:?}", gar_reconnect_backoff);
                        }
                    }
                }
            }
        }

        // Graceful shutdown complete
        // PID file will be removed by PidFileGuard drop
        tracing::info!("Daemon shutdown complete");
        Ok(())
    }

    /// Render the next animation frame
    fn render_animation_frame(&mut self) -> Result<()> {
        // Access both fields directly to allow split borrowing
        let conn = self.conn.as_mut()
            .ok_or_else(|| anyhow::anyhow!("X11 connection not available"))?;
        let anim = self.animation.as_mut()
            .ok_or_else(|| anyhow::anyhow!("No active animation"))?;

        // Render current frame
        let frame = &anim.scaled_frames[anim.current_frame];
        anim.renderer.render_and_present(conn, frame)?;

        // Advance to next frame
        anim.advance();

        Ok(())
    }

    /// Handle a single client connection
    async fn handle_client(&mut self, stream: UnixStream) -> Result<()> {
        let mut client = IpcClient::new(stream);

        // Single request-response per connection (stateless)
        if let Some(cmd) = client.read_command().await? {
            let response = self.handle_command(cmd);
            client.send_response(&response).await?;
        }

        Ok(())
    }

    /// Handle an IPC command
    fn handle_command(&mut self, cmd: Command) -> Response {
        match cmd {
            Command::Set { source, mode, monitor: _, interval_secs, shuffle, animate, max_fps, span } => {
                let scale_mode = mode.unwrap_or(self.state.config.general.mode);

                // Stop any existing animation first
                self.animation = None;

                // Detect animated formats from URL/path
                let source_lower = source.to_lowercase();

                // Formats that are almost always animated — auto-detect
                let auto_animate = source_lower.ends_with(".gif")
                    || source_lower.contains(".gif?")
                    || source_lower.contains("/gif/")
                    || source_lower.ends_with(".webp")
                    || source_lower.contains(".webp?")
                    || source_lower.contains("/webp/")
                    || source_lower.ends_with(".apng")
                    || source_lower.contains(".apng?")
                    || source_lower.ends_with(".mp4")
                    || source_lower.ends_with(".webm")
                    || source_lower.ends_with(".mkv")
                    || source_lower.ends_with(".avi")
                    || source_lower.ends_with(".mov")
                    || source_lower.ends_with(".m4v");

                // PNG needs explicit --animate (most PNGs aren't APNG)
                let png_animate = animate && (source_lower.ends_with(".png"));

                // Auto-animate known formats; --animate forces attempt on .png
                let should_animate = auto_animate || png_animate;

                if should_animate {
                    // Try to start animation
                    match self.start_animation(&source, scale_mode, max_fps) {
                        Ok(_) => {
                            tracing::info!("Animation started: {}", source);
                            return Response::ok();
                        }
                        Err(e) => {
                            tracing::warn!("Failed to start animation: {}, falling back to static", e);
                            // Fall through to static handling
                        }
                    }
                }

                // Set up slideshow with the new source (static image)
                match self.set_wallpaper_with_options(&source, scale_mode, shuffle, interval_secs, span) {
                    Ok(_) => {
                        // Update slideshow interval
                        self.state.slideshow_interval = interval_secs.map(Duration::from_secs);

                        if let Some(secs) = interval_secs {
                            tracing::info!("Slideshow started: {} second interval", secs);
                        }

                        Response::ok()
                    }
                    Err(e) => Response::error(e.to_string()),
                }
            }
            Command::SetWorkspace { workspace, source, mode } => {
                let scale_mode = mode.unwrap_or(self.state.config.general.mode);
                // Only set if we're on this workspace (default to per-monitor)
                if self.state.current_workspace == workspace {
                    match self.set_wallpaper_from_source(&source, scale_mode, false, false) {
                        Ok(_) => Response::ok(),
                        Err(e) => Response::error(e.to_string()),
                    }
                } else {
                    Response::ok() // Ignore, we're not on this workspace
                }
            }
            Command::Next { .. } => {
                match self.advance_slideshow() {
                    Ok(_) => Response::ok(),
                    Err(e) => Response::error(e.to_string()),
                }
            }
            Command::Prev { .. } => {
                match self.prev_slideshow() {
                    Ok(_) => Response::ok(),
                    Err(e) => Response::error(e.to_string()),
                }
            }
            Command::Random { .. } => {
                match self.random_wallpaper() {
                    Ok(_) => Response::ok(),
                    Err(e) => Response::error(e.to_string()),
                }
            }
            Command::Status => {
                Response::ok_with_data(self.get_status())
            }
            Command::Pause => {
                self.state.paused = true;
                tracing::info!("Slideshow paused");
                Response::ok()
            }
            Command::Resume => {
                self.state.paused = false;
                tracing::info!("Slideshow resumed");
                Response::ok()
            }
            Command::Toggle => {
                self.state.paused = !self.state.paused;
                tracing::info!("Slideshow {}", if self.state.paused { "paused" } else { "resumed" });
                Response::ok_with_data(serde_json::json!({ "paused": self.state.paused }))
            }
            Command::Reload => {
                match self.reload_config() {
                    Ok(_) => Response::ok(),
                    Err(e) => Response::error(e.to_string()),
                }
            }
            Command::ClearCache => {
                if let Some(ref mut cache) = self.cache {
                    match cache.clear() {
                        Ok(_) => {
                            tracing::info!("Cache cleared");
                            Response::ok()
                        }
                        Err(e) => Response::error(format!("Failed to clear cache: {}", e)),
                    }
                } else {
                    Response::ok() // No cache to clear
                }
            }
            Command::List { source } => {
                match self.list_source(&source) {
                    Ok(images) => Response::ok_with_data(serde_json::json!(images)),
                    Err(e) => Response::error(e.to_string()),
                }
            }
            Command::Subscribe { .. } | Command::Unsubscribe { .. } => {
                // Subscriptions not yet implemented
                Response::error("Subscriptions not yet implemented")
            }
            Command::QueryMonitors => {
                let monitors = self.get_monitors();
                Response::ok_with_data(serde_json::json!({ "monitors": monitors }))
            }
            Command::QueryCurrent => {
                let current = self.get_current_wallpaper_info();
                Response::ok_with_data(current)
            }
            Command::SetMonitor { monitor, source, mode } => {
                let scale_mode = mode.unwrap_or(self.state.config.general.mode);
                match self.set_monitor_wallpaper(&monitor, &source, scale_mode) {
                    Ok(_) => Response::ok(),
                    Err(e) => Response::error(e.to_string()),
                }
            }
        }
    }

    /// Handle a gar event
    fn handle_gar_event(&mut self, event: GarEvent) -> Result<()> {
        match event {
            GarEvent::Workspace { current, previous } => {
                tracing::debug!("Workspace changed: {} -> {}", previous, current);
                self.on_workspace_change(current)?;
            }
            GarEvent::Monitor { name, action } => {
                tracing::info!("Monitor event: {} {}", name, action);
                self.on_monitor_change(&name, &action)?;
            }
            GarEvent::Focus { .. } => {
                // Ignore focus events
            }
            GarEvent::Unknown => {
                // Ignore unknown events
            }
        }
        Ok(())
    }

    /// Handle monitor hotplug event from gar
    fn on_monitor_change(&mut self, name: &str, action: &str) -> Result<()> {
        match action {
            "added" => {
                tracing::info!("Monitor added: {} - re-applying wallpapers", name);
                // Re-apply wallpapers to all monitors when one is added
                // This ensures the new monitor gets a wallpaper
                self.refresh_wallpapers()?;
            }
            "removed" => {
                tracing::info!("Monitor removed: {}", name);
                // Remove monitor from our state if we're tracking per-monitor wallpapers
                self.state.monitors.remove(name);
            }
            "changed" => {
                tracing::info!("Monitor changed: {} - re-applying wallpapers", name);
                // Resolution or position changed, re-apply wallpapers
                self.refresh_wallpapers()?;
            }
            _ => {
                tracing::debug!("Unknown monitor action: {} for {}", action, name);
            }
        }
        Ok(())
    }

    /// Refresh wallpapers on all monitors
    fn refresh_wallpapers(&mut self) -> Result<()> {
        // If we have an active animation, restart it (picks up new screen size)
        if self.animation.is_some() {
            tracing::debug!("Active animation detected, will restart on next frame");
            // Animation will pick up new screen dimensions on next render
        }

        // Check if we have per-monitor wallpapers
        let conn = match self.conn() {
            Ok(c) => c,
            Err(_) => return Ok(()), // No connection, nothing to refresh
        };
        let monitors = Monitor::get_all(conn).unwrap_or_default();

        if monitors.len() > 1 && !self.state.monitors.is_empty() {
            // Multiple monitors with per-monitor wallpapers: use compositor
            self.composite_all_wallpapers(&monitors)?;
            tracing::debug!("Wallpapers refreshed (composited {} monitors)", monitors.len());
        } else if let Some(ref playlist) = self.state.playlist {
            // Single monitor or no per-monitor state: use global wallpaper
            if let Some(current) = playlist.current() {
                let mode = playlist.mode;
                let current = current.to_string();
                self.set_wallpaper(&current, mode)?;
                tracing::debug!("Wallpapers refreshed");
            }
        } else if !self.state.config.default.source.is_empty() {
            // Fall back to default wallpaper
            self.apply_default_wallpaper()?;
        }

        Ok(())
    }

    /// Try to connect to gar IPC
    async fn try_connect_gar(&self) -> Option<GarIpcClient> {
        match GarIpcClient::connect().await {
            Ok(mut client) => {
                // Subscribe to workspace and monitor events
                if client.subscribe(&["workspace", "monitor"]).await.is_ok() {
                    tracing::info!("Connected to gar IPC (subscribed to workspace, monitor events)");
                    Some(client)
                } else {
                    tracing::debug!("Failed to subscribe to gar events");
                    None
                }
            }
            Err(_) => {
                tracing::debug!("gar not running, workspace integration disabled");
                None
            }
        }
    }

    /// Apply the default wallpaper from config
    fn apply_default_wallpaper(&mut self) -> Result<()> {
        let source = self.state.config.default.source.clone();
        let mode = self.state.config.default.mode;
        let shuffle = self.state.config.default.slideshow
            .as_ref()
            .map(|s| s.shuffle)
            .unwrap_or(false);

        if source.is_empty() {
            return Ok(());
        }

        // Default to per-monitor (span = false)
        self.set_wallpaper_from_source(&source, mode, shuffle, false)
    }

    /// Start an animated image playback (GIF, WebP, APNG, or video)
    fn start_animation(&mut self, source: &str, mode: ScaleMode, max_fps: u32) -> Result<()> {
        let is_remote = source.starts_with("http://") || source.starts_with("https://");
        let source_lower = source.to_lowercase();

        // Detect format from extension/URL
        let is_webp = source_lower.ends_with(".webp")
            || source_lower.contains(".webp?")
            || source_lower.contains("/webp/");
        let is_apng = source_lower.ends_with(".apng")
            || source_lower.contains(".apng?");
        let is_png = source_lower.ends_with(".png");

        // Check for video formats
        #[cfg(feature = "video")]
        let is_video = {
            let expanded = shellexpand::tilde(source);
            !is_remote && is_video_file(expanded.as_ref())
        };
        #[cfg(not(feature = "video"))]
        let is_video = false;

        // Handle video separately (can't load into memory efficiently)
        #[cfg(feature = "video")]
        if is_video {
            return self.start_video_animation(source, mode, max_fps);
        }

        // Load animation data for image formats
        let bytes = if is_remote {
            tracing::info!("Fetching remote animation: {}", source);
            self.fetch_bytes(source)?
        } else {
            let expanded = shellexpand::tilde(source);
            std::fs::read(expanded.as_ref())
                .with_context(|| format!("Failed to read: {}", source))?
        };

        // Load frames based on format - clone frames to avoid lifetime issues
        let (frames, frame_count, avg_fps, format_name): (Vec<AnimationFrame>, usize, f64, &str) = if is_webp {
            let webp = AnimatedWebP::load_from_bytes(&bytes)?;
            if !webp.is_animated() {
                anyhow::bail!("WebP is not animated (single frame)");
            }
            let fc = webp.frame_count();
            let fps = webp.average_fps();
            (webp.frames().to_vec(), fc, fps, "WebP")
        } else if is_apng || is_png {
            // Try APNG first for .png files (might be animated)
            match AnimatedPng::load_from_bytes(&bytes) {
                Ok(apng) if apng.is_animated() => {
                    let fc = apng.frame_count();
                    let fps = apng.average_fps();
                    (apng.frames().to_vec(), fc, fps, "APNG")
                }
                Ok(_) => {
                    anyhow::bail!("PNG is not animated");
                }
                Err(e) if is_apng => {
                    // .apng extension but failed to load as APNG
                    anyhow::bail!("Failed to load APNG: {}", e);
                }
                Err(_) => {
                    // .png extension but not an APNG, try as static
                    anyhow::bail!("PNG is not animated (use without --animate)");
                }
            }
        } else {
            // Default to GIF
            let gif = AnimatedGif::load_from_bytes(&bytes)?;
            if !gif.is_animated() {
                anyhow::bail!("GIF is not animated (single frame)");
            }
            let fc = gif.frame_count();
            let fps = gif.average_fps();
            (gif.frames().to_vec(), fc, fps, "GIF")
        };

        // Suppress warning when video feature is disabled
        let _ = is_video;

        // Create animation renderer
        let conn = self.conn()?;
        let renderer = AnimationRenderer::new(conn)?;
        let (width, height) = conn.screen_dimensions();

        tracing::info!(
            "Animation loaded: {} frames, {:.1} FPS ({})",
            frame_count,
            avg_fps,
            format_name
        );

        self.animation = Some(ActiveAnimation::from_frames(
            &frames,
            renderer,
            max_fps,
            mode,
            source.to_string(),
            width as u32,
            height as u32,
        ));

        Ok(())
    }

    /// Start video animation playback
    #[cfg(feature = "video")]
    fn start_video_animation(&mut self, source: &str, mode: ScaleMode, max_fps: u32) -> Result<()> {
        let expanded = shellexpand::tilde(source);
        let path = std::path::Path::new(expanded.as_ref());

        tracing::info!("Opening video: {}", path.display());

        let mut decoder = VideoDecoder::open(path)?;
        let info = decoder.info().clone();
        let frame_delay = decoder.frame_delay();

        tracing::info!(
            "Video: {}x{}, {:.1} FPS, {:.1}s duration, ~{} frames ({})",
            info.width,
            info.height,
            info.frame_rate,
            info.duration,
            info.frame_count,
            info.codec
        );

        // Limit frames for memory efficiency (max ~30 seconds at target fps)
        let max_frames = (30.0 * info.frame_rate.min(max_fps as f64)) as usize;
        let frame_limit = max_frames.max(100).min(info.frame_count);

        // Extract frames
        let mut frames = Vec::with_capacity(frame_limit);
        while let Some(decoded) = decoder.next_frame()? {
            frames.push(AnimationFrame {
                image: decoded.image,
                delay: frame_delay,
            });

            if frames.len() >= frame_limit {
                tracing::debug!("Reached frame limit ({}), stopping decode", frame_limit);
                break;
            }
        }

        if frames.is_empty() {
            anyhow::bail!("Video has no decodable frames");
        }

        let frame_count = frames.len();
        let avg_fps = info.frame_rate;

        // Create animation renderer
        let conn = self.conn()?;
        let renderer = AnimationRenderer::new(conn)?;
        let (width, height) = conn.screen_dimensions();

        tracing::info!(
            "Video loaded: {} frames, {:.1} FPS",
            frame_count,
            avg_fps
        );

        self.animation = Some(ActiveAnimation::from_frames(
            &frames,
            renderer,
            max_fps,
            mode,
            source.to_string(),
            width as u32,
            height as u32,
        ));

        Ok(())
    }

    /// Fetch raw bytes from a URL (with caching)
    fn fetch_bytes(&mut self, url: &str) -> Result<Vec<u8>> {
        // Check cache first
        if let Some(ref mut cache) = self.cache {
            if let Some(cached_path) = cache.get(url) {
                tracing::debug!("Cache hit: {}", url);
                return std::fs::read(&cached_path)
                    .context("Failed to read cached file");
            }
        }

        // Fetch from network
        tracing::debug!("Cache miss, fetching: {}", url);
        let client = reqwest::blocking::Client::builder()
            .user_agent("garbg/0.1")
            .build()?;

        let response = client.get(url).send()?;
        let status = response.status();

        if !status.is_success() {
            anyhow::bail!("HTTP error {}: {}", status, url);
        }

        // Get ETag for conditional requests
        let etag = response.headers()
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .map(String::from);

        let bytes = response.bytes()?.to_vec();

        // Store in cache
        if let Some(ref mut cache) = self.cache {
            if let Err(e) = cache.store(url, &bytes, etag) {
                tracing::warn!("Failed to cache {}: {}", url, e);
            } else {
                tracing::debug!("Cached: {}", url);
            }
        }

        Ok(bytes)
    }

    /// Set wallpaper with full options (used by IPC Set command)
    fn set_wallpaper_with_options(
        &mut self,
        source: &str,
        mode: ScaleMode,
        shuffle: bool,
        _interval_secs: Option<u64>,
        span: bool,
    ) -> Result<()> {
        self.set_wallpaper_from_source(source, mode, shuffle, span)
    }

    /// Set wallpaper from a source (file, directory, or URL)
    fn set_wallpaper_from_source(&mut self, source: &str, mode: ScaleMode, shuffle: bool, span: bool) -> Result<()> {
        // Expand path
        let expanded = shellexpand::tilde(source);
        let path = std::path::Path::new(expanded.as_ref());

        // Check if it's a directory
        if path.is_dir() {
            // Create a playlist from the directory
            let images = self.list_local_directory(&expanded)?;
            if images.is_empty() {
                anyhow::bail!("No images found in directory: {}", source);
            }

            let mut playlist = PlaylistState::new(
                source.to_string(),
                detect_source_type(source),
                images,
                shuffle,
                mode,
            );

            if shuffle {
                playlist.reshuffle();
            }

            let first = playlist.current().unwrap_or("").to_string();
            playlist.save()?;
            self.state.playlist = Some(playlist);

            self.set_wallpaper_with_span(&first, mode, span)?;

            tracing::info!(
                "Playlist loaded: {} images{}",
                self.state.playlist.as_ref().map(|p| p.len()).unwrap_or(0),
                if shuffle { " (shuffled)" } else { "" }
            );
        } else if source.starts_with("http://") || source.starts_with("https://") {
            // Remote URL
            let image = self.fetch_image(source)?;
            self.set_image_with_span(&image, source, mode, span)?;
        } else {
            // Single file
            self.set_wallpaper_with_span(source, mode, span)?;
        }

        Ok(())
    }

    /// Set wallpaper from a local file
    pub fn set_wallpaper(&mut self, source: &str, mode: ScaleMode) -> Result<()> {
        self.set_wallpaper_with_span(source, mode, false)
    }

    /// Set wallpaper from a local file with span option
    pub fn set_wallpaper_with_span(&mut self, source: &str, mode: ScaleMode, span: bool) -> Result<()> {
        let expanded = shellexpand::tilde(source);
        let image = ImageLoader::load_file(expanded.as_ref())?;
        self.set_image_with_span(&image, source, mode, span)
    }

    /// Set wallpaper from an image with span option
    fn set_image_with_span(&mut self, image: &image::RgbaImage, source: &str, mode: ScaleMode, span: bool) -> Result<()> {
        let conn = self.conn()?;
        let monitors = Monitor::get_all(conn).unwrap_or_default();

        if !span && monitors.len() > 1 {
            // Per-monitor mode: scale wallpaper to each monitor individually
            let compositor = Compositor::new(&monitors);
            let wallpapers = Compositor::create_wallpapers_uniform(&monitors, image, mode);
            let composited = compositor.composite(&wallpapers);
            self.conn_mut()?.set_wallpaper(&composited)?;

            tracing::info!(
                "Wallpaper set on {} monitors: {} (mode: {})",
                monitors.len(),
                source,
                mode
            );
        } else {
            // Span mode or single monitor: scale to full screen
            let conn = self.conn_mut()?;
            let (width, height) = conn.screen_dimensions();
            let scaled = scale_image(image, width as u32, height as u32, mode);
            conn.set_wallpaper(&scaled)?;

            tracing::info!("Wallpaper set: {} (mode: {})", source, mode);
        }

        Ok(())
    }

    /// Fetch image from URL (with caching)
    fn fetch_image(&mut self, url: &str) -> Result<image::RgbaImage> {
        // Use cached bytes
        let bytes = self.fetch_bytes(url)?;
        ImageLoader::load_bytes(&bytes, None)
    }

    /// Advance to the next wallpaper in the slideshow
    fn advance_slideshow(&mut self) -> Result<()> {
        // Reload state in case it was modified externally
        if let Some(ref mut playlist) = self.state.playlist {
            playlist.reload()?;
        } else if let Some(playlist) = PlaylistState::load()? {
            self.state.playlist = Some(playlist);
        }

        // Extract what we need from the playlist first
        let (next, mode, current_index, total) = {
            let playlist = self.state.playlist.as_mut()
                .ok_or_else(|| anyhow::anyhow!("No active playlist"))?;

            let next = playlist.next().to_string();
            let mode = playlist.mode;
            let current_index = playlist.current_index;
            let total = playlist.len();
            playlist.save()?;

            (next, mode, current_index, total)
        };

        self.set_wallpaper(&next, mode)?;

        tracing::info!(
            "Slideshow [{}/{}]: {}",
            current_index + 1,
            total,
            next
        );

        Ok(())
    }

    /// Go to the previous wallpaper in the slideshow
    fn prev_slideshow(&mut self) -> Result<()> {
        // Reload state in case it was modified externally
        if let Some(ref mut playlist) = self.state.playlist {
            playlist.reload()?;
        } else if let Some(playlist) = PlaylistState::load()? {
            self.state.playlist = Some(playlist);
        }

        // Extract what we need from the playlist first
        let (prev, mode, current_index, total) = {
            let playlist = self.state.playlist.as_mut()
                .ok_or_else(|| anyhow::anyhow!("No active playlist"))?;

            let prev = playlist.prev().to_string();
            let mode = playlist.mode;
            let current_index = playlist.current_index;
            let total = playlist.len();
            playlist.save()?;

            (prev, mode, current_index, total)
        };

        self.set_wallpaper(&prev, mode)?;

        tracing::info!(
            "Slideshow [{}/{}]: {}",
            current_index + 1,
            total,
            prev
        );

        Ok(())
    }

    /// Set a random wallpaper from the current playlist
    fn random_wallpaper(&mut self) -> Result<()> {
        if let Some(ref mut playlist) = self.state.playlist {
            use rand::Rng;
            let idx = rand::thread_rng().gen_range(0..playlist.len());
            playlist.current_index = idx;
            let img = playlist.images[idx].clone();
            let mode = playlist.mode;
            playlist.save()?;

            self.set_wallpaper(&img, mode)?;
        } else {
            anyhow::bail!("No active playlist");
        }

        Ok(())
    }

    /// Handle workspace change
    pub fn on_workspace_change(&mut self, workspace: usize) -> Result<()> {
        self.state.current_workspace = workspace;

        // Check if this workspace has a specific wallpaper
        let ws_config = self.state.config.workspaces
            .iter()
            .find(|w| w.id == workspace)
            .cloned();

        if let Some(config) = ws_config {
            let mode = config.mode.unwrap_or(self.state.config.general.mode);
            // Default to per-monitor (span = false)
            self.set_wallpaper_from_source(&config.source, mode, false, false)?;
        }

        Ok(())
    }

    /// Reload configuration
    fn reload_config(&mut self) -> Result<()> {
        let config = Config::load_default()?;

        // Update slideshow interval from new config
        self.state.slideshow_interval = config.default.slideshow
            .as_ref()
            .filter(|s| s.enabled)
            .map(|s| s.interval);

        self.state.config = config;
        tracing::info!("Configuration reloaded");

        // Re-apply default wallpaper
        self.apply_default_wallpaper()?;

        Ok(())
    }

    /// Get current status as JSON
    fn get_status(&self) -> serde_json::Value {
        let playlist_info = self.state.playlist.as_ref().map(|p| {
            serde_json::json!({
                "source": p.source,
                "current_index": p.current_index,
                "total": p.len(),
                "current_image": p.current(),
                "shuffled": p.shuffled,
                "mode": format!("{}", p.mode),
            })
        });

        let interval_secs = self.state.slideshow_interval.map(|d| d.as_secs());

        serde_json::json!({
            "workspace": self.state.current_workspace,
            "paused": self.state.paused,
            "interval_secs": interval_secs,
            "playlist": playlist_info,
        })
    }

    /// List images from a source
    fn list_source(&self, source: &str) -> Result<Vec<String>> {
        let expanded = shellexpand::tilde(source);
        self.list_local_directory(&expanded)
    }

    /// List images in a local directory (recursively)
    fn list_local_directory(&self, path: &str) -> Result<Vec<String>> {
        let dir_path = std::path::Path::new(path);

        if dir_path.is_file() {
            return Ok(vec![path.to_string()]);
        }

        if !dir_path.is_dir() {
            anyhow::bail!("Path is not a file or directory: {}", path);
        }

        let mut images = Vec::new();
        Self::collect_images_recursive(dir_path, &mut images)?;
        images.sort();
        Ok(images)
    }

    /// Recursively collect supported image files from a directory.
    fn collect_images_recursive(dir: &std::path::Path, images: &mut Vec<String>) -> Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let entry_path = entry.path();
            if entry_path.is_dir() {
                Self::collect_images_recursive(&entry_path, images)?;
            } else if entry_path.is_file() && ImageLoader::is_supported_format(&entry_path) {
                images.push(entry_path.to_string_lossy().to_string());
            }
        }
        Ok(())
    }

    /// Get connected monitors info via RandR
    fn get_monitors(&self) -> Vec<serde_json::Value> {
        let conn = match self.conn.as_ref() {
            Some(c) => c,
            None => {
                tracing::warn!("X11 connection not available for monitor detection");
                return vec![serde_json::json!({
                    "name": "default",
                    "width": 1920,
                    "height": 1080,
                    "x": 0,
                    "y": 0,
                    "primary": true,
                })];
            }
        };

        match Monitor::get_all(conn) {
            Ok(monitors) if !monitors.is_empty() => {
                monitors.iter().map(|m| {
                    serde_json::json!({
                        "name": m.name,
                        "width": m.width,
                        "height": m.height,
                        "x": m.x,
                        "y": m.y,
                        "primary": m.primary,
                    })
                }).collect()
            }
            Ok(_) => {
                // No monitors detected, fall back to screen dimensions
                tracing::debug!("No monitors detected via RandR, using screen dimensions");
                let (width, height) = conn.screen_dimensions();
                vec![serde_json::json!({
                    "name": "default",
                    "width": width,
                    "height": height,
                    "x": 0,
                    "y": 0,
                    "primary": true,
                })]
            }
            Err(e) => {
                // RandR failed, fall back to screen dimensions
                tracing::warn!("RandR detection failed: {}, using screen dimensions", e);
                let (width, height) = conn.screen_dimensions();
                vec![serde_json::json!({
                    "name": "default",
                    "width": width,
                    "height": height,
                    "x": 0,
                    "y": 0,
                    "primary": true,
                })]
            }
        }
    }

    /// Get current wallpaper info
    fn get_current_wallpaper_info(&self) -> serde_json::Value {
        let current_image = self.state.playlist.as_ref()
            .and_then(|p| p.current().map(|s| s.to_string()));

        let mode = self.state.playlist.as_ref()
            .map(|p| format!("{}", p.mode))
            .unwrap_or_else(|| format!("{}", self.state.config.general.mode));

        let animation_active = self.animation.is_some();

        serde_json::json!({
            "source": current_image,
            "mode": mode,
            "paused": self.state.paused,
            "animation_active": animation_active,
            "workspace": self.state.current_workspace,
        })
    }

    /// Set wallpaper for a specific monitor
    fn set_monitor_wallpaper(&mut self, monitor_name: &str, source: &str, mode: ScaleMode) -> Result<()> {
        // Get detected monitors
        let monitors = Monitor::get_all(self.conn()?)?;

        if monitors.is_empty() {
            // Fall back to setting global wallpaper if no monitors detected
            tracing::warn!("No monitors detected, setting wallpaper globally");
            return self.set_wallpaper(source, mode);
        }

        // Find the target monitor
        let target_monitor = monitors.iter()
            .find(|m| m.name == monitor_name)
            .ok_or_else(|| anyhow::anyhow!(
                "Monitor '{}' not found. Available: {:?}",
                monitor_name,
                monitors.iter().map(|m| &m.name).collect::<Vec<_>>()
            ))?;

        // Load and scale the image for this monitor
        let expanded = shellexpand::tilde(source);
        let image = ImageLoader::load_file(expanded.as_ref())?;

        // Store wallpaper state for this monitor
        self.state.monitors.insert(monitor_name.to_string(), MonitorWallpaper {
            name: monitor_name.to_string(),
            source: source.to_string(),
            mode,
        });

        tracing::info!(
            "Set wallpaper for monitor {}: {} (mode: {})",
            monitor_name,
            source,
            mode
        );

        // If only one monitor, set wallpaper directly
        if monitors.len() == 1 {
            let scaled = scale_image(
                &image,
                target_monitor.width as u32,
                target_monitor.height as u32,
                mode,
            );
            return self.conn_mut()?.set_wallpaper(&scaled);
        }

        // Multiple monitors: composite all wallpapers
        self.composite_all_wallpapers(&monitors)
    }

    /// Composite wallpapers for all monitors and set the result
    fn composite_all_wallpapers(&mut self, monitors: &[Monitor]) -> Result<()> {
        if monitors.is_empty() {
            return Ok(());
        }

        let compositor = Compositor::new(monitors);
        let mut wallpapers = Vec::new();

        // Get global default wallpaper for monitors without specific wallpaper
        let default_image = if !self.state.config.default.source.is_empty() {
            let expanded = shellexpand::tilde(&self.state.config.default.source);
            ImageLoader::load_file(expanded.as_ref()).ok()
        } else if let Some(ref playlist) = self.state.playlist {
            playlist.current()
                .and_then(|path| {
                    let expanded = shellexpand::tilde(path);
                    ImageLoader::load_file(expanded.as_ref()).ok()
                })
        } else {
            None
        };

        for monitor in monitors {
            let image = if let Some(wp_state) = self.state.monitors.get(&monitor.name) {
                // Use the per-monitor wallpaper
                let expanded = shellexpand::tilde(&wp_state.source);
                match ImageLoader::load_file(expanded.as_ref()) {
                    Ok(img) => img,
                    Err(e) => {
                        tracing::warn!("Failed to load wallpaper for {}: {}", monitor.name, e);
                        if let Some(ref def) = default_image {
                            def.clone()
                        } else {
                            // Create a black image as fallback
                            image::RgbaImage::from_pixel(
                                monitor.width as u32,
                                monitor.height as u32,
                                image::Rgba([0, 0, 0, 255])
                            )
                        }
                    }
                }
            } else if let Some(ref def) = default_image {
                // Use the default wallpaper
                def.clone()
            } else {
                // No wallpaper, use black
                image::RgbaImage::from_pixel(
                    monitor.width as u32,
                    monitor.height as u32,
                    image::Rgba([0, 0, 0, 255])
                )
            };

            let mode = self.state.monitors.get(&monitor.name)
                .map(|wp| wp.mode)
                .unwrap_or(self.state.config.general.mode);

            wallpapers.push(Compositor::create_monitor_wallpaper(monitor, &image, mode));
        }

        // Composite and set
        let composited = compositor.composite(&wallpapers);
        self.conn_mut()?.set_wallpaper(&composited)?;

        tracing::debug!(
            "Composited {} monitors ({}x{})",
            monitors.len(),
            compositor.total_width,
            compositor.total_height
        );

        Ok(())
    }
}

/// RAII guard for PID file cleanup
///
/// Removes the PID file when dropped, ensuring cleanup even on panic.
struct PidFileGuard;

impl Drop for PidFileGuard {
    fn drop(&mut self) {
        if let Err(e) = pid::remove_pid_file() {
            tracing::warn!("Failed to remove PID file on shutdown: {}", e);
        } else {
            tracing::debug!("PID file removed on shutdown");
        }
    }
}
