//! Human-like interaction dispatch for captcha solution submission.
//!
//! Uses the `human` crate's Bézier mouse paths and realistic keystroke timing
//! to generate natural-looking interaction sequences. These are dispatched via
//! the browser engine's CDP page using JS event simulation.
//!
//! This module lives in the server binary because it bridges two Layer 2 crates
//! (browser + human) which cannot depend on each other directly.

use cloudyab_human::keyboard::KeyboardSimulator;
use cloudyab_human::mouse::{MouseSimulator, Point};
use tracing::info;

/// Minimum delay between major interaction phases (ms).
const PHASE_DELAY_MS: u64 = 150;

/// A sequence of interaction commands to execute on a page.
/// These are generated using human-like timing and then dispatched via CDP.
#[derive(Debug, Clone)]
pub enum InteractionCommand {
    /// Move mouse along a path (list of x,y,delay_ms tuples).
    MouseMove(Vec<(f64, f64, u64)>),
    /// Click at coordinates.
    Click(f64, f64),
    /// Mouse down at coordinates.
    MouseDown(f64, f64),
    /// Mouse up at coordinates.
    MouseUp(f64, f64),
    /// Type a character with delay.
    KeyPress(char, u64),
    /// Wait for a duration.
    Wait(u64),
}

/// Generate a human-like click sequence at the given coordinates.
pub fn generate_click_sequence(target_x: f64, target_y: f64) -> Vec<InteractionCommand> {
    let simulator = MouseSimulator::new();
    let (offset_x, offset_y) = simulator.click_offset();
    let final_x = target_x + offset_x;
    let final_y = target_y + offset_y;

    let from = Point {
        x: final_x - 100.0,
        y: final_y - 50.0,
    };
    let to = Point {
        x: final_x,
        y: final_y,
    };

    let path = simulator.generate_path(from, to);
    let move_steps: Vec<(f64, f64, u64)> = path
        .iter()
        .map(|s| (s.point.x, s.point.y, s.delay.as_millis() as u64))
        .collect();

    vec![
        InteractionCommand::MouseMove(move_steps),
        InteractionCommand::Wait(PHASE_DELAY_MS),
        InteractionCommand::Click(final_x, final_y),
    ]
}

/// Generate a human-like slider drag sequence.
pub fn generate_drag_sequence(
    start_x: f64,
    start_y: f64,
    end_x: f64,
) -> Vec<InteractionCommand> {
    let simulator = MouseSimulator::new();

    // Approach the slider handle
    let approach_from = Point {
        x: start_x - 80.0,
        y: start_y - 30.0,
    };
    let start = Point {
        x: start_x,
        y: start_y,
    };
    let approach_path = simulator.generate_path(approach_from, start);
    let approach_steps: Vec<(f64, f64, u64)> = approach_path
        .iter()
        .map(|s| (s.point.x, s.point.y, s.delay.as_millis() as u64))
        .collect();

    // Drag path from start to end
    let end = Point {
        x: end_x,
        y: start_y,
    };
    let drag_path = simulator.generate_path(start, end);
    let drag_steps: Vec<(f64, f64, u64)> = drag_path
        .iter()
        .map(|s| (s.point.x, s.point.y, s.delay.as_millis() as u64))
        .collect();

    vec![
        InteractionCommand::MouseMove(approach_steps),
        InteractionCommand::Wait(PHASE_DELAY_MS),
        InteractionCommand::MouseDown(start_x, start_y),
        InteractionCommand::Wait(50),
        InteractionCommand::MouseMove(drag_steps),
        InteractionCommand::Wait(50),
        InteractionCommand::MouseUp(end_x, start_y),
    ]
}

/// Generate a human-like typing sequence for text input.
pub fn generate_typing_sequence(text: &str) -> Vec<InteractionCommand> {
    let simulator = KeyboardSimulator::default();
    let keystrokes = simulator.generate_keystrokes(text);

    keystrokes
        .iter()
        .map(|ks| InteractionCommand::KeyPress(ks.char, ks.delay_before.as_millis() as u64))
        .collect()
}

