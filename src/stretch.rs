//! Playing faster or slower without the voice going higher or lower (log
//! entry 3, idea 2): WSOLA, waveform-similarity overlap-add. The sound is cut
//! into overlapping pieces of about 30 milliseconds; each piece is taken from
//! further on (faster) or further back (slower) than the last, at the place
//! within a few milliseconds that best continues it, and the pieces are faded
//! into one another. Speech stays clear up to about twice as fast.
//!
//! Samples are 16-bit, interleaved, `channels` to a frame.

/// Frames in one piece, 32 ms at 48 kHz, and the step between pieces. At 21
/// ms, with a search of 5 ms, 2x and the slowest speeds were rough
/// (milestone 4 test, 2.3): a low voice's pitch period is 8 to 12 ms, so the
/// search could not reach the place that continued it in step.
const PIECE: usize = 1536;
const HOP: usize = PIECE / 2;
/// How far either side of its place a piece may be taken from, to continue
/// the last one best: 12 ms, more than a low voice's pitch period.
const SEARCH: usize = 576;

pub struct Stretch {
    channels: usize,
    speed: f64,
    /// Input not yet used up, interleaved.
    input: Vec<f32>,
    /// Where the next piece belongs in `input`, in frames.
    place: f64,
    /// Where the last piece started in `input`, if there was one.
    last: Option<usize>,
    /// The second half of the last piece, faded out, to fade the next in over.
    tail: Vec<f32>,
    window: Vec<f32>,
}

impl Stretch {
    pub fn new(channels: usize, speed: f64) -> Stretch {
        let window = (0..PIECE)
            .map(|i| {
                let x = std::f32::consts::PI * 2.0 * i as f32 / PIECE as f32;
                0.5 - 0.5 * x.cos()
            })
            .collect();
        Stretch {
            channels,
            speed,
            input: Vec::new(),
            place: 0.0,
            last: None,
            tail: vec![0.0; HOP * channels],
            window,
        }
    }

    pub fn speed(&self) -> f64 {
        self.speed
    }

    pub fn set_speed(&mut self, speed: f64) {
        self.speed = speed;
    }

    /// Starts again, as after a skip: nothing held over.
    pub fn clear(&mut self) {
        self.input.clear();
        self.place = 0.0;
        self.last = None;
        self.tail.iter_mut().for_each(|s| *s = 0.0);
    }

    pub fn push(&mut self, samples: &[i16]) {
        self.input.extend(samples.iter().map(|&s| s as f32));
    }

    /// Frames of `input` from `at`, for `len` frames, mixed to one channel.
    fn mono(&self, at: usize, len: usize) -> impl Iterator<Item = f32> + '_ {
        let ch = self.channels;
        self.input[at * ch..(at + len) * ch]
            .chunks_exact(ch)
            .map(move |f| f.iter().sum::<f32>())
    }

    /// The start, near `centre`, of the piece that best continues the one
    /// that started at `last`.
    fn best(&self, centre: usize, last: usize) -> usize {
        // At normal speed the natural continuation is the place itself, which
        // gives back the sound unchanged.
        if (self.speed - 1.0).abs() < 1e-9 {
            return centre;
        }
        let natural: Vec<f32> = self.mono(last + HOP, HOP).collect();
        let from = centre.saturating_sub(SEARCH);
        let to = centre + SEARCH;
        // Mixed to one channel once, then every place compared within it.
        let near: Vec<f32> = self.mono(from, to - from + HOP).collect();
        let mut best = (centre, f32::MIN);
        // Every other frame: half the work, and no difference to hear.
        for k in (0..=to - from).step_by(2) {
            let piece = &near[k..k + HOP];
            let mut dot = 0.0f32;
            let mut energy = 1e-6f32;
            for (a, b) in piece.iter().zip(&natural) {
                dot += a * b;
                energy += a * a;
            }
            let score = dot / energy.sqrt();
            if score > best.1 {
                best = (from + k, score);
            }
        }
        best.0
    }

    /// As much output as the input so far allows, added to `out`. Each frame
    /// of output stands for `speed` frames of input.
    pub fn pull(&mut self, out: &mut Vec<i16>) {
        let ch = self.channels;
        loop {
            let frames = self.input.len() / ch;
            let centre = self.place.round() as usize;
            let need = match self.last {
                Some(last) => (centre + SEARCH + PIECE).max(last + HOP + HOP),
                None => centre + PIECE,
            };
            if frames < need {
                return;
            }
            let start = match self.last {
                Some(last) => self.best(centre, last),
                None => centre,
            };
            // The first half fades in over the last piece's fading tail.
            for i in 0..HOP {
                for c in 0..ch {
                    let s =
                        self.input[(start + i) * ch + c] * self.window[i] + self.tail[i * ch + c];
                    out.push(s.round().clamp(-32768.0, 32767.0) as i16);
                }
            }
            for i in 0..HOP {
                for c in 0..ch {
                    self.tail[i * ch + c] =
                        self.input[(start + HOP + i) * ch + c] * self.window[HOP + i];
                }
            }
            self.last = Some(start);
            self.place += HOP as f64 * self.speed;
            // Drop what no piece can reach any more.
            let keep = start.min(self.place as usize).saturating_sub(SEARCH);
            if keep > 0 {
                self.input.drain(..keep * ch);
                self.place -= keep as f64;
                self.last = Some(start - keep);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(frames: usize) -> Vec<i16> {
        (0..frames)
            .flat_map(|i| {
                let s = ((i as f32 * 0.05).sin() * 8000.0) as i16;
                [s, s]
            })
            .collect()
    }

    fn run(speed: f64, input: &[i16]) -> Vec<i16> {
        let mut st = Stretch::new(2, speed);
        let mut out = Vec::new();
        for chunk in input.chunks(2 * 4800) {
            st.push(chunk);
            st.pull(&mut out);
        }
        out
    }

    #[test]
    fn normal_speed_gives_the_sound_back_unchanged() {
        let input = tone(48_000);
        let out = run(1.0, &input);
        assert!(out.len() > 40_000 * 2);
        // After the first half piece, which fades in, it is the same sound.
        for i in HOP * 2..out.len() {
            assert!((out[i] as i32 - input[i] as i32).abs() <= 1, "at {i}");
        }
    }

    #[test]
    fn faster_is_shorter_and_slower_is_longer() {
        let input = tone(96_000);
        let normal = run(1.0, &input).len() as f64;
        let fast = run(2.0, &input).len() as f64;
        let slow = run(0.5, &input).len() as f64;
        assert!((fast / normal - 0.5).abs() < 0.05, "{}", fast / normal);
        // Slower needs more input held back, so a little less comes out by the end.
        assert!((slow / normal - 2.0).abs() < 0.1, "{}", slow / normal);
    }

    #[test]
    fn faster_keeps_the_pitch() {
        // Zero crossings per output frame stay as they were: the same note.
        let input = tone(96_000);
        let crossings = |s: &[i16]| {
            let left: Vec<i16> = s.iter().step_by(2).copied().collect();
            left.windows(2).filter(|w| (w[0] < 0) != (w[1] < 0)).count() as f64 / left.len() as f64
        };
        let normal = crossings(&run(1.0, &input));
        let fast = crossings(&run(1.5, &input));
        assert!((fast / normal - 1.0).abs() < 0.05, "{}", fast / normal);
    }
}
