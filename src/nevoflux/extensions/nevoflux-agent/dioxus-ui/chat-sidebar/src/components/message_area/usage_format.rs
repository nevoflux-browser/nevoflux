/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Display formatting for a reply's token stats.
//!
//! All pure functions: the component only assembles them, so the thresholds
//! and wording stay testable. See
//! `docs/superpowers/specs/2026-09-22-message-token-stats-design.md`.

use shared_protocol::chat::TurnUsage;

/// Below this the generation window is too short to mean anything: a call that
/// emits all its tool-call arguments in one chunk has a window near zero and
/// would read as tens of thousands of tokens per second.
const MIN_DECODE_MS: u64 = 300;

/// Compact token count: 856, 12.4k, 101k, 1.2M.
pub fn compact_tokens(n: u64) -> String {
    match n {
        0..=999 => n.to_string(),
        1_000..=99_999 => format!("{:.1}k", n as f64 / 1_000.0),
        100_000..=999_999 => format!("{}k", (n as f64 / 1_000.0).round() as u64),
        _ => format!("{:.1}M", n as f64 / 1_000_000.0),
    }
}

/// Exact number with thousands separators, for the hover detail.
fn exact(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// Input across both parties.
fn total_input(usage: &TurnUsage) -> u64 {
    usage.main.input + usage.subagent.as_ref().map_or(0, |s| s.input)
}

/// Output across both parties.
fn total_output(usage: &TurnUsage) -> u64 {
    usage.main.output + usage.subagent.as_ref().map_or(0, |s| s.output)
}

/// Generation speed, from the main agent's output and generation window only.
///
/// `None` when there is no window, the window is too short to be meaningful,
/// or nothing was generated — in those cases no speed is shown at all.
pub fn tokens_per_second(usage: &TurnUsage) -> Option<String> {
    let decode_ms = usage.decode_ms?;
    if decode_ms < MIN_DECODE_MS || usage.main.output == 0 {
        return None;
    }
    let rate = usage.main.output as f64 / (decode_ms as f64 / 1000.0);
    if rate >= 10.0 {
        Some(format!("{} tok/s", rate.round() as u64))
    } else {
        Some(format!("{rate:.1} tok/s"))
    }
}

/// Whether any of the numbers came from estimation.
pub fn is_estimated(usage: &TurnUsage) -> bool {
    usage.main.estimated || usage.subagent.as_ref().is_some_and(|s| s.estimated)
}

/// The toolbar line, without the leading `≈` (the component adds that based
/// on [`is_estimated`]).
pub fn summary_line(usage: &TurnUsage) -> String {
    let base = format!(
        "↑{} ↓{}",
        compact_tokens(total_input(usage)),
        compact_tokens(total_output(usage))
    );
    match tokens_per_second(usage) {
        Some(speed) => format!("{base} · {speed}"),
        None => base,
    }
}

/// The hover detail, one line per entry.
pub fn detail_lines(usage: &TurnUsage) -> Vec<String> {
    let mut lines = Vec::new();

    lines.push(format!("Input   {} tokens", exact(total_input(usage))));
    let mut calls = format!("        {} LLM calls", usage.main.calls);
    if let Some(last) = usage.last_input {
        calls.push_str(&format!(" · last context {}", exact(last)));
    }
    lines.push(calls);
    if let Some(sub) = &usage.subagent {
        lines.push(format!(
            "        incl. subagents {}{}",
            exact(sub.input),
            if sub.estimated { " (estimated)" } else { "" }
        ));
    }

    lines.push(format!("Output  {} tokens", exact(total_output(usage))));
    if let Some(sub) = &usage.subagent {
        lines.push(format!(
            "        incl. subagents {}{}",
            exact(sub.output),
            if sub.estimated { " (estimated)" } else { "" }
        ));
    }

    match (tokens_per_second(usage), usage.decode_ms) {
        (Some(speed), Some(ms)) => lines.push(format!(
            "Speed   {speed} (main agent, {:.1}s generating)",
            ms as f64 / 1000.0
        )),
        _ if usage.external_agent => lines.push(
            "Speed   not available: this agent runs its own loop, so tool time can't be separated from generation".into(),
        ),
        _ => {}
    }

    let mut timing = Vec::new();
    if let Some(ms) = usage.first_token_ms {
        timing.push(format!("First token {:.1}s", ms as f64 / 1000.0));
    }
    if let Some(ms) = usage.total_ms {
        timing.push(format!("Total {:.1}s", ms as f64 / 1000.0));
    }
    if !timing.is_empty() {
        lines.push(timing.join(" · "));
    }

    if let Some(model) = &usage.model {
        lines.push(format!("Model   {model}"));
    }

    if is_estimated(usage) {
        lines.push("Provider reported no usage — values are estimated".into());
        if usage.external_agent {
            lines.push("Input excludes the agent's own system prompt and tool rounds".into());
        }
    }

    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared_protocol::chat::UsageBucket;

    fn usage() -> TurnUsage {
        TurnUsage {
            main: UsageBucket {
                input: 8329,
                output: 646,
                calls: 5,
                estimated: false,
            },
            subagent: None,
            last_input: Some(3204),
            decode_ms: Some(15300),
            first_token_ms: Some(1200),
            total_ms: Some(28400),
            model: Some("claude-sonnet-5".into()),
            external_agent: false,
        }
    }

    #[test]
    fn compact_tokens_scales_by_magnitude() {
        assert_eq!(compact_tokens(0), "0");
        assert_eq!(compact_tokens(856), "856");
        assert_eq!(compact_tokens(12_431), "12.4k");
        assert_eq!(compact_tokens(100_500), "101k");
        assert_eq!(compact_tokens(1_240_000), "1.2M");
    }

    #[test]
    fn speed_uses_main_output_over_decode_time() {
        // 646 tokens over 15.3s
        assert_eq!(tokens_per_second(&usage()).as_deref(), Some("42 tok/s"));
    }

    #[test]
    fn slow_speeds_keep_one_decimal() {
        let u = TurnUsage {
            main: UsageBucket {
                output: 42,
                ..usage().main
            },
            decode_ms: Some(10_000),
            ..usage()
        };
        assert_eq!(tokens_per_second(&u).as_deref(), Some("4.2 tok/s"));
    }

    #[test]
    fn speed_is_absent_without_a_decode_window() {
        let u = TurnUsage {
            decode_ms: None,
            ..usage()
        };
        assert!(tokens_per_second(&u).is_none());
    }

    #[test]
    fn speed_is_absent_for_windows_below_the_floor() {
        let u = TurnUsage {
            decode_ms: Some(120),
            ..usage()
        };
        assert!(tokens_per_second(&u).is_none());
    }

    #[test]
    fn speed_is_absent_without_output() {
        let u = TurnUsage {
            main: UsageBucket {
                output: 0,
                ..usage().main
            },
            ..usage()
        };
        assert!(tokens_per_second(&u).is_none());
    }

    #[test]
    fn summary_line_includes_totals_and_speed() {
        assert_eq!(summary_line(&usage()), "↑8.3k ↓646 · 42 tok/s");
    }

    #[test]
    fn summary_line_adds_subagent_totals() {
        let u = TurnUsage {
            subagent: Some(UsageBucket {
                input: 4102,
                output: 210,
                calls: 3,
                estimated: false,
            }),
            ..usage()
        };
        assert_eq!(summary_line(&u), "↑12.4k ↓856 · 42 tok/s");
    }

    #[test]
    fn summary_line_drops_speed_when_unavailable() {
        let u = TurnUsage {
            decode_ms: None,
            ..usage()
        };
        assert_eq!(summary_line(&u), "↑8.3k ↓646");
    }

    #[test]
    fn estimation_is_flagged_when_either_bucket_is_estimated() {
        assert!(!is_estimated(&usage()));
        let main_estimated = TurnUsage {
            main: UsageBucket {
                estimated: true,
                ..usage().main
            },
            ..usage()
        };
        assert!(is_estimated(&main_estimated));
        let sub_estimated = TurnUsage {
            subagent: Some(UsageBucket {
                input: 1,
                output: 1,
                calls: 1,
                estimated: true,
            }),
            ..usage()
        };
        assert!(is_estimated(&sub_estimated));
    }

    #[test]
    fn detail_lines_spell_out_exact_numbers() {
        let lines = detail_lines(&usage());
        assert!(
            lines.iter().any(|l| l.contains("8,329")),
            "exact input: {lines:?}"
        );
        assert!(
            lines.iter().any(|l| l.contains("5 LLM calls")),
            "call count: {lines:?}"
        );
        assert!(
            lines.iter().any(|l| l.contains("last context 3,204")),
            "context size: {lines:?}"
        );
        assert!(
            lines.iter().any(|l| l.contains("claude-sonnet-5")),
            "model: {lines:?}"
        );
    }

    #[test]
    fn detail_lines_explain_external_agents() {
        let u = TurnUsage {
            external_agent: true,
            decode_ms: None,
            main: UsageBucket {
                estimated: true,
                ..usage().main
            },
            ..usage()
        };
        let lines = detail_lines(&u);
        assert!(
            lines.iter().any(|l| l.starts_with("Speed")),
            "says why there is no speed: {lines:?}"
        );
        assert!(
            lines.iter().any(|l| l.contains("system prompt")),
            "says the input estimate misses the agent's own overhead: {lines:?}"
        );
    }
}
