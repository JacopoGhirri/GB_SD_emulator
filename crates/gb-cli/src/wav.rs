//! Minimal 16-bit stereo PCM WAV writer (no dependency needed for a 44-byte header).

/// Encode interleaved stereo f32 samples (-1.0..=1.0, clipped) as a RIFF/WAVE file.
pub fn encode(samples: &[f32], sample_rate: u32) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // fmt chunk size
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&2u16.to_le_bytes()); // channels
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&(sample_rate * 4).to_le_bytes()); // byte rate
    out.extend_from_slice(&4u16.to_le_bytes()); // block align
    out.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        let v = (s.clamp(-1.0, 1.0) * 32767.0).round() as i16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

/// (peak, rms) of the samples, for a quick "is there sound" report.
pub fn level(samples: &[f32]) -> (f32, f32) {
    if samples.is_empty() {
        return (0.0, 0.0);
    }
    let peak = samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    let rms = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
    (peak, rms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_and_samples_are_well_formed() {
        let wav = encode(&[0.0, 1.0, -1.0, 2.0], 48_000);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..16], b"WAVEfmt ");
        assert_eq!(u32::from_le_bytes(wav[4..8].try_into().unwrap()), 36 + 8);
        assert_eq!(u32::from_le_bytes(wav[24..28].try_into().unwrap()), 48_000);
        assert_eq!(wav.len(), 44 + 8);
        let s = |i: usize| i16::from_le_bytes(wav[44 + 2 * i..46 + 2 * i].try_into().unwrap());
        assert_eq!((s(0), s(1), s(2), s(3)), (0, 32767, -32767, 32767));
    }

    #[test]
    fn level_reports_peak_and_rms() {
        assert_eq!(level(&[]), (0.0, 0.0));
        let (peak, rms) = level(&[0.5, -0.5]);
        assert_eq!(peak, 0.5);
        assert!((rms - 0.5).abs() < 1e-6);
    }
}
