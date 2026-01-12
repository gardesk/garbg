# Deferred Targets - Completion Checklist

Complete all items before Phase 5. Ordered by effort (low → high).

---

## Low Effort

### Phase 4: PID File Management
- [x] Write PID to `$XDG_RUNTIME_DIR/garbg.pid` on daemon start
- [x] Check for stale PID file and clean up
- [x] Remove PID file on graceful shutdown

### Phase 4: Signal Handling
- [x] Handle SIGHUP → reload configuration
- [x] Handle SIGTERM → graceful shutdown
- [x] Handle SIGINT → graceful shutdown

### Phase 3: Wire Disk Cache to Daemon
- [x] Use DiskCache when fetching remote images in daemon
- [x] Check cache before HTTP fetch
- [x] Store fetched images in cache

### Phase 4: Test gar Integration
- [x] Verify gar IPC connection works
- [x] Test workspace change events trigger wallpaper switch
- [x] Fix any parsing issues with gar events

---

## Medium Effort

### Phase 3: Wire Provider Trait Architecture
- [x] Refactor HTTP fetching to use HttpProvider
- [x] Refactor GitHub fetching to use GitHubProvider
- [x] Refactor local files to use FileProvider
- [x] Use ProviderRegistry for URI dispatch
- [x] Remove ad-hoc implementations from main.rs

### Phase 2: Animated WebP Support
- [x] Detect animated WebP files
- [x] Use image crate's WebP animation support (if available)
- [x] Integrate with existing AnimatedGif infrastructure
- [x] Test with sample animated WebP files

### Phase 2: Frame Pre-rendering Ring Buffer
- [x] Create ring buffer struct (~30 frames)
- [x] Background thread for frame decoding
- [x] Producer-consumer with crossbeam channels
- [x] Memory-efficient frame recycling

### Phase 3: S3 Provider
- [x] Add `s3` feature flag
- [x] Parse `s3://bucket/prefix` URIs
- [x] List objects with prefix (aws-sdk-s3 or rusoto)
- [x] Support S3-compatible endpoints (MinIO)
- [x] Handle authentication (env vars, credentials file)

---

## High Effort

### Phase 2: Animated PNG (APNG) Support
- [x] Add APNG decoder (png crate or apng crate)
- [x] Extract frames and delays
- [x] Integrate with animation infrastructure
- [x] Test with sample APNG files

### Phase 2: Video Decoding (ffmpeg-next)
- [x] Add `video` feature flag
- [x] Integrate ffmpeg-next crate
- [x] Decode video frames to RGBA
- [x] Handle common codecs (H.264, VP9, AV1)
- [x] Support MP4 and WebM containers
- [x] Audio always muted (no PulseAudio complexity)
- [x] Integrate with animation loop for playback

---

## Progress Tracker

| Phase | Section | Status |
|-------|---------|--------|
| 4 | PID File | ✅ |
| 4 | Signal Handling | ✅ |
| 3 | Disk Cache in Daemon | ✅ |
| 4 | Test gar Integration | ✅ |
| 3 | Provider Trait Wiring | ✅ |
| 2 | Animated WebP | ✅ |
| 2 | Ring Buffer | ✅ |
| 3 | S3 Provider | ✅ |
| 2 | APNG Support | ✅ |
| 2 | Video Decoding | ✅ |

---

## Completion Criteria

All boxes checked = Ready for Phase 5 (Lua, multi-monitor, docs)

**STATUS: COMPLETE** - All deferred targets implemented!
