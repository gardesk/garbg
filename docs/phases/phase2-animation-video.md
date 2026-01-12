# Phase 2: Animation & Video Support

## Goal
Add support for animated GIFs and video wallpapers with efficient frame rendering.

## Tasks

### 2.1 Animated GIF Support
- [ ] Parse GIF files with frame-by-frame access
- [ ] Extract frame delays from GIF metadata
- [ ] Handle GIF disposal methods (replace, combine, etc.)
- [ ] Implement frame timing with proper delays

### 2.2 Double Buffering
- [ ] Create front and back buffer pixmaps
- [ ] Render to back buffer while displaying front
- [ ] Swap buffers atomically
- [ ] Minimize visual tearing

### 2.3 Animation Event Loop
- [ ] Timer-based frame advancement
- [ ] Adaptive frame skipping under load
- [ ] Max 60fps cap to prevent excessive CPU usage
- [ ] Pause/resume animation state

### 2.4 Video Decoding (Optional Feature)
- [ ] Integrate ffmpeg-next for video decoding
- [ ] Decode video frames to RGBA
- [ ] Handle common codecs (H.264, VP9, AV1)
- [ ] Support MP4 and WebM containers

### 2.5 Frame Pre-rendering Buffer
- [ ] Ring buffer of ~30 pre-decoded frames
- [ ] Background thread for frame decoding
- [ ] Producer-consumer pattern with channels
- [ ] Memory-efficient frame recycling

### 2.6 Animated WebP/APNG
- [ ] Detect animated WebP files
- [ ] Parse APNG frame structure
- [ ] Unified animation interface for all formats

## Deliverables
- `garbg set ~/animation.gif` plays animated GIF
- `garbg set ~/video.mp4` plays video as wallpaper (with --features video)
- Smooth playback at correct frame rates
- Minimal CPU usage during playback

## Technical Notes

### GIF Frame Timing
```rust
// GIF delays are in centiseconds (1/100th second)
let delay_ms = frame.delay * 10;
// Some GIFs have 0 delay, default to ~100ms
let delay_ms = if delay_ms == 0 { 100 } else { delay_ms };
```

### Double Buffer Swap
```rust
// Render to back buffer
put_image(back_buffer, frame_data);
// Swap pointers
std::mem::swap(&mut front_buffer, &mut back_buffer);
// Set root to new front buffer
set_root_pixmap(front_buffer);
```

### Video Pipeline
```
MP4/WebM File
     |
     v
[FFmpeg Demuxer]
     |
     v
[Video Decoder] --> Frame Queue (30 frames)
     |                    |
     v                    v
[Scaler/Converter]  [Render Thread]
     |                    |
     v                    v
  BGRA Data         X11 PutImage
```

## Files Modified/Created
- `/garbg/garbg/src/media/gif.rs` - GIF decoder
- `/garbg/garbg/src/media/video.rs` - Video decoder (optional)
- `/garbg/garbg/src/media/animation.rs` - Animation frame management
- `/garbg/garbg/src/x11/animation.rs` - Double buffering
- `/garbg/garbg/src/daemon/animation_loop.rs` - Frame timing loop
