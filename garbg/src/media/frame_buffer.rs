//! Frame pre-rendering ring buffer
//!
//! Provides memory-efficient frame buffering for animations and video playback.
//! Uses a producer-consumer pattern with frame recycling.

use crossbeam_channel::{bounded, Receiver, Sender, TryRecvError};
use image::RgbaImage;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

/// A single frame in the buffer
pub struct BufferedFrame {
    /// The frame image data
    pub image: RgbaImage,
    /// Frame index in the source
    pub index: usize,
    /// Frame delay (time until next frame)
    pub delay: Duration,
}

impl BufferedFrame {
    /// Create a new buffered frame
    pub fn new(image: RgbaImage, index: usize, delay: Duration) -> Self {
        Self { image, index, delay }
    }
}

/// Statistics about the frame buffer
#[derive(Debug, Clone)]
pub struct FrameBufferStats {
    /// Number of frames currently in buffer
    pub buffered: usize,
    /// Total frames produced
    pub produced: usize,
    /// Total frames consumed
    pub consumed: usize,
    /// Number of frames recycled
    pub recycled: usize,
    /// Whether the producer is done
    pub producer_done: bool,
}

/// Ring buffer for pre-rendered frames
///
/// Uses a producer-consumer pattern where:
/// - Producer thread decodes frames ahead of time
/// - Consumer thread displays frames
/// - Empty frame slots are recycled back to producer
pub struct FrameRingBuffer {
    /// Channel for ready frames (producer -> consumer)
    ready_frames: Receiver<BufferedFrame>,
    /// Channel for recycled frames (consumer -> producer)
    recycle_tx: Sender<RgbaImage>,
    /// Whether the producer has finished
    producer_done: Arc<AtomicBool>,
    /// Number of frames currently buffered
    buffered_count: Arc<AtomicUsize>,
    /// Statistics
    consumed_count: usize,
}

impl FrameRingBuffer {
    /// Maximum number of frames to buffer ahead
    pub const DEFAULT_BUFFER_SIZE: usize = 30;

    /// Create a new frame ring buffer with a producer function
    ///
    /// The producer function should:
    /// 1. Try to receive recycled frames from `recycle_rx`
    /// 2. If no recycled frame, allocate a new one
    /// 3. Fill the frame with decoded data
    /// 4. Send via `frame_tx`
    /// 5. Return `None` when done producing frames
    pub fn new<F>(
        buffer_size: usize,
        frame_width: u32,
        frame_height: u32,
        mut producer: F,
    ) -> Self
    where
        F: FnMut(&Receiver<RgbaImage>, &Sender<BufferedFrame>, usize) -> bool + Send + 'static,
    {
        let (frame_tx, frame_rx) = bounded::<BufferedFrame>(buffer_size);
        let (recycle_tx, recycle_rx) = bounded::<RgbaImage>(buffer_size);
        let producer_done = Arc::new(AtomicBool::new(false));
        let producer_done_clone = Arc::clone(&producer_done);
        let buffered_count = Arc::new(AtomicUsize::new(0));
        let buffered_count_clone = Arc::clone(&buffered_count);

        // Pre-allocate recycled frames
        for _ in 0..buffer_size {
            let frame = RgbaImage::new(frame_width, frame_height);
            let _ = recycle_tx.try_send(frame);
        }

        // Spawn producer thread
        thread::spawn(move || {
            let mut frame_index = 0;
            loop {
                let should_continue = producer(&recycle_rx, &frame_tx, frame_index);
                if !should_continue {
                    break;
                }
                buffered_count_clone.fetch_add(1, Ordering::Relaxed);
                frame_index += 1;
            }
            producer_done_clone.store(true, Ordering::Release);
            tracing::debug!("Frame producer finished at index {}", frame_index);
        });

        Self {
            ready_frames: frame_rx,
            recycle_tx,
            producer_done,
            buffered_count,
            consumed_count: 0,
        }
    }

    /// Try to get the next frame (non-blocking)
    ///
    /// Returns `None` if no frame is available yet.
    pub fn try_next(&mut self) -> Option<BufferedFrame> {
        match self.ready_frames.try_recv() {
            Ok(frame) => {
                self.buffered_count.fetch_sub(1, Ordering::Relaxed);
                self.consumed_count += 1;
                Some(frame)
            }
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => None,
        }
    }

