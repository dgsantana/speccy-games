//! A square-wave beeper, the way the Spectrum made every sound it ever made.
//!
//! The engine emits [`Sound`] values holding the original's pitch and duration
//! bytes. A pitch byte is a delay-loop counter, so frequency is inversely
//! proportional to it; [`PITCH_SCALE`] is the constant that turns one into the
//! other, chosen so the title tune lands in the register it does on real hardware.
//!
//! If no audio device is available the beeper silently does nothing, because a
//! missing sound card should not stop anyone playing.

use std::fmt;
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use mm_core::Sound;

/// Frequency of a note is `PITCH_SCALE / pitch`.
pub const PITCH_SCALE: f32 = 56_320.0;

/// Seconds per unit of duration for a sound effect.
const EFFECT_UNIT: f32 = 0.000_6;
/// Seconds per unit of duration for a title-tune note.
const TUNE_UNIT: f32 = 0.004;
/// How long the beeper spends on each half of a chord before swapping.
const CHORD_SWAP: f32 = 0.001_5;
/// Peak amplitude. The Spectrum's speaker was not subtle, but our ears are.
const AMPLITUDE: f32 = 0.12;

/// The state the audio callback reads.
#[derive(Debug, Default)]
struct Voice {
    sample_rate: f32,
    /// Square wave phase, 0.0 to 1.0.
    phase: f32,
    /// The frequency currently sounding, or zero for silence.
    frequency: f32,
    /// The other half of a chord, if one is playing.
    alternate: f32,
    /// Seconds left before the note ends.
    remaining: f32,
    /// Seconds left before a chord swaps halves.
    swap_in: f32,
}

impl Voice {
    fn start(&mut self, frequency: f32, alternate: f32, seconds: f32) {
        self.frequency = frequency;
        self.alternate = alternate;
        self.remaining = seconds;
        self.swap_in = CHORD_SWAP;
    }

    fn silence(&mut self) {
        self.frequency = 0.0;
        self.alternate = 0.0;
        self.remaining = 0.0;
    }

    /// Produce the next sample.
    fn next_sample(&mut self) -> f32 {
        if self.remaining <= 0.0 || self.frequency <= 0.0 {
            return 0.0;
        }
        let step = 1.0 / self.sample_rate;
        self.remaining -= step;

        if self.alternate > 0.0 {
            self.swap_in -= step;
            if self.swap_in <= 0.0 {
                std::mem::swap(&mut self.frequency, &mut self.alternate);
                self.swap_in = CHORD_SWAP;
            }
        }

        self.phase += self.frequency * step;
        if self.phase >= 1.0 {
            self.phase -= self.phase.floor();
        }
        if self.phase < 0.5 { AMPLITUDE } else { -AMPLITUDE }
    }
}

/// An open audio output. Dropping it stops the sound.
pub struct Beeper {
    voice: Arc<Mutex<Voice>>,
    /// Held so the stream stays open; cpal stops it when this is dropped.
    stream: Option<cpal::Stream>,
}

impl fmt::Debug for Beeper {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Beeper")
            .field("active", &self.stream.is_some())
            .field("voice", &self.voice.lock().ok().map(|v| v.frequency))
            .finish()
    }
}

impl Default for Beeper {
    fn default() -> Self {
        Self::new()
    }
}

impl Beeper {
    /// Open the default output device, falling back to silence if there is none.
    pub fn new() -> Self {
        let voice = Arc::new(Mutex::new(Voice::default()));
        let stream = build_stream(&voice);
        if stream.is_none() {
            eprintln!("no audio output available; running silently");
        }
        Self { voice, stream }
    }

    /// Play a sound, replacing whatever the single beeper voice was doing.
    pub fn play(&self, sound: Sound) {
        let Ok(mut voice) = self.voice.lock() else {
            return;
        };
        match sound {
            Sound::Note { pitch, duration } => {
                voice.start(frequency_of(pitch), 0.0, f32::from(duration) * EFFECT_UNIT);
            }
            Sound::Chord {
                low,
                high,
                duration,
            } => {
                voice.start(
                    frequency_of(low),
                    frequency_of(high),
                    f32::from(duration) * TUNE_UNIT,
                );
            }
            Sound::Silence => voice.silence(),
        }
    }
}

/// Frequency in hertz for one of the original's pitch bytes.
pub fn frequency_of(pitch: u8) -> f32 {
    if pitch == 0 {
        0.0
    } else {
        PITCH_SCALE / f32::from(pitch)
    }
}

fn build_stream(voice: &Arc<Mutex<Voice>>) -> Option<cpal::Stream> {
    let device = cpal::default_host().default_output_device()?;
    let config = device.default_output_config().ok()?;
    let sample_rate = config.sample_rate().0 as f32;
    let channels = config.channels() as usize;

    voice.lock().ok()?.sample_rate = sample_rate;

    let voice = Arc::clone(voice);
    let on_error = |err| eprintln!("audio stream error: {err}");

    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => device.build_output_stream(
            &config.into(),
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                fill(&voice, data, channels);
            },
            on_error,
            None,
        ),
        cpal::SampleFormat::I16 => device.build_output_stream(
            &config.into(),
            move |data: &mut [i16], _: &cpal::OutputCallbackInfo| {
                let mut scratch = vec![0.0f32; data.len()];
                fill(&voice, &mut scratch, channels);
                for (out, sample) in data.iter_mut().zip(scratch) {
                    *out = (sample * f32::from(i16::MAX)) as i16;
                }
            },
            on_error,
            None,
        ),
        _ => return None,
    }
    .ok()?;

    stream.play().ok()?;
    Some(stream)
}

/// Write one buffer's worth of samples, duplicated across every channel.
fn fill(voice: &Arc<Mutex<Voice>>, data: &mut [f32], channels: usize) {
    let Ok(mut voice) = voice.lock() else {
        data.fill(0.0);
        return;
    };
    for frame in data.chunks_mut(channels.max(1)) {
        let sample = voice.next_sample();
        frame.fill(sample);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_larger_pitch_byte_is_a_lower_note() {
        assert!(frequency_of(43) > frequency_of(203));
        assert!(frequency_of(0) < f32::EPSILON);
    }

    #[test]
    fn the_title_tune_lands_in_a_musical_register() {
        // The Blue Danube's opening note and the lowest note it reaches.
        assert!((frequency_of(128) - 440.0).abs() < 1.0);
        assert!(frequency_of(203) > 200.0 && frequency_of(43) < 1500.0);
    }

    #[test]
    fn a_note_stops_when_its_duration_runs_out() {
        let mut voice = Voice {
            sample_rate: 1000.0,
            ..Voice::default()
        };
        voice.start(440.0, 0.0, 0.005);
        let mut sounded = 0;
        for _ in 0..20 {
            if voice.next_sample().abs() > f32::EPSILON {
                sounded += 1;
            }
        }
        assert_eq!(sounded, 5);
    }
}
