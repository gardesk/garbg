# Phase 4: Daemon Mode & gar Integration

## Goal
Implement daemon mode with IPC server, integrate with gar window manager for workspace-aware wallpapers.

## Tasks

### 4.1 Daemon Mode
- [ ] Daemonize process (fork, setsid, etc.)
- [ ] PID file management
- [ ] Signal handling (SIGHUP for reload, SIGTERM for shutdown)
- [ ] Graceful shutdown with resource cleanup

### 4.2 IPC Server
- [ ] Unix socket at `$XDG_RUNTIME_DIR/garbg.sock`
- [ ] JSON lines protocol
- [ ] Command parsing and dispatch
- [ ] Response serialization
- [ ] Client connection management

### 4.3 IPC Commands
- [ ] `set` - Set wallpaper
- [ ] `next` / `prev` - Slideshow navigation
- [ ] `random` - Random wallpaper from source
- [ ] `reload` - Reload configuration
- [ ] `pause` / `resume` - Animation/slideshow control
- [ ] `status` - Get current state
- [ ] `subscribe` - Event subscription

### 4.4 gar IPC Client
- [ ] Connect to gar's IPC socket
- [ ] Subscribe to workspace events
- [ ] Subscribe to monitor events
- [ ] Parse gar's JSON event format

### 4.5 Per-Workspace Wallpapers
- [ ] Map workspace ID to wallpaper config
- [ ] Switch wallpaper on workspace change
- [ ] Smooth transition (instant, per design decision)
- [ ] Fallback to default wallpaper

### 4.6 Slideshow/Rotation
- [ ] Timer-based rotation
- [ ] Configurable interval
- [ ] Random or sequential ordering
- [ ] Per-source playlist management

## Deliverables
- `garbg daemon` starts background daemon
- `garbgctl set /path/to/image.png` sends command to daemon
- Changing workspaces in gar changes wallpaper
- Slideshow rotates wallpapers at configured interval

## IPC Protocol

### Commands (Client -> Server)

```json
{"command": "set", "source": "/path/to/image.png", "mode": "fill"}
{"command": "next"}
{"command": "prev"}
{"command": "random"}
{"command": "reload"}
{"command": "pause"}
{"command": "resume"}
{"command": "status"}
{"command": "subscribe", "events": ["wallpaper_changed", "slideshow_advanced"]}
```

### Responses (Server -> Client)

```json
{"success": true}
{"success": true, "data": {"current": "/path/to/image.png", "mode": "fill", "paused": false}}
{"success": false, "error": "File not found"}
```

### Events (Server -> Subscribed Clients)

```json
{"event": "wallpaper_changed", "source": "/path/to/new.png", "workspace": 1}
{"event": "slideshow_advanced", "current": 5, "total": 20}
{"event": "error", "message": "Failed to fetch remote image"}
```

## gar Event Handling

```rust
// Subscribe to gar workspace events
let subscribe_msg = json!({
    "command": "subscribe",
    "events": ["workspace"]
});

// Handle workspace change
fn handle_gar_event(&mut self, event: GarEvent) {
    match event {
        GarEvent::Workspace { current, .. } => {
            if let Some(config) = self.config.workspaces.get(&current) {
                self.set_wallpaper(&config.source, config.mode);
            }
        }
        _ => {}
    }
}
```

## Files Modified/Created
- `/garbg/garbg/src/daemon/mod.rs` - Daemon module
- `/garbg/garbg/src/daemon/state.rs` - Daemon state management
- `/garbg/garbg/src/daemon/event_loop.rs` - Main event loop
- `/garbg/garbg/src/daemon/signals.rs` - Signal handling
- `/garbg/garbg/src/ipc/mod.rs` - IPC module
- `/garbg/garbg/src/ipc/server.rs` - IPC server
- `/garbg/garbg/src/ipc/protocol.rs` - Command/Response types
- `/garbg/garbg/src/ipc/gar_client.rs` - gar IPC client
