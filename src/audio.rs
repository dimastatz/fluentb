//! Converts captured microphone samples into what Whisper expects: mono, 16 kHz.

use crate::facade::Audio;

/// The sample rate Whisper models are trained on.
pub const WHISPER_SAMPLE_RATE: u32 = 16_000;

/// Averages interleaved frames of `channels` samples into one mono sample each.
/// A trailing partial frame is dropped.
#[must_use]
pub fn to_mono(samples: &[f32], channels: u16) -> Vec<f32> {
    match usize::from(channels) {
        0 => Vec::new(),
        1 => samples.to_vec(),
        n => samples
            .chunks_exact(n)
            .map(|frame| frame.iter().sum::<f32>() / f32::from(channels))
            .collect(),
    }
}

/// Resamples mono audio with linear interpolation, which is plenty for speech.
#[must_use]
pub fn resample(samples: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == to_rate || samples.is_empty() || from_rate == 0 || to_rate == 0 {
        return samples.to_vec();
    }
    let step = f64::from(from_rate) / f64::from(to_rate);
    let last = samples.len() - 1;
    let output_len = output_length(samples.len(), from_rate, to_rate);
    (0..output_len)
        .map(|i| {
            let position = index_to_f64(i) * step;
            let left = f64_to_index(position).min(last);
            let right = (left + 1).min(last);
            let weight = position - index_to_f64(left);
            let interpolated = f64::from(samples[left])
                + (f64::from(samples[right]) - f64::from(samples[left])) * weight;
            narrow(interpolated)
        })
        .collect()
}

/// Recordings shorter than this are treated as accidental taps and not transcribed.
pub const MIN_DURATION_MS: u32 = 300;

/// Whether `audio` is too short to contain speech.
#[must_use]
pub fn is_too_short(audio: &Audio) -> bool {
    let min_samples = u64::from(audio.sample_rate) * u64::from(MIN_DURATION_MS) / 1000;
    (audio.samples.len() as u64) < min_samples
}

/// Turns raw interleaved device samples into Whisper-ready [`Audio`].
#[must_use]
pub fn prepare(interleaved: &[f32], channels: u16, sample_rate: u32) -> Audio {
    Audio {
        samples: resample(
            &to_mono(interleaved, channels),
            sample_rate,
            WHISPER_SAMPLE_RATE,
        ),
        sample_rate: WHISPER_SAMPLE_RATE,
    }
}

/// `ceil(len * to / from)`, computed in integers.
fn output_length(len: usize, from_rate: u32, to_rate: u32) -> usize {
    let scaled = (len as u128) * u128::from(to_rate);
    usize::try_from(scaled.div_ceil(u128::from(from_rate))).unwrap_or(usize::MAX)
}

// Sample counts and positions stay far below 2^52, so these conversions are exact.
#[allow(clippy::cast_precision_loss)]
fn index_to_f64(index: usize) -> f64 {
    index as f64
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn f64_to_index(position: f64) -> usize {
    position as usize
}

#[allow(clippy::cast_possible_truncation)]
fn narrow(sample: f64) -> f32 {
    sample as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mono_passes_through() {
        assert_eq!(to_mono(&[0.1, 0.2, 0.3], 1), [0.1, 0.2, 0.3]);
    }

    #[test]
    fn stereo_frames_are_averaged() {
        assert_eq!(
            to_mono(&[1.0, 0.0, 0.5, 0.5, -1.0, 1.0], 2),
            [0.5, 0.5, 0.0]
        );
    }

    #[test]
    fn partial_frames_and_zero_channels_are_dropped() {
        assert_eq!(to_mono(&[0.3, 0.3, 0.3, 0.9], 3), [0.3]);
        assert_eq!(to_mono(&[0.5, 0.5], 0), Vec::<f32>::new());
    }

    #[test]
    fn same_rate_is_unchanged() {
        assert_eq!(resample(&[0.1, 0.2], 16_000, 16_000), [0.1, 0.2]);
        assert_eq!(resample(&[], 48_000, 16_000), Vec::<f32>::new());
        assert_eq!(resample(&[0.1], 0, 16_000), [0.1]);
    }

    #[test]
    fn downsampling_keeps_every_third_sample_for_48k() {
        let input: Vec<f32> = (0..9u8).map(f32::from).collect();

        assert_eq!(resample(&input, 48_000, 16_000), [0.0, 3.0, 6.0]);
    }

    #[test]
    fn upsampling_interpolates_and_holds_the_last_sample() {
        assert_eq!(resample(&[0.0, 1.0], 8_000, 16_000), [0.0, 0.5, 1.0, 1.0]);
    }

    #[test]
    fn length_scales_with_the_rate_ratio() {
        let one_second = vec![0.0; 44_100];

        assert_eq!(resample(&one_second, 44_100, 16_000).len(), 16_000);
    }

    #[test]
    fn short_recordings_are_detected() {
        let audio = |samples: usize| Audio {
            samples: vec![0.0; samples],
            sample_rate: WHISPER_SAMPLE_RATE,
        };

        assert!(is_too_short(&audio(0)));
        assert!(is_too_short(&audio(4_799)));
        assert!(!is_too_short(&audio(4_800)));
    }

    #[test]
    fn prepare_produces_16k_mono() {
        let stereo_48k = vec![0.25; 48_000 * 2];

        let audio = prepare(&stereo_48k, 2, 48_000);

        assert_eq!(audio.sample_rate, WHISPER_SAMPLE_RATE);
        assert_eq!(audio.samples.len(), 16_000);
        assert!(audio
            .samples
            .iter()
            .all(|s| (*s - 0.25).abs() < f32::EPSILON));
    }
}
