//! A square-wave beeper, driven by emulating the original's sound loop.
//!
//! The routine at 37596 is where every note in the theme tune comes from:
//!
//! ```text
//! OUT (254),A   11      the speaker level is written every iteration
//! DEC D          4      D counts down, reloads, and flips the speaker
//! JR NZ         12
//! DEC E          4      E does the same, independently
//! JR NZ         12
//! DJNZ          13      256 iterations per unit of the duration byte
//! ```
//!
//! An iteration is 56 T-states, so at 3.5 MHz the loop runs at
//! [`ITERATIONS_PER_SECOND`] and one duration unit lasts [`BEEP_UNIT`].
//!
//! What matters is that `D` and `E` flip the *same* speaker bit. The output is
//! therefore the exclusive-or of two square waves, not two tones played in
//! turn. For the near-equal pairs the tune uses for its melody — 128 and 129,
//! 102 and 103 — the two flips interleave and the speaker changes state once
//! every `D` iterations rather than every `2 * D`, so the note sounds an octave
//! above a plain square wave at the same counter value. For the wide pairs used
//! for accompaniment it produces the rough two-tone buzz the tune is known for.
//! Approximating any of this with an oscillator gets the pitch wrong, so this
//! synth just runs the counters.
//!
//! The two frequency bytes of zero in the tune data need no special case: `DEC`
//! from zero wraps to 255, so a zero byte is a 256-iteration counter, and when
//! both counters are 256 they flip together and cancel. That is the tune's rest.
//!
//! If no audio device is available the beeper silently does nothing, because a
//! missing sound card should not stop anyone playing.

use std::fmt;
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use mm_core::Sound;

/// Iterations of the sound loop per second: 3.5 MHz divided by 56 T-states.
pub const ITERATIONS_PER_SECOND: f32 = 62_500.0;

/// Seconds in one unit of a duration byte, being 256 iterations of the loop.
pub const BEEP_UNIT: f32 = 256.0 / ITERATIONS_PER_SECOND;

/// A single counter flips the speaker every `pitch` iterations, so a full cycle
/// takes `2 * pitch` of them and the frequency is `PITCH_SCALE / pitch` hertz.
pub const PITCH_SCALE: f32 = ITERATIONS_PER_SECOND / 2.0;

/// Peak amplitude. The Spectrum's speaker was not subtle, but our ears are.
const AMPLITUDE: f32 = 0.12;

/// A counter byte, where zero means a full 256 because `DEC` wraps.
fn counter(byte: u8) -> u16 {
    if byte == 0 { 256 } else { u16::from(byte) }
}

/// The speaker, as a pair of down-counters.
#[derive(Debug, Default)]
struct Voice {
    sample_rate: f32,
    /// Loop iterations per output sample, a little over one at 48 kHz.
    iters_per_sample: f32,
    /// Fractional iterations carried into the next sample.
    carry: f32,
    d_reload: u16,
    d: u16,
    /// The second counter, absent for a single-pitch sound effect.
    e_reload: Option<u16>,
    e: u16,
    /// Current speaker level.
    high: bool,
    /// Seconds left before the sound ends.
    remaining: f32,
}

impl Voice {
    fn set_sample_rate(&mut self, rate: f32) {
        self.sample_rate = rate;
        self.iters_per_sample = ITERATIONS_PER_SECOND / rate;
    }

    /// Start a single-pitch sound, as the effect routines produce.
    fn start_note(&mut self, pitch: u8, seconds: f32) {
        self.d_reload = counter(pitch);
        self.d = self.d_reload;
        self.e_reload = None;
        self.remaining = seconds;
    }

    /// Start a two-counter note, as the theme tune produces.
    fn start_chord(&mut self, first: u8, second: u8, seconds: f32) {
        self.d_reload = counter(first);
        self.d = self.d_reload;
        let reload = counter(second);
        self.e_reload = Some(reload);
        self.e = reload;
        self.remaining = seconds;
    }

    fn silence(&mut self) {
        self.remaining = 0.0;
    }

    /// One iteration of the loop: decrement each counter, flipping on reload.
    fn tick(&mut self) {
        self.d -= 1;
        if self.d == 0 {
            self.d = self.d_reload;
            self.high = !self.high;
        }
        if let Some(reload) = self.e_reload {
            self.e -= 1;
            if self.e == 0 {
                self.e = reload;
                self.high = !self.high;
            }
        }
    }

    fn level(&self) -> f32 {
        if self.high { AMPLITUDE } else { -AMPLITUDE }
    }