/// Generate a human-like coordinate click sequence (for image grid captchas).
pub fn generate_multi_click_sequence(coords: &[(i32, i32)], base_x: f64, base_y: f64) -> Vec<InteractionCommand> {
    let simulator = MouseSimulator::new();
    let mut commands = Vec::new();
    let mut current = Point {
        x: base_x,
        y: base_y,
    };

    for (cx, cy) in coords {
        let target = Point {
            x: base_x + *cx as f64,
            y: base_y + *cy as f64,
        };

        let path = simulator.generate_path(current, target);
        let steps: Vec<(f64, f64, u64)> = path
            .iter()
            .map(|s| (s.point.x, s.point.y, s.delay.as_millis() as u64))
            .collect();

        commands.push(InteractionCommand::MouseMove(steps));
        commands.push(InteractionCommand::Wait(PHASE_DELAY_MS));
        commands.push(InteractionCommand::Click(target.x, target.y));
        // Random pause between clicks (300-800ms)
        commands.push(InteractionCommand::Wait(300 + (target.x as u64 % 500)));

        current = target;
    }

    commands
}

/// Convert an interaction command sequence into JavaScript for CDP execution.
/// Returns an async JS function that executes the sequence with proper timing.
pub fn commands_to_js(commands: &[InteractionCommand]) -> String {
    let mut js_parts = Vec::new();
    js_parts.push("(async () => {".to_string());
    js_parts.push("  function sleep(ms) { return new Promise(r => setTimeout(r, ms)); }".to_string());
    js_parts.push("  function dispatch(type, x, y) {".to_string());
    js_parts.push("    const el = document.elementFromPoint(x, y) || document.body;".to_string());
    js_parts.push("    el.dispatchEvent(new MouseEvent(type, {clientX:x, clientY:y, bubbles:true}));".to_string());
    js_parts.push("  }".to_string());
    js_parts.push("  function keyAt(ch) {".to_string());
    js_parts.push("    const el = document.activeElement || document.body;".to_string());
    js_parts.push("    el.dispatchEvent(new KeyboardEvent('keydown', {key:ch, bubbles:true}));".to_string());
    js_parts.push("    if (el.tagName==='INPUT'||el.tagName==='TEXTAREA') {".to_string());
    js_parts.push("      el.value += ch;".to_string());
    js_parts.push("      el.dispatchEvent(new Event('input', {bubbles:true}));".to_string());
    js_parts.push("    }".to_string());
    js_parts.push("    el.dispatchEvent(new KeyboardEvent('keyup', {key:ch, bubbles:true}));".to_string());
    js_parts.push("  }".to_string());

    for cmd in commands {
        match cmd {
            InteractionCommand::MouseMove(steps) => {
                for (x, y, delay) in steps {
                    js_parts.push(format!("  dispatch('mousemove',{x},{y}); await sleep({delay});"));
                }
            }
            InteractionCommand::Click(x, y) => {
                js_parts.push(format!("  dispatch('mousedown',{x},{y}); await sleep(50);"));
                js_parts.push(format!("  dispatch('mouseup',{x},{y});"));
                js_parts.push(format!("  dispatch('click',{x},{y});"));
            }
            InteractionCommand::MouseDown(x, y) => {
                js_parts.push(format!("  dispatch('mousedown',{x},{y});"));
            }
            InteractionCommand::MouseUp(x, y) => {
                js_parts.push(format!("  dispatch('mouseup',{x},{y});"));
            }
            InteractionCommand::KeyPress(ch, delay) => {
                let escaped = ch.to_string().replace('\'', "\\'").replace('\\', "\\\\");
                js_parts.push(format!("  await sleep({delay}); keyAt('{escaped}');"));
            }
            InteractionCommand::Wait(ms) => {
                js_parts.push(format!("  await sleep({ms});"));
            }
        }
    }

    js_parts.push("})()".to_string());
    js_parts.join("\n")
}

/// Log a summary of the interaction sequence for debugging.
pub fn log_sequence_summary(commands: &[InteractionCommand]) {
    let moves = commands
        .iter()
        .filter(|c| matches!(c, InteractionCommand::MouseMove(_)))
        .count();
    let clicks = commands
        .iter()
        .filter(|c| matches!(c, InteractionCommand::Click(_, _)))
        .count();
    let keys = commands
        .iter()
        .filter(|c| matches!(c, InteractionCommand::KeyPress(_, _)))
        .count();
    let total_delay: u64 = commands
        .iter()
        .map(|c| match c {
            InteractionCommand::Wait(ms) => *ms,
            InteractionCommand::KeyPress(_, ms) => *ms,
            InteractionCommand::MouseMove(steps) => steps.iter().map(|(_, _, d)| d).sum(),
            _ => 0,
        })
        .sum();

    info!(
        moves,
        clicks,
        keys,
        total_delay_ms = total_delay,
        "Human interaction sequence generated"
    );
}
