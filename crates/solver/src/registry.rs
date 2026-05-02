//! Solver registry — manages multiple solver backends and routes captchas.

use crate::traits::{Solver, SolverError};
use cloudyab_types::captcha::{CaptchaResult, CaptchaType};
use tracing::{info, warn};

/// Registry of captcha solver backends.
///
/// Solvers are tried in priority order (lowest number first).
/// If one fails, the next is tried until all are exhausted.
pub struct SolverRegistry {
    solvers: Vec<Box<dyn Solver>>,
}

impl SolverRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            solvers: Vec::new(),
        }
    }

    /// Register a new solver backend.
    pub fn register(&mut self, solver: Box<dyn Solver>) {
        info!(solver = solver.name(), priority = solver.priority(), "Registered captcha solver");
        self.solvers.push(solver);
        // Keep sorted by priority
        self.solvers.sort_by_key(|s| s.priority());
    }

    /// Attempt to solve a captcha using registered solvers.
    ///
    /// Tries each compatible solver in priority order until one succeeds.
    pub async fn solve(
        &self,
        image: &[u8],
        captcha_type: &CaptchaType,
        context: &str,
    ) -> Result<CaptchaResult, SolverError> {
        let compatible: Vec<_> = self
            .solvers
            .iter()
            .filter(|s| s.supports(captcha_type))
            .collect();

        if compatible.is_empty() {
            return Err(SolverError::Unsupported(format!("{captcha_type:?}")));
        }

        for solver in &compatible {
            info!(solver = solver.name(), "Attempting captcha solve");

            match solver.solve(image, captcha_type, context).await {
                Ok(result) if result.success => {
                    info!(solver = solver.name(), "Captcha solved successfully");
                    return Ok(result);
                }
                Ok(result) => {
                    warn!(
                        solver = solver.name(),
                        error = ?result.error,
                        "Solver returned unsuccessful result, trying next"
                    );
                }
                Err(e) => {
                    warn!(solver = solver.name(), error = %e, "Solver failed, trying next");
                }
            }
        }

        Err(SolverError::Exhausted)
    }

    /// Get the number of registered solvers.
    pub fn solver_count(&self) -> usize {
        self.solvers.len()
    }
}

impl Default for SolverRegistry {
    fn default() -> Self {
        Self::new()
    }
}
