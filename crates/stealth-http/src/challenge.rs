//! Cloudflare JavaScript challenge solver using boa_engine.
//!
//! Inspired by cloudscraper: intercepts CF's JS challenge, executes it
//! in a sandboxed JS interpreter, and submits the computed answer.

use boa_engine::{Context, Source};
use thiserror::Error;
use tracing::debug;

/// Errors from challenge solving.
#[derive(Debug, Error)]
pub enum ChallengeError {
    #[error("Failed to parse challenge from page: {0}")]
    ParseFailed(String),

    #[error("JS execution failed: {0}")]
    JsExecutionFailed(String),

    #[error("Challenge answer computation failed: {0}")]
    ComputationFailed(String),
}

/// Cloudflare challenge solver.
pub struct ChallengeSolver;

impl ChallengeSolver {
    /// Attempt to solve a Cloudflare JS challenge from the page HTML.
    ///
    /// Returns the computed answer and the submission URL if successful.
    pub fn solve_cf_challenge(
        html: &str,
        domain: &str,
    ) -> Result<ChallengeAnswer, ChallengeError> {
        // Extract the JS challenge code from the page
        let js_code = Self::extract_challenge_js(html)?;

        debug!(domain, "Extracted CF challenge JS, executing...");

        // Execute in sandboxed JS context
        let answer = Self::execute_js(&js_code, domain)?;

        // Extract the submission form data
        let form_data = Self::extract_form_data(html)?;

        Ok(ChallengeAnswer {
            answer,
            submit_url: form_data.action,
            form_params: form_data.params,
        })
    }

    /// Extract the JavaScript challenge code from CF's page.
    fn extract_challenge_js(html: &str) -> Result<String, ChallengeError> {
        // CF embeds the challenge in a <script> tag with specific patterns
        // Look for the challenge computation script
        let start_markers = [
            "setTimeout(function(){",
            "var s,t,o,p,b,r,e,a,k,i,n,g,f,",
        ];

        for marker in &start_markers {
            if let Some(start) = html.find(marker) {
                // Find the end of the script block
                if let Some(end) = html[start..].find("</script>") {
                    let script = &html[start..start + end];
                    return Ok(script.to_string());
                }
            }
        }

        Err(ChallengeError::ParseFailed(
            "Could not locate challenge script in page".into(),
        ))
    }

    /// Execute JavaScript in a sandboxed boa context.
    fn execute_js(code: &str, domain: &str) -> Result<String, ChallengeError> {
        let mut context = Context::default();

        // Set up the minimal DOM environment CF expects
        let setup_js = format!(
            r#"
            var document = {{
                getElementById: function(id) {{
                    return {{ innerHTML: '', value: '' }};
                }},
                createElement: function(tag) {{
                    return {{ href: '', firstChild: {{ href: 'https://{}/' }} }};
                }}
            }};
            var location = {{ hostname: '{}' }};
            "#,
            domain, domain
        );

        context
            .eval(Source::from_bytes(&setup_js))
            .map_err(|e| ChallengeError::JsExecutionFailed(format!("Setup failed: {e}")))?;

        // Execute the challenge code
        let result = context
            .eval(Source::from_bytes(code))
            .map_err(|e| ChallengeError::JsExecutionFailed(format!("Execution failed: {e}")))?;

        let answer = result
            .to_string(&mut context)
            .map_err(|e| ChallengeError::ComputationFailed(format!("ToString failed: {e}")))?;

        debug!("Challenge answer computed successfully");
        Ok(answer.to_std_string_escaped())
    }

    /// Extract form submission data from the challenge page.
    fn extract_form_data(html: &str) -> Result<FormData, ChallengeError> {
        // CF challenge pages have a form with action URL and hidden fields
        let action = if let Some(start) = html.find("action=\"") {
            let rest = &html[start + 8..];
            if let Some(end) = rest.find('"') {
                rest[..end].to_string()
            } else {
                return Err(ChallengeError::ParseFailed("No form action end quote".into()));
            }
        } else {
            return Err(ChallengeError::ParseFailed("No form action found".into()));
        };

        // Extract hidden input fields
        let mut params = Vec::new();
        let mut search_from = 0;
        while let Some(pos) = html[search_from..].find("name=\"") {
            let abs_pos = search_from + pos + 6;
            if let Some(end) = html[abs_pos..].find('"') {
                let name = html[abs_pos..abs_pos + end].to_string();

                // Find corresponding value
                if let Some(val_pos) = html[abs_pos..].find("value=\"") {
                    let val_start = abs_pos + val_pos + 7;
                    if let Some(val_end) = html[val_start..].find('"') {
                        let value = html[val_start..val_start + val_end].to_string();
                        params.push((name, value));
                    }
                }
            }
            search_from = abs_pos + 1;
        }

        Ok(FormData { action, params })
    }
}

/// Computed challenge answer ready for submission.
pub struct ChallengeAnswer {
    /// The computed answer value
    pub answer: String,
    /// URL to submit the answer to
    pub submit_url: String,
    /// Additional form parameters to include
    pub form_params: Vec<(String, String)>,
}

/// Extracted form data from a challenge page.
struct FormData {
    action: String,
    params: Vec<(String, String)>,
}