    /// Produce the next sample, averaging over the iterations it spans.
    ///
    /// The loop runs faster than any sane output rate, so a sample covers one
    /// or two iterations; averaging them is a cheap anti-alias filter.
    fn next_sample(&mut self) -> f32 {
        if self.remaining <= 0.0 || self.d_reload == 0 {
            return 0.0;
        }
        self.remaining -= 1.0 / self.sample_rate;

        self.carry += self.iters_per_sample;
        let steps = self.carry as u32;
        self.carry -= steps as f32;

        if steps == 0 {
            return self.level();
        }
        let mut sum = 0.0;
        for _ in 0..steps {
            sum += self.level();
            self.tick();
        }
        sum / steps as f32
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
            .field("playing", &self.voice.lock().ok().map(|v| v.remaining > 0.0))
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
                voice.start_note(pitch, f32::from(duration) * BEEP_UNIT);
            }
            Sound::Chord {
                first,
                second,
                duration,
            } => {
                voice.start_chord(first, second, f32::from(duration) * BEEP_UNIT);
            }
            Sound::Silence => voice.silence(),
        }
    }
}

/// Frequency in hertz of a single-counter sound effect.
pub fn frequency_of(pitch: u8) -> f32 {
    PITCH_SCALE / f32::from(counter(pitch))
}

fn build_stream(voice: &Arc<Mutex<Voice>>) -> Option<cpal::Stream> {
    let device = cpal::default_host().default_output_device()?;
    let config = device.default_output_config().ok()?;
    let sample_rate = config.sample_rate() as f32;
    let channels = config.channels() as usize;

    voice.lock().ok()?.set_sample_rate(sample_rate);

    let voice = Arc::clone(voice);
    let on_error = |err| eprintln!("audio stream error: {err}");

    let format = config.sample_format();
    let config: cpal::StreamConfig = config.into();
    let stream = match format {
        cpal::SampleFormat::F32 => device.build_output_stream(
            config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                fill(&voice, data, channels);
            },
            on_error,
            None,
        ),
        cpal::SampleFormat::I16 => device.build_output_stream(
            config,
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

    /// Run the voice for a while and report the fundamental, by counting how
    /// often the speaker changes level.
    fn measured_hz(voice: &mut Voice, seconds: f32) -> f32 {
        let samples = (voice.sample_rate * seconds) as usize;
        let mut flips = 0;
        let mut last = voice.next_sample() > 0.0;
        for _ in 1..samples {
            let now = voice.next_sample() > 0.0;
            if now != last {
                flips += 1;
            }
            last = now;
        }
        flips as f32 / 2.0 / seconds
    }

    fn voice_at(rate: f32) -> Voice {
        let mut voice = Voice::default();
        voice.set_sample_rate(rate);
        voice
    }

    #[test]
    fn a_larger_pitch_byte_is_a_lower_note() {
        assert!(frequency_of(43) > frequency_of(203));
    }

    #[test]
    fn note_lengths_match_the_original_loop() {
        // The two durations the theme tune uses, in seconds.
        assert!((80.0 * BEEP_UNIT - 0.328).abs() < 0.005);
        assert!((50.0 * BEEP_UNIT - 0.205).abs() < 0.005);
    }

    #[test]
    fn a_melody_note_sounds_an_octave_above_a_plain_square_wave() {
        // 128 and 129 open the tune. The two counters interleave, so the
        // speaker changes state every 128 iterations rather than every 256.
        let mut voice = voice_at(192_000.0);
        voice.start_chord(128, 129, 1.0);
        let hz = measured_hz(&mut voice, 0.5);
        assert!(
            (hz - 488.0).abs() < 15.0,
            "melody note measured {hz} Hz, expected about 488"
        );
        assert!((hz / frequency_of(128) - 2.0).abs() < 0.1);
    }

    #[test]
    fn the_opening_phrase_is_a_major_triad() {
        // The Blue Danube opens on an arpeggio: a major third then a minor third.
        let mut hz = Vec::new();
        for pair in [(128u8, 129u8), (102, 103), (86, 87)] {
            let mut voice = voice_at(192_000.0);
            voice.start_chord(pair.0, pair.1, 1.0);
            hz.push(measured_hz(&mut voice, 0.5));
        }
        let major_third = hz[1] / hz[0];
        let minor_third = hz[2] / hz[1];
        assert!((major_third - 1.26).abs() < 0.03, "got {major_third}");
        assert!((minor_third - 1.19).abs() < 0.03, "got {minor_third}");
    }

    #[test]
    fn two_zero_bytes_are_a_rest() {
        // Both counters reload at 256 and flip together, cancelling out.
        let mut voice = voice_at(48_000.0);
        voice.start_chord(0, 0, 1.0);
        let hz = measured_hz(&mut voice, 0.25);
        assert!(hz < 1.0, "the rest made a sound at {hz} Hz");
    }

    #[test]
    fn a_sound_stops_when_its_duration_runs_out() {
        let mut voice = voice_at(1000.0);
        voice.start_note(4, 0.005);
        let sounded = (0..20).filter(|_| voice.next_sample().abs() > 0.0).count();
        assert_eq!(sounded, 5);
    }
}
