//! Human-like solution submitter implementation.
//!
//! Implements the core `SolutionSubmitter` trait using the interaction module
//! to generate Bézier mouse paths and realistic typing sequences, then
//! dispatches them via the engine's `evaluate_js` method.

use async_trait::async_trait;
use cloudyab_core::engine::{BrowsingEngine, EngineError, SolutionSubmitter};
use cloudyab_types::captcha::CaptchaSolution;
use tracing::info;

use crate::interaction;

/// Human-like solution submitter that generates natural interaction sequences.
pub struct HumanSubmitter;

#[async_trait]
impl SolutionSubmitter for HumanSubmitter {
    async fn submit(
        &self,
        engine: &dyn BrowsingEngine,
        solution: &CaptchaSolution,
        container_selector: Option<&str>,
    ) -> Result<(), EngineError> {
        let js = match solution {
            CaptchaSolution::SliderOffset(offset) => {
                let (start_x, start_y) = get_slider_position(engine, container_selector).await?;
                let end_x = start_x + *offset as f64;
                let commands = interaction::generate_drag_sequence(start_x, start_y, end_x);
                interaction::log_sequence_summary(&commands);
                interaction::commands_to_js(&commands)
            }
            CaptchaSolution::Text(text) => {
                let click_js = build_focus_input_js(container_selector);
                engine.evaluate_js(&click_js).await?;
                let commands = interaction::generate_typing_sequence(text);
                interaction::log_sequence_summary(&commands);
                interaction::commands_to_js(&commands)
            }
            CaptchaSolution::Coordinates(coords) => {
                let (base_x, base_y) =
                    get_captcha_image_position(engine, container_selector).await?;
                let commands = interaction::generate_multi_click_sequence(coords, base_x, base_y);
                interaction::log_sequence_summary(&commands);
                let js = interaction::commands_to_js(&commands);
                let verify_js = build_click_verify_js(container_selector);
                format!("{js}\n{verify_js}")
            }
            CaptchaSolution::Token(token) => {
                info!("Token solution — using direct injection");
                build_token_inject_js(token, container_selector)
            }
        };

        engine.evaluate_js(&js).await?;
        Ok(())
    }
}

/// Get the slider handle's center position via JS query.
async fn get_slider_position(
    engine: &dyn BrowsingEngine,
    container: Option<&str>,
) -> Result<(f64, f64), EngineError> {
    let container_sel = container.unwrap_or("document");
    let js = format!(
        r#"(() => {{
            const root = '{container_sel}' === 'document'
                ? document : document.querySelector('{container_sel}');
            const el = root.querySelector('.slider-handle, .slide-btn, [data-slider-handle]');
            if (!el) return null;
            const rect = el.getBoundingClientRect();
            return {{x: rect.left + rect.width / 2, y: rect.top + rect.height / 2}};
        }})()"#
    );

    let result = engine.evaluate_js(&js).await?;
    let x = result.get("x").and_then(|v| v.as_f64()).unwrap_or(400.0);
    let y = result.get("y").and_then(|v| v.as_f64()).unwrap_or(300.0);
    Ok((x, y))
}

/// Get the captcha image's top-left position for coordinate-based solutions.
async fn get_captcha_image_position(
    engine: &dyn BrowsingEngine,
    container: Option<&str>,
) -> Result<(f64, f64), EngineError> {
    let container_sel = container.unwrap_or("document");
    let js = format!(
        r#"(() => {{
            const root = '{container_sel}' === 'document'
                ? document : document.querySelector('{container_sel}');
            const img = root.querySelector('img, canvas, .captcha-image');
            if (!img) return null;
            const rect = img.getBoundingClientRect();
            return {{x: rect.left, y: rect.top}};
        }})()"#
    );

    let result = engine.evaluate_js(&js).await?;
    let x = result.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let y = result.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0);
    Ok((x, y))
}

/// Build JS to focus the captcha text input field.
fn build_focus_input_js(container: Option<&str>) -> String {
    let container_sel = container.unwrap_or("document");
    format!(
        r#"(() => {{
            const root = '{container_sel}' === 'document'
                ? document : document.querySelector('{container_sel}');
            const input = root.querySelector(
                'input[name*="captcha" i], input[placeholder*="code" i], input[type="text"]'
            );
            if (input) {{ input.focus(); input.value = ''; }}
        }})()"#
    )
}

/// Build JS to click the verify/submit button after coordinate selection.
fn build_click_verify_js(container: Option<&str>) -> String {
    let container_sel = container.unwrap_or("document");
    format!(
        r#"(() => {{
            const root = '{container_sel}' === 'document'
                ? document : document.querySelector('{container_sel}');
            const btn = root.querySelector(
                'button[type="submit"], .verify-btn, [data-action="verify"]'
            );
            if (btn) btn.click();
        }})()"#
    )
}

/// Build JS for direct token injection (no human interaction needed).
fn build_token_inject_js(token: &str, container: Option<&str>) -> String {
    let escaped = token.replace('\\', "\\\\").replace('\'', "\\'");
    let container_sel = container.unwrap_or("document");
    format!(
        r#"(() => {{
            const root = '{container_sel}' === 'document'
                ? document : document.querySelector('{container_sel}');
            const cb = root.querySelector('[data-callback]');
            if (cb) {{
                const fn_name = cb.getAttribute('data-callback');
                if (window[fn_name]) {{ window[fn_name]('{escaped}'); return true; }}
            }}
            const ta = root.querySelector('textarea[name*="response"], #g-recaptcha-response');
            if (ta) {{
                ta.value = '{escaped}';
                ta.dispatchEvent(new Event('input', {{bubbles: true}}));
                const form = ta.closest('form');
                if (form) form.submit();
                return true;
            }}
            return false;
        }})()"#
    )
}
