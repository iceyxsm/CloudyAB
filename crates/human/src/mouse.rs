//! Bézier curve mouse movement simulation.
//!
//! Generates human-like mouse paths using cubic Bézier curves with
//! randomized control points and variable speed.

use rand::Rng;
use std::time::Duration;

/// A point on the screen.
#[derive(Debug, Clone, Copy)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

/// A mouse movement step with position and timing.
#[derive(Debug, Clone)]
pub struct MouseStep {
    pub point: Point,
    pub delay: Duration,
}

/// Generates human-like mouse movement paths.
pub struct MouseSimulator {
    /// Minimum movement duration in ms
    min_duration_ms: u64,
    /// Maximum movement duration in ms
    max_duration_ms: u64,
    /// Number of intermediate points to generate
    steps: usize,
}

impl MouseSimulator {
    /// Create a new mouse simulator with default settings.
    pub fn new() -> Self {
        Self {
            min_duration_ms: 200,
            max_duration_ms: 600,
            steps: 20,
        }
    }

    /// Generate a Bézier curve path from start to end.
    pub fn generate_path(&self, from: Point, to: Point) -> Vec<MouseStep> {
        let mut rng = rand::thread_rng();

        // Calculate distance for duration scaling
        let distance = ((to.x - from.x).powi(2) + (to.y - from.y).powi(2)).sqrt();
        let duration_ms = (self.min_duration_ms as f64
            + (distance / 1000.0) * (self.max_duration_ms - self.min_duration_ms) as f64)
            .min(self.max_duration_ms as f64) as u64;

        // Generate random control points for cubic Bézier
        let cp1 = Point {
            x: from.x + (to.x - from.x) * rng.gen_range(0.2..0.5) + rng.gen_range(-50.0..50.0),
            y: from.y + (to.y - from.y) * rng.gen_range(0.0..0.3) + rng.gen_range(-50.0..50.0),
        };
        let cp2 = Point {
            x: from.x + (to.x - from.x) * rng.gen_range(0.5..0.8) + rng.gen_range(-30.0..30.0),
            y: from.y + (to.y - from.y) * rng.gen_range(0.7..1.0) + rng.gen_range(-30.0..30.0),
        };

        let mut steps = Vec::with_capacity(self.steps);
        let step_duration = Duration::from_millis(duration_ms / self.steps as u64);

        for i in 0..=self.steps {
            let t = i as f64 / self.steps as f64;

            // Cubic Bézier formula: B(t) = (1-t)³P0 + 3(1-t)²tP1 + 3(1-t)t²P2 + t³P3
            let mt = 1.0 - t;
            let mt2 = mt * mt;
            let mt3 = mt2 * mt;
            let t2 = t * t;
            let t3 = t2 * t;

            let x = mt3 * from.x + 3.0 * mt2 * t * cp1.x + 3.0 * mt * t2 * cp2.x + t3 * to.x;
            let y = mt3 * from.y + 3.0 * mt2 * t * cp1.y + 3.0 * mt * t2 * cp2.y + t3 * to.y;

            // Add slight jitter to simulate hand tremor
            let jitter_x = rng.gen_range(-0.5..0.5);
            let jitter_y = rng.gen_range(-0.5..0.5);

            // Variable speed: slower at start and end (ease-in-out)
            let speed_factor = 1.0 - (2.0 * t - 1.0).powi(2); // parabolic
            let delay = Duration::from_millis(
                (step_duration.as_millis() as f64 * (0.5 + speed_factor * 0.8)) as u64,
            );

            steps.push(MouseStep {
                point: Point {
                    x: x + jitter_x,
                    y: y + jitter_y,
                },
                delay,
            });
        }

        steps
    }

    /// Generate a small random offset for clicking (humans don't click exact center).
    pub fn click_offset(&self) -> (f64, f64) {
        let mut rng = rand::thread_rng();
        (rng.gen_range(-3.0..3.0), rng.gen_range(-3.0..3.0))
    }
}

impl Default for MouseSimulator {
    fn default() -> Self {
        Self::new()
    }
}
