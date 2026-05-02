//! Realistic keystroke timing simulation.
//!
//! Models human typing patterns with variable inter-key delays,
//! occasional pauses, and natural rhythm variations.

use rand::Rng;
use std::time::Duration;

/// A single keystroke event with timing.
#[derive(Debug, Clone)]
pub struct KeyStroke {
    /// The character to type
    pub char: char,
    /// Delay before pressing this key
    pub delay_before: Duration,
    /// How long the key is held down
    pub hold_duration: Duration,
}

/// Generates realistic typing patterns.
pub struct KeyboardSimulator {
    /// Base typing speed in characters per minute
    base_cpm: f64,
    /// Variance factor (0.0 = perfectly consistent, 1.0 = very variable)
    variance: f64,
}

impl KeyboardSimulator {
    /// Create a new keyboard simulator.
    ///
    /// `wpm` is words per minute (average word = 5 chars).
    pub fn new(wpm: u32) -> Self {
        Self {
            base_cpm: wpm as f64 * 5.0,
            variance: 0.3,
        }
    }

    /// Generate keystroke events for a string.
    pub fn generate_keystrokes(&self, text: &str) -> Vec<KeyStroke> {
        let mut rng = rand::thread_rng();
        let base_delay_ms = 60_000.0 / self.base_cpm;

        text.chars()
            .enumerate()
            .map(|(i, ch)| {
                // Base delay with variance
                let mut delay_ms = base_delay_ms * (1.0 + rng.gen_range(-self.variance..self.variance));

                // Longer pauses after spaces (word boundaries)
                if i > 0 && text.chars().nth(i - 1) == Some(' ') {
                    delay_ms *= rng.gen_range(1.2..1.8);
                }

                // Longer pauses after punctuation
                if i > 0 {
                    let prev = text.chars().nth(i - 1).unwrap_or(' ');
                    if ".!?,;:".contains(prev) {
                        delay_ms *= rng.gen_range(1.5..2.5);
                    }
                }

                // Occasional thinking pauses (every ~20 chars)
                if rng.gen_ratio(1, 20) {
                    delay_ms += rng.gen_range(200.0..500.0);
                }

                // Shift key adds slight delay for uppercase
                if ch.is_uppercase() || "!@#$%^&*()_+{}|:\"<>?".contains(ch) {
                    delay_ms += rng.gen_range(20.0..50.0);
                }

                let hold_ms = rng.gen_range(30.0..80.0);

                KeyStroke {
                    char: ch,
                    delay_before: Duration::from_millis(delay_ms as u64),
                    hold_duration: Duration::from_millis(hold_ms as u64),
                }
            })
            .collect()
    }
}

impl Default for KeyboardSimulator {
    fn default() -> Self {
        // Average human typing speed: ~40 WPM
        Self::new(40)
    }
}
