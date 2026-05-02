//! Natural scrolling simulation.

use rand::Rng;
use std::time::Duration;

/// A scroll event with delta and timing.
#[derive(Debug, Clone)]
pub struct ScrollStep {
    /// Pixels to scroll (positive = down)
    pub delta_y: i32,
    /// Delay before this scroll event
    pub delay: Duration,
}

/// Generates human-like scroll patterns.
pub struct ScrollSimulator;

impl ScrollSimulator {
    /// Generate a scroll-down sequence to move `total_pixels` down.
    pub fn scroll_down(total_pixels: i32) -> Vec<ScrollStep> {
        let mut rng = rand::thread_rng();
        let mut steps = Vec::new();
        let mut scrolled = 0;

        while scrolled < total_pixels {
            // Variable scroll amounts (like a mouse wheel)
            let delta = rng.gen_range(40..120).min(total_pixels - scrolled);
            let delay = Duration::from_millis(rng.gen_range(30..80));

            steps.push(ScrollStep {
                delta_y: delta,
                delay,
            });
            scrolled += delta;
        }

        // Add a small pause at the end (human reads content)
        if let Some(last) = steps.last_mut() {
            last.delay = Duration::from_millis(rng.gen_range(200..500));
        }

        steps
    }

    /// Generate a scroll-up sequence.
    pub fn scroll_up(total_pixels: i32) -> Vec<ScrollStep> {
        Self::scroll_down(total_pixels)
            .into_iter()
            .map(|s| ScrollStep {
                delta_y: -s.delta_y,
                delay: s.delay,
            })
            .collect()
    }
}
