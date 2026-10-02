//! In-memory WAV playback with volume scaling, shared by the session helper's
//! alert chime and the dashboard's "Preview chime" button.
//!
//! # The bug this replaces
//!
//! Both call sites used to call `PlaySoundW(SND_ASYNC | SND_MEMORY)` on a
//! freshly scaled `Vec` and return, dropping the buffer while winmm was still
//! reading it — a use-after-free that played garbage or crashed depending on
//! allocator reuse. They also assumed a 44-byte WAV header and scaled every
//! byte after it, which corrupts files with extra chunks (`LIST`, `fact`).
//!
//! Now: the scaled copy is owned by a short-lived worker thread that plays it
//! with `SND_SYNC`, so the memory provably outlives playback, and scaling walks
//! the RIFF chunk list to touch only 16-bit PCM sample data.

#[link(name = "winmm")]
extern "system" {
    fn PlaySoundW(pszsound: *const u16, hmod: isize, fdwsound: u32) -> i32;
}

const SND_SYNC: u32 = 0x0000;
const SND_NODEFAULT: u32 = 0x0002;
const SND_MEMORY: u32 = 0x0004;

/// Play `wav` at `volume_pct` (0–100; 0 is silent) without blocking the
/// caller. A newer call stops an older one, which is winmm's normal behaviour;
/// the older worker then returns and frees its buffer.
pub fn play_wav_scaled(wav: &'static [u8], volume_pct: u32) {
    let volume_pct = volume_pct.min(100);
    if volume_pct == 0 {
        return;
    }
    let buffer = scale_pcm16_wav(wav, volume_pct);
    let spawned = std::thread::Builder::new()
        .name("wav-playback".into())
        .spawn(move || {
            // SAFETY: `buffer` is owned by this thread and is only dropped
            // after the synchronous PlaySoundW call has returned, i.e. after
            // winmm has finished (or been told to stop) reading it.
            unsafe {
                let _ = PlaySoundW(
                    buffer.as_ptr() as *const u16,
                    0,
                    SND_SYNC | SND_NODEFAULT | SND_MEMORY,
                );
            }
            drop(buffer);
        });
    if let Err(e) = spawned {
        tracing::warn!(error = %e, "could not start audio playback thread");
    }
}

/// Copy `wav`, scaling 16-bit PCM samples to `volume_pct` percent. Anything
/// that is not a well-formed 16-bit PCM RIFF/WAVE file is returned unscaled
/// rather than guessed at.
pub fn scale_pcm16_wav(wav: &[u8], volume_pct: u32) -> Vec<u8> {
    let mut out = wav.to_vec();
    if volume_pct >= 100 {
        return out;
    }
    let Some((data_start, data_len)) = pcm16_data_range(wav) else {
        return out;
    };
    let end = (data_start + data_len).min(out.len());
    for chunk in out[data_start..end].chunks_exact_mut(2) {
        let sample = i16::from_le_bytes([chunk[0], chunk[1]]) as i32;
        let scaled = (sample * volume_pct as i32 / 100).clamp(i16::MIN as i32, i16::MAX as i32);
        chunk.copy_from_slice(&(scaled as i16).to_le_bytes());
    }
    out
}

/// Offset and length of the `data` chunk, if the file is 16-bit PCM WAVE.
fn pcm16_data_range(wav: &[u8]) -> Option<(usize, usize)> {
    if wav.len() < 12 || &wav[0..4] != b"RIFF" || &wav[8..12] != b"WAVE" {
        return None;
    }
    let mut pos = 12;
    let mut is_pcm16 = false;
    while pos + 8 <= wav.len() {
        let id = &wav[pos..pos + 4];
        let size = u32::from_le_bytes(wav[pos + 4..pos + 8].try_into().ok()?) as usize;
        let body = pos + 8;
        match id {
            b"fmt " if size >= 16 && body + 16 <= wav.len() => {
                let format = u16::from_le_bytes([wav[body], wav[body + 1]]);
                let bits = u16::from_le_bytes([wav[body + 14], wav[body + 15]]);
                is_pcm16 = format == 1 && bits == 16;
            }
            b"data" => return is_pcm16.then_some((body, size)),
            _ => {}
        }
        // Chunks are word-aligned: odd sizes carry one pad byte.
        pos = body.checked_add(size)?.checked_add(size & 1)?;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal 16-bit PCM WAV, optionally with a `LIST` chunk before `data`.
    fn wav(samples: &[i16], extra_chunk: bool) -> Vec<u8> {
        let mut body = Vec::new();
        body.extend_from_slice(b"WAVE");
        body.extend_from_slice(b"fmt ");
        body.extend_from_slice(&16u32.to_le_bytes());
        body.extend_from_slice(&1u16.to_le_bytes()); // PCM
        body.extend_from_slice(&1u16.to_le_bytes()); // mono
        body.extend_from_slice(&8000u32.to_le_bytes());
        body.extend_from_slice(&16000u32.to_le_bytes());
        body.extend_from_slice(&2u16.to_le_bytes());
        body.extend_from_slice(&16u16.to_le_bytes()); // bits
        if extra_chunk {
            body.extend_from_slice(b"LIST");
            body.extend_from_slice(&3u32.to_le_bytes());
            body.extend_from_slice(&[0x7f, 0x7f, 0x7f, 0]); // odd size + pad
        }
        body.extend_from_slice(b"data");
        body.extend_from_slice(&((samples.len() * 2) as u32).to_le_bytes());
        for s in samples {
            body.extend_from_slice(&s.to_le_bytes());
        }
        let mut out = b"RIFF".to_vec();
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(&body);
        out
    }

    fn samples_of(wav: &[u8]) -> Vec<i16> {
        let (start, len) = pcm16_data_range(wav).expect("pcm16");
        wav[start..start + len]
            .chunks_exact(2)
            .map(|c| i16::from_le_bytes([c[0], c[1]]))
            .collect()
    }

    #[test]
    fn scales_only_sample_data() {
        let original = wav(&[1000, -1000, i16::MAX], false);
        let half = scale_pcm16_wav(&original, 50);
        assert_eq!(samples_of(&half), [500, -500, i16::MAX / 2]);
        assert_eq!(half[..36], original[..36], "headers untouched");
    }

    #[test]
    fn skips_extra_chunks_instead_of_assuming_a_44_byte_header() {
        let original = wav(&[2000, 4000], true);
        let scaled = scale_pcm16_wav(&original, 25);
        assert_eq!(samples_of(&scaled), [500, 1000]);
        let list = original
            .windows(4)
            .position(|w| w == b"LIST")
            .expect("LIST");
        assert_eq!(
            scaled[list..list + 12],
            original[list..list + 12],
            "LIST chunk untouched"
        );
    }

    #[test]
    fn full_volume_and_non_pcm_are_copied_verbatim() {
        let original = wav(&[1234], false);
        assert_eq!(scale_pcm16_wav(&original, 100), original);
        let garbage = b"not a wav file at all".to_vec();
        assert_eq!(scale_pcm16_wav(&garbage, 10), garbage);
    }

    #[test]
    fn the_shipped_chime_is_pcm16() {
        let chime = include_bytes!("../../session/src/assets/chime.wav");
        assert!(pcm16_data_range(chime).is_some());
    }
}
