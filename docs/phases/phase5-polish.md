# Phase 5: Polish & Integration

## Goal
Complete the user experience with Lua integration, CLI tool, multi-monitor support, and configuration hot-reload.

## Tasks

### 5.1 Lua Module for gar
- [ ] Create shared library loadable by gar's Lua
- [ ] Expose `garbg.set()` function
- [ ] Expose `garbg.next()`, `garbg.prev()`, `garbg.random()`
- [ ] Expose `garbg.config()` for declarative setup
- [ ] Handle IPC connection from Lua context

### 5.2 garbgctl CLI Tool
- [ ] `garbgctl set <source>` - Set wallpaper
- [ ] `garbgctl next` / `prev` - Slideshow control
- [ ] `garbgctl random` - Random wallpaper
- [ ] `garbgctl status` - Show current state
- [ ] `garbgctl reload` - Reload config
- [ ] `garbgctl pause` / `resume` - Animation control
- [ ] Pretty-printed output with colors

### 5.3 Multi-Monitor Support
- [ ] Detect monitors via RandR
- [ ] Per-monitor wallpaper configuration
- [ ] Handle monitor hotplug events
- [ ] Composite wallpapers for spanning setups
- [ ] Independent slideshows per monitor

### 5.4 Configuration Hot-Reload
- [ ] Watch config file with inotify
- [ ] Parse and validate new config
- [ ] Apply changes without restart
- [ ] SIGHUP trigger for manual reload
- [ ] Report config errors via IPC

### 5.5 Error Handling & Logging
- [ ] Structured logging with tracing
- [ ] Log levels (error, warn, info, debug, trace)
- [ ] Journal/syslog integration for daemon
- [ ] User-friendly error messages
- [ ] Detailed errors for debugging

### 5.6 Documentation
- [ ] Man page for garbg
- [ ] Man page for garbgctl
- [ ] Example configurations
- [ ] Integration guide for gar

## Deliverables
- Lua module works in gar's init.lua
- garbgctl provides full control over daemon
- Multi-monitor setups work correctly
- Config changes apply without restart

## Lua Integration Example

```lua
-- ~/.config/gar/init.lua
local garbg = require("garbg")

-- Declarative configuration
garbg.config({
    default = {
        source = "~/Pictures/wallpapers",
        mode = "fill",
        slideshow = {
            enabled = true,
            interval = 300,
            shuffle = true,
        },
    },
    workspaces = {
        [1] = "~/Pictures/workspace1.png",
        [2] = { source = "~/Videos/loop.mp4", mode = "fill" },
        [3] = "github://user/repo/wallpapers/ws3.png",
    },
    monitors = {
        ["DP-1"] = { source = "~/Pictures/wide/", mode = "fill" },
    },
})

-- Keybinds
gar.bind("mod+w", function() garbg.next() end)
gar.bind("mod+shift+w", function() garbg.random() end)

-- React to workspace changes
gar.on("workspace", function(event)
    garbg.switch_workspace(event.current)
end)
```

## garbgctl Usage

```bash
# Set wallpaper
garbgctl set ~/Pictures/wallpaper.png
garbgctl set github://user/repo/wallpapers --random

# Slideshow control
garbgctl next
garbgctl prev
garbgctl random

# Animation control
garbgctl pause
garbgctl resume

# Status and management
garbgctl status
garbgctl reload

# Output example
$ garbgctl status
garbg daemon v0.1.0
  Status: running
  Current: ~/Pictures/wallpaper.png
  Mode: fill
  Slideshow: enabled (5m interval, 12/50)
  Animation: playing
  Monitors: DP-1, HDMI-1
```

## Multi-Monitor Configuration

```toml
# ~/.config/garbg/config.toml

[[monitors]]
name = "DP-1"
source = "~/Pictures/wide/"
mode = "fill"
slideshow = true
interval = "10m"

[[monitors]]
name = "HDMI-1"
source = "~/Pictures/vertical/"
mode = "fit"

# Spanning mode (single wallpaper across all monitors)
[spanning]
enabled = false
source = "~/Pictures/ultrawide.png"
mode = "fill"
```

## Files Modified/Created
- `/garbg/garbg/src/lua/mod.rs` - Lua module
- `/garbg/garbg/src/lua/api.rs` - Lua API functions
- `/garbg/garbgctl/src/main.rs` - CLI implementation
- `/garbg/garbg/src/x11/monitors.rs` - RandR implementation
- `/garbg/garbg/src/config/watch.rs` - Config file watching
- `/garbg/docs/garbg.1.md` - Man page source
- `/garbg/docs/garbgctl.1.md` - Man page source
- `/garbg/config/default.toml` - Example configuration
