# Phase 1: Core Foundation

## Goal
Establish the basic infrastructure for garbg: project structure, X11 connection, static image rendering, and a minimal CLI.

## Tasks

### 1.1 Project Scaffolding
- [x] Create workspace Cargo.toml with garbg and garbgctl members
- [x] Define workspace dependencies (x11rb, image, tokio, clap, etc.)
- [x] Create module structure under garbg/src/

### 1.2 X11 Connection
- [x] Connect to X server using x11rb
- [x] Intern required atoms (_XROOTPMAP_ID, ESETROOT_PMAP_ID)
- [x] Create graphics context for drawing
- [ ] Handle connection errors gracefully

### 1.3 Root Window Pixmap Rendering
- [x] Create pixmap from image data
- [x] Convert RGBA to BGRA (X11 native format)
- [x] Set pixmap as root window background
- [x] Set standard atoms for compatibility with other tools
- [x] Clear root window to display new background
- [ ] Free old pixmap on wallpaper change

### 1.4 Static Image Loading
- [ ] Load PNG images
- [ ] Load JPEG images
- [ ] Load WebP images
- [ ] Implement scale modes: fill, fit, stretch, center, tile

### 1.5 Basic CLI
- [x] Parse commands with clap
- [x] `garbg set <source>` command
- [x] `--mode` flag for scale mode
- [x] `--monitor` flag for target monitor
- [ ] `--verbose` logging support

## Deliverables
- `garbg set ~/path/to/image.png` sets wallpaper
- Static images display correctly at screen resolution
- Scale modes work as expected

## Files Modified/Created
- `/garbg/Cargo.toml` - workspace config
- `/garbg/garbg/Cargo.toml` - main crate config
- `/garbg/garbgctl/Cargo.toml` - CLI tool config
- `/garbg/garbg/src/lib.rs` - library root
- `/garbg/garbg/src/main.rs` - CLI entry point
- `/garbg/garbg/src/x11/mod.rs` - X11 module
- `/garbg/garbg/src/x11/connection.rs` - X11 connection
- `/garbg/garbg/src/x11/renderer.rs` - Pixmap rendering
- `/garbg/garbg/src/x11/monitors.rs` - RandR monitor detection
- `/garbg/garbg/src/media/mod.rs` - Media module
- `/garbg/garbg/src/media/loader.rs` - Image loading
- `/garbg/garbg/src/media/scaler.rs` - Image scaling
- `/garbg/garbg/src/config/mod.rs` - Config types
