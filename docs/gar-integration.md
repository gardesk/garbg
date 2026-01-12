# garbg Integration with gar Window Manager

This guide explains how to integrate garbg (wallpaper daemon) with the gar tiling window manager.

## Quick Start

### 1. Start garbg daemon on login

Add to your `~/.config/gar/init.lua`:

```lua
-- Start garbg daemon on gar startup
gar.exec_once("garbg daemon")
```

Or if using a shell startup script (`~/.xinitrc`, `~/.xprofile`):

```bash
garbg daemon &
```

### 2. Set initial wallpaper

In your gar config:

```lua
-- Set wallpaper after daemon starts
gar.exec_once("sleep 0.5 && garbg set ~/Pictures/wallpaper.jpg")

-- Or set a slideshow directory
gar.exec_once("sleep 0.5 && garbg set ~/Pictures/wallpapers --random --interval 5m")
```

## Keybindings

Add wallpaper control keybindings to your gar config:

```lua
-- Wallpaper navigation
gar.bind("mod+bracketright", function() gar.exec("garbg next") end)     -- Mod+]
gar.bind("mod+bracketleft", function() gar.exec("garbg prev") end)      -- Mod+[
gar.bind("mod+shift+w", function() gar.exec("garbg random") end)        -- Mod+Shift+W

-- Pause/resume slideshow or animation
gar.bind("mod+shift+p", function() gar.exec("garbg toggle") end)        -- Mod+Shift+P
```

## Helper Functions

Create reusable functions in your gar config:

```lua
-- Wallpaper helper module
garbg = {
    next = function() gar.exec("garbg next") end,
    prev = function() gar.exec("garbg prev") end,
    pause = function() gar.exec("garbg pause") end,
    resume = function() gar.exec("garbg resume") end,
    toggle = function() gar.exec("garbg toggle") end,
    random = function() gar.exec("garbg random") end,

    set = function(source, opts)
        local cmd = "garbg set '" .. source .. "'"
        if opts then
            if opts.mode then cmd = cmd .. " --mode " .. opts.mode end
            if opts.random then cmd = cmd .. " --random" end
            if opts.interval then cmd = cmd .. " --interval " .. opts.interval end
            if opts.animate then cmd = cmd .. " --animate" end
        end
        gar.exec(cmd)
    end,

    set_monitor = function(monitor, source, mode)
        local cmd = "garbg set-monitor " .. monitor .. " '" .. source .. "'"
        if mode then cmd = cmd .. " --mode " .. mode end
        gar.exec(cmd)
    end,
}

-- Usage examples:
-- garbg.set("~/Pictures/wallpapers", { random = true, interval = "5m" })
-- garbg.set_monitor("DP-1", "~/left.jpg")
-- garbg.toggle()
```

## Per-Workspace Wallpapers

garbg automatically subscribes to gar workspace events. Configure per-workspace wallpapers in `~/.config/garbg/config.toml`:

```toml
[general]
mode = "fill"

[default]
source = "~/Pictures/default.jpg"

[[workspaces]]
id = 1
source = "~/Pictures/workspace1.jpg"

[[workspaces]]
id = 2
source = "~/Pictures/workspace2.jpg"

[[workspaces]]
id = 9
source = "~/Pictures/gaming.jpg"
```

When you switch workspaces in gar, garbg will automatically change the wallpaper.

## Multi-Monitor Setup

### Query available monitors

```bash
garbg query monitors
```

Output:
```json
{
  "monitors": [
    {"name": "DP-1", "width": 2560, "height": 1440, "x": 0, "y": 0, "primary": true},
    {"name": "HDMI-1", "width": 1920, "height": 1080, "x": 2560, "y": 180, "primary": false}
  ]
}
```

### Set per-monitor wallpapers

```bash
# Set different wallpapers for each monitor
garbg set-monitor DP-1 ~/Pictures/left.jpg
garbg set-monitor HDMI-1 ~/Pictures/right.jpg
```

In gar config:
```lua
gar.exec_once("sleep 0.5 && garbg set-monitor DP-1 ~/Pictures/main.jpg")
gar.exec_once("sleep 0.5 && garbg set-monitor HDMI-1 ~/Pictures/side.jpg")
```

## Monitor Hotplug

garbg subscribes to monitor events from gar. When a monitor is:
- **Added**: Wallpaper is automatically applied to the new monitor
- **Removed**: Monitor state is cleaned up
- **Changed**: Wallpapers are re-composited at new positions/resolutions

No additional configuration needed - this works automatically when connected to gar.

## Animated Wallpapers

### Animated GIFs

```bash
garbg set ~/Pictures/animated.gif --animate
```

### Video wallpapers (requires `video` feature)

```bash
garbg set ~/Videos/loop.mp4
# Videos are automatically animated (no --animate flag needed)
```

## IPC Commands

All commands can be sent to the daemon via the CLI:

| Command | Description |
|---------|-------------|
| `garbg set <source>` | Set wallpaper |
| `garbg set-monitor <name> <source>` | Set per-monitor wallpaper |
| `garbg next` | Next in slideshow |
| `garbg prev` | Previous in slideshow |
| `garbg random` | Random from playlist |
| `garbg pause` | Pause slideshow/animation |
| `garbg resume` | Resume slideshow/animation |
| `garbg toggle` | Toggle pause state |
| `garbg status` | Get current status (JSON) |
| `garbg query monitors` | List monitors (JSON) |
| `garbg query current` | Current wallpaper info (JSON) |
| `garbg reload` | Reload configuration |

## Troubleshooting

### garbg not connecting to gar

Check that gar is running and its IPC socket exists:
```bash
ls -la $XDG_RUNTIME_DIR/gar.sock
```

garbg will automatically reconnect with exponential backoff if gar restarts.

### Wallpaper not appearing

1. Verify the daemon is running:
   ```bash
   pgrep -f "garbg daemon"
   ```

2. Check daemon logs:
   ```bash
   garbg daemon  # Run in foreground to see logs
   ```

3. Verify RandR is working:
   ```bash
   garbg query monitors
   ```

### Animation stuttering

- Reduce max FPS: `garbg set file.gif --animate --max-fps 30`
- Ensure no heavy background processes

## Example Complete Config

`~/.config/gar/init.lua`:
```lua
-- Start garbg wallpaper daemon
gar.exec_once("garbg daemon")

-- Set initial wallpaper with slideshow
gar.exec_once("sleep 0.5 && garbg set ~/Pictures/wallpapers --random --interval 10m")

-- Wallpaper keybindings
gar.bind("mod+bracketright", function() gar.exec("garbg next") end)
gar.bind("mod+bracketleft", function() gar.exec("garbg prev") end)
gar.bind("mod+shift+w", function() gar.exec("garbg random") end)
gar.bind("mod+shift+p", function() gar.exec("garbg toggle") end)
```

`~/.config/garbg/config.toml`:
```toml
[general]
mode = "fill"

[animation]
enabled = true
max_fps = 60

[cache]
max_size_mb = 512

[default]
source = "~/Pictures/wallpapers"

[default.slideshow]
enabled = true
interval = "10m"
shuffle = true

[[workspaces]]
id = 1
source = "~/Pictures/main.jpg"

[[workspaces]]
id = 9
source = "~/Pictures/gaming.jpg"
```