    /// Get the next frame (blocking with timeout)
    ///
    /// Returns `None` if timeout is reached or producer is done.
    pub fn next_timeout(&mut self, timeout: Duration) -> Option<BufferedFrame> {
        match self.ready_frames.recv_timeout(timeout) {
            Ok(frame) => {
                self.buffered_count.fetch_sub(1, Ordering::Relaxed);
                self.consumed_count += 1;
                Some(frame)
            }
            Err(_) => None,
        }
    }

    /// Get the next frame (blocking)
    ///
    /// Returns `None` only when producer is done and buffer is empty.
    pub fn next(&mut self) -> Option<BufferedFrame> {
        match self.ready_frames.recv() {
            Ok(frame) => {
                self.buffered_count.fetch_sub(1, Ordering::Relaxed);
                self.consumed_count += 1;
                Some(frame)
            }
            Err(_) => None,
        }
    }

    /// Recycle a frame's image buffer back to the producer
    ///
    /// This allows the producer to reuse the memory allocation.
    pub fn recycle(&self, image: RgbaImage) {
        let _ = self.recycle_tx.try_send(image);
    }

    /// Check if the producer has finished
    pub fn is_producer_done(&self) -> bool {
        self.producer_done.load(Ordering::Acquire)
    }

    /// Check if the buffer is empty and producer is done
    pub fn is_exhausted(&self) -> bool {
        self.is_producer_done() && self.buffered_count.load(Ordering::Relaxed) == 0
    }

    /// Get current buffer statistics
    pub fn stats(&self) -> FrameBufferStats {
        FrameBufferStats {
            buffered: self.buffered_count.load(Ordering::Relaxed),
            produced: self.consumed_count + self.buffered_count.load(Ordering::Relaxed),
            consumed: self.consumed_count,
            recycled: 0, // Would need additional tracking
            producer_done: self.is_producer_done(),
        }
    }

    /// Number of frames currently buffered
    pub fn buffered_count(&self) -> usize {
        self.buffered_count.load(Ordering::Relaxed)
    }
}

/// Simple looping frame buffer for finite animations (GIF, WebP)
///
/// Pre-scales all frames and cycles through them.
pub struct LoopingFrameBuffer {
    /// All frames (pre-scaled)
    frames: Vec<BufferedFrame>,
    /// Current frame index
    current: usize,
}

impl LoopingFrameBuffer {
    /// Create from existing frames
    pub fn new(frames: Vec<BufferedFrame>) -> Self {
        Self { frames, current: 0 }
    }

    /// Get the current frame
    pub fn current(&self) -> Option<&BufferedFrame> {
        self.frames.get(self.current)
    }

    /// Advance to the next frame, looping if necessary
    ///
    /// Returns true if we looped back to the start.
    pub fn advance(&mut self) -> bool {
        self.current += 1;
        if self.current >= self.frames.len() {
            self.current = 0;
            true
        } else {
            false
        }
    }

    /// Go to a specific frame index
    pub fn seek(&mut self, index: usize) {
        self.current = index % self.frames.len();
    }

    /// Get the number of frames
    pub fn len(&self) -> usize {
        self.frames.len()
    }

    /// Check if buffer is empty
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// Get the current frame index
    pub fn current_index(&self) -> usize {
        self.current
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_looping_buffer() {
        let frames = vec![
            BufferedFrame::new(RgbaImage::new(1, 1), 0, Duration::from_millis(100)),
            BufferedFrame::new(RgbaImage::new(1, 1), 1, Duration::from_millis(100)),
            BufferedFrame::new(RgbaImage::new(1, 1), 2, Duration::from_millis(100)),
        ];

        let mut buffer = LoopingFrameBuffer::new(frames);

        assert_eq!(buffer.current_index(), 0);
        assert!(!buffer.advance()); // 0 -> 1
        assert_eq!(buffer.current_index(), 1);
        assert!(!buffer.advance()); // 1 -> 2
        assert_eq!(buffer.current_index(), 2);
        assert!(buffer.advance()); // 2 -> 0 (looped)
        assert_eq!(buffer.current_index(), 0);
    }
}
