//! Adapter that bridges the core `CaptchaSolver` trait to the solver crate's `SolverRegistry`.
//!
//! The core crate defines the `CaptchaSolver` trait (so it doesn't depend on Layer 2 crates).
//! The solver crate has `SolverRegistry` with its own `Solver` trait.
//! This adapter connects them so the orchestrator can use the solver registry.

use async_trait::async_trait;
use cloudyab_core::engine::{CaptchaSolver, EngineError};
use cloudyab_solver::local::{ImageClassifierSolver, SliderSolver, TextOcrSolver};
use cloudyab_solver::registry::SolverRegistry;
use cloudyab_types::captcha::{CaptchaResult, CaptchaType};
use std::path::Path;
use tracing::info;

/// Adapter that wraps `SolverRegistry` and implements the core `CaptchaSolver` trait.
pub struct SolverRegistryAdapter {
    registry: SolverRegistry,
}

impl SolverRegistryAdapter {
    /// Create a new adapter with all local solvers registered.
    pub fn new(models_dir: &Path) -> Self {
        let mut registry = SolverRegistry::new();

        registry.register(Box::new(TextOcrSolver::new(models_dir)));
        registry.register(Box::new(ImageClassifierSolver::new(models_dir)));
        registry.register(Box::new(SliderSolver));

        info!(
            solvers = registry.solver_count(),
            "Solver registry initialized"
        );

        Self { registry }
    }
}

#[async_trait]
impl CaptchaSolver for SolverRegistryAdapter {
    async fn solve(
        &self,
        image: &[u8],
        captcha_type: &CaptchaType,
        context: &str,
    ) -> Result<CaptchaResult, EngineError> {
        self.registry
            .solve(image, captcha_type, context)
            .await
            .map_err(|e| EngineError::CaptchaFailed(e.to_string()))
    }

    fn supports(&self, _captcha_type: &CaptchaType) -> bool {
        true
    }

    fn name(&self) -> &str {
        "solver-registry"
    }
}
