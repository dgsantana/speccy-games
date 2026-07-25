//! Sound events produced by the engine.
//!
//! The Spectrum made every sound by toggling the speaker in a delay loop, so a
//! note is described by a pitch byte (larger means lower) and a duration byte.
//! The engine only says what to play; turning that into samples is the audio
//! backend's job.

/// One thing for the beeper to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sound {
    /// A single note.
    Note { pitch: u8, duration: u8 },
    /// Two pitches interleaved, which is how the title tune fakes two voices.
    Chord { low: u8, high: u8, duration: u8 },
    /// Stop whatever is playing.
    Silence,
}

/// Sounds queued during one frame, drained by the front end.
#[derive(Debug, Default)]
pub struct SoundQueue {
    events: Vec<Sound>,
}

impl SoundQueue {
    pub fn push(&mut self, sound: Sound) {
        self.events.push(sound);
    }

    pub fn note(&mut self, pitch: u8, duration: u8) {
        self.push(Sound::Note { pitch, duration });
    }

    pub fn drain(&mut self) -> impl Iterator<Item = Sound> + '_ {
        self.events.drain(..)
    }

    pub fn clear(&mut self) {
        self.events.clear();
    }
}
