/// Decoded, resampled audio held in memory. Deinterleaved f32, 1 or 2 channels,
/// all channels the same length. Shared read-only via `Arc` between UI and audio threads.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AudioBuffer {
    pub channels: Vec<Vec<f32>>,
}

impl AudioBuffer {
    pub fn new(channels: Vec<Vec<f32>>) -> Self {
        debug_assert!(channels.windows(2).all(|w| w[0].len() == w[1].len()));
        Self { channels }
    }

    pub fn channel_count(&self) -> usize {
        self.channels.len()
    }

    pub fn frames(&self) -> usize {
        self.channels.first().map_or(0, Vec::len)
    }
}
