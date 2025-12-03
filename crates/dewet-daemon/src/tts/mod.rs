use std::sync::Arc;

use anyhow::Result;

use crate::config::TtsConfig;

pub type SharedSynth = Arc<dyn SpeechSynthesizer>;

pub trait SpeechSynthesizer: Send + Sync {
    fn synthesize(&self, text: &str) -> Result<Vec<u8>>;
}

pub fn create_synthesizer(config: &TtsConfig) -> SharedSynth {
    match config.provider.as_str() {
        _ => Arc::new(NullSynth),
    }
}

struct NullSynth;

impl SpeechSynthesizer for NullSynth {
    fn synthesize(&self, text: &str) -> Result<Vec<u8>> {
        let seconds = (text.len() as f32 / 14.0).clamp(0.5, 3.0);
        Ok(render_silence(seconds))
    }
}

fn render_silence(duration_secs: f32) -> Vec<u8> {
    let sample_rate = 16_000u32;
    let channels = 1u16;
    let bits_per_sample = 16u16;
    let total_samples = (sample_rate as f32 * duration_secs) as u32;
    let byte_rate = sample_rate * channels as u32 * bits_per_sample as u32 / 8;
    let block_align = channels * bits_per_sample / 8;
    let data_len = total_samples * block_align as u32;
    let mut buffer = Vec::with_capacity(44 + data_len as usize);

    buffer.extend_from_slice(b"RIFF");
    buffer.extend_from_slice(&(36 + data_len).to_le_bytes());
    buffer.extend_from_slice(b"WAVEfmt ");
    buffer.extend_from_slice(&16u32.to_le_bytes()); // PCM chunk size
    buffer.extend_from_slice(&1u16.to_le_bytes()); // PCM format
    buffer.extend_from_slice(&channels.to_le_bytes());
    buffer.extend_from_slice(&sample_rate.to_le_bytes());
    buffer.extend_from_slice(&byte_rate.to_le_bytes());
    buffer.extend_from_slice(&block_align.to_le_bytes());
    buffer.extend_from_slice(&bits_per_sample.to_le_bytes());
    buffer.extend_from_slice(b"data");
    buffer.extend_from_slice(&data_len.to_le_bytes());

    buffer.resize(44 + data_len as usize, 0u8);
    buffer
}
