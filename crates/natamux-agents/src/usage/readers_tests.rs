use super::*;
use crate::usage::usage_reader;

type Reader = fn(&Path) -> ModelUsage;

// Cut from real session files, with the prompt text and ids removed.
fn fixture(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("usage")
        .join(name)
}

fn row(model: &str, counts: [u64; 5]) -> (String, UsageCounts) {
    let [input, cache_read, cache_write, cache_write_1h, output] = counts;
    (
        model.to_string(),
        UsageCounts {
            input,
            cache_read,
            cache_write,
            cache_write_1h,
            output,
        },
    )
}

#[test]
fn read_claude_usage_counts_each_message_once_from_its_last_line_with_the_sub_agents() {
    let usage = read_claude_usage(&fixture("claude-session.jsonl"));
    assert_eq!(
        usage.0,
        [
            row("claude-opus-5-5", [12, 3000, 100, 40, 100]),
            row("claude-haiku-5-5", [5, 100, 0, 0, 7]),
        ]
    );
}

#[test]
fn read_codex_usage_counts_a_repeated_total_once_and_puts_each_change_on_the_model_of_its_turn() {
    let usage = read_codex_usage(&fixture("codex-rollout.jsonl"));
    assert_eq!(
        usage.0,
        [
            row("gpt-6-sol", [500, 400, 100, 0, 50]),
            row("gpt-6.1-sol", [400, 1600, 0, 0, 100]),
        ]
    );
}

#[test]
fn read_copilot_usage_sums_the_usage_records_without_the_cache_in_the_fresh_input() {
    let usage = read_copilot_usage(&fixture("copilot-events.jsonl"));
    assert_eq!(
        usage.0,
        [
            row("claude-sonnet-5.5", [6, 53_900, 12_286, 0, 366]),
            row("gpt-6.1-sol", [100, 0, 0, 0, 10]),
        ]
    );
}

#[test]
fn read_gemini_usage_sums_the_tokens_of_each_message_with_the_thoughts_as_output() {
    let usage = read_gemini_usage(&fixture("gemini-session.json"));
    assert_eq!(usage.0, [row("gemini-3.8-flash", [500, 2200, 0, 0, 85])]);
}

#[test]
fn usage_reader_has_a_reader_for_each_agent_that_writes_its_usage_and_none_for_cursor() {
    let read: [(&str, Reader); 4] = [
        ("claude-code", read_claude_usage),
        ("codex", read_codex_usage),
        ("copilot", read_copilot_usage),
        ("gemini", read_gemini_usage),
    ];
    for (agent, reader) in read {
        let found = usage_reader(agent).unwrap();
        assert_eq!(found as usize, reader as usize, "{agent}");
    }
    assert!(usage_reader("cursor").is_none());
}

fn write_lines(name: &str, lines: &[String]) -> (crate::test_support::TempDir, std::path::PathBuf) {
    let dir = crate::test_support::TempDir::new("usage-stream");
    let path = dir.path().join(name);
    std::fs::write(&path, lines.join("\n")).unwrap();
    (dir, path)
}

#[test]
fn readers_skip_big_broken_and_look_alike_lines_without_changing_the_totals() {
    let big = format!(r#"{{"type":"user","content":"{}"}}"#, "x".repeat(200_000));
    let look_alike = r#"{"type":"user","content":"the word \"usage\" in text"}"#.to_string();
    let broken = r#"{"message":{"usage":"#.to_string();
    let claude = |id: &str, out: u64| {
        format!(
            r#"{{"message":{{"id":"{id}","model":"m","usage":{{"input_tokens":1,"output_tokens":{out}}}}}}}"#
        )
    };
    let lines = [
        big.clone(),
        claude("a", 5),
        look_alike.clone(),
        broken.clone(),
        claude("a", 9),
        big.clone(),
        claude("b", 2),
    ];
    let (_dir, path) = write_lines("s.jsonl", &lines);
    assert_eq!(read_claude_usage(&path).0, [row("m", [2, 0, 0, 0, 11])]);

    let turn = r#"{"type":"turn_context","payload":{"model":"gpt"}}"#.to_string();
    let count = |input: u64| {
        format!(
            r#"{{"type":"event_msg","payload":{{"type":"token_count","info":{{"total_token_usage":{{"input_tokens":{input},"output_tokens":1}}}}}}}}"#
        )
    };
    let lines = [
        big.clone(),
        turn,
        look_alike.clone(),
        count(10),
        broken.clone(),
        count(10),
        count(30),
    ];
    let (_dir, path) = write_lines("c.jsonl", &lines);
    assert_eq!(read_codex_usage(&path).0, [row("gpt", [30, 0, 0, 0, 1])]);

    let record = r#"{"type":"session.usage_record","data":{"usage":{"inputTokens":7,"outputTokens":3,"model":"cp"}}}"#.to_string();
    let lines = [big, look_alike, broken, record.clone(), record];
    let (_dir, path) = write_lines("e.jsonl", &lines);
    assert_eq!(read_copilot_usage(&path).0, [row("cp", [14, 0, 0, 0, 6])]);
}
