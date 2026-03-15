use std::env;
use std::io::{self, Read};
use std::process;
use std::time::Instant;

const GREP_MESSAGE: &str = "Use rg (ripgrep) instead of grep in this project. ripgrep is faster, respects .gitignore, and uses modern regex by default. Replace the blocked command with the equivalent 'rg ...' command.";

#[derive(Clone, Debug, Eq, PartialEq)]
struct BlockDecision {
    message: String,
}

impl BlockDecision {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

fn main() {
    let exit_code = match run() {
        Ok(code) => code,
        Err(message) => {
            eprintln!("{message}");
            1
        }
    };

    process::exit(exit_code);
}

fn run() -> Result<i32, String> {
    let config = Config::parse(env::args().skip(1))?;

    match config.mode {
        Mode::Evaluate { input, claude_json } => {
            let raw = match input {
                InputMode::Command(text) => text,
                InputMode::StdinCommand => read_stdin()?,
                InputMode::ClaudeHookJson => extract_claude_command(&read_stdin()?)?,
            };

            match evaluate_command(raw.trim()) {
                Some(decision) if claude_json => {
                    println!(
                        "{{\"decision\":\"block\",\"reason\":\"{}\"}}",
                        escape_json(&decision.message)
                    );
                    Ok(0)
                }
                Some(decision) => {
                    eprintln!("{}", decision.message);
                    Ok(2)
                }
                None => Ok(0),
            }
        }
        Mode::Benchmark {
            command,
            iterations,
        } => {
            if iterations == 0 {
                return Err("iterations must be greater than 0".to_string());
            }

            let start = Instant::now();
            let mut blocks = 0u64;

            for _ in 0..iterations {
                if evaluate_command(&command).is_some() {
                    blocks += 1;
                }
            }

            let elapsed = start.elapsed();
            let total_ns = elapsed.as_nanos();
            let avg_ns = total_ns as f64 / iterations as f64;
            let avg_us = avg_ns / 1_000.0;

            println!("iterations={iterations}");
            println!("blocked={blocks}");
            println!("total_ns={total_ns}");
            println!("avg_ns={avg_ns:.2}");
            println!("avg_us={avg_us:.4}");
            Ok(0)
        }
    }
}

#[derive(Debug)]
struct Config {
    mode: Mode,
}

#[derive(Debug)]
enum Mode {
    Evaluate { input: InputMode, claude_json: bool },
    Benchmark { command: String, iterations: u64 },
}

#[derive(Debug)]
enum InputMode {
    Command(String),
    StdinCommand,
    ClaudeHookJson,
}

impl Config {
    fn parse<I>(args: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = String>,
    {
        let mut input: Option<InputMode> = None;
        let mut claude_json = false;
        let mut benchmark_command: Option<String> = None;
        let mut iterations = 100_000u64;

        let mut iter = args.into_iter();
        while let Some(arg) = iter.next() {
            match arg.as_str() {
                "--command" => {
                    let value = iter
                        .next()
                        .ok_or_else(|| "missing value for --command".to_string())?;
                    input = Some(InputMode::Command(value));
                }
                "--stdin-command" => {
                    input = Some(InputMode::StdinCommand);
                }
                "--claude-hook-json" => {
                    input = Some(InputMode::ClaudeHookJson);
                    claude_json = true;
                }
                "--gemini-hook-json" => {
                    input = Some(InputMode::ClaudeHookJson);
                }
                "--claude-json" => {
                    claude_json = true;
                }
                "--benchmark-command" => {
                    let value = iter
                        .next()
                        .ok_or_else(|| "missing value for --benchmark-command".to_string())?;
                    benchmark_command = Some(value);
                }
                "--iterations" => {
                    let value = iter
                        .next()
                        .ok_or_else(|| "missing value for --iterations".to_string())?;
                    iterations = value
                        .parse::<u64>()
                        .map_err(|_| "iterations must be an integer".to_string())?;
                }
                "--help" | "-h" => {
                    print_usage();
                    return Ok(Self {
                        mode: Mode::Evaluate {
                            input: InputMode::Command(String::new()),
                            claude_json: false,
                        },
                    });
                }
                _ => {
                    return Err(format!("unknown argument: {arg}"));
                }
            }
        }

        if let Some(command) = benchmark_command {
            return Ok(Self {
                mode: Mode::Benchmark {
                    command,
                    iterations,
                },
            });
        }

        let input = input.ok_or_else(|| {
            "expected one of --command, --stdin-command, or --claude-hook-json".to_string()
        })?;

        Ok(Self {
            mode: Mode::Evaluate { input, claude_json },
        })
    }
}

fn print_usage() {
    println!(
        "Usage:\n  enforce-rg-command --command \"grep -rn pattern .\" [--claude-json]\n  enforce-rg-command --stdin-command [--claude-json]\n  enforce-rg-command --claude-hook-json\n  enforce-rg-command --gemini-hook-json\n  enforce-rg-command --benchmark-command \"grep -rn pattern .\" [--iterations 1000000]"
    );
}

fn read_stdin() -> Result<String, String> {
    let mut buffer = String::new();
    io::stdin()
        .read_to_string(&mut buffer)
        .map_err(|error| format!("failed to read stdin: {error}"))?;
    Ok(buffer)
}

fn escape_json(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 8);
    for ch in value.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn evaluate_command(command: &str) -> Option<BlockDecision> {
    for segment in parse_segments(command) {
        if let Some(decision) = evaluate_segment(&segment.tokens) {
            return Some(decision);
        }
    }

    None
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ParsedToken {
    raw: String,
    value: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct ParsedSegment {
    tokens: Vec<ParsedToken>,
}

fn parse_segments(command: &str) -> Vec<ParsedSegment> {
    let bytes = command.as_bytes();
    let mut segments = Vec::new();
    let mut tokens = Vec::new();
    let mut raw = Vec::with_capacity(32);
    let mut value = Vec::with_capacity(32);
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut index = 0usize;

    while index < bytes.len() {
        let byte = bytes[index];

        if in_single_quote {
            raw.push(byte);
            if byte == b'\'' {
                in_single_quote = false;
            } else {
                value.push(byte);
            }
            index += 1;
            continue;
        }

        if in_double_quote {
            match byte {
                b'"' => {
                    raw.push(byte);
                    in_double_quote = false;
                }
                b'\\' => {
                    raw.push(byte);
                    if index + 1 < bytes.len() {
                        index += 1;
                        raw.push(bytes[index]);
                        value.push(bytes[index]);
                    } else {
                        value.push(b'\\');
                    }
                }
                _ => {
                    raw.push(byte);
                    value.push(byte);
                }
            }
            index += 1;
            continue;
        }

        match byte {
            b' ' | b'\n' | b'\r' | b'\t' => flush_parsed_token(&mut raw, &mut value, &mut tokens),
            b'\'' => {
                raw.push(byte);
                in_single_quote = true;
            }
            b'"' => {
                raw.push(byte);
                in_double_quote = true;
            }
            b';' => {
                flush_parsed_token(&mut raw, &mut value, &mut tokens);
                flush_segment(&mut tokens, &mut segments);
            }
            b'|' | b'&' => {
                flush_parsed_token(&mut raw, &mut value, &mut tokens);
                flush_segment(&mut tokens, &mut segments);
                if index + 1 < bytes.len() && bytes[index + 1] == byte {
                    index += 1;
                }
            }
            b'\\' => {
                raw.push(byte);
                if index + 1 < bytes.len() {
                    index += 1;
                    raw.push(bytes[index]);
                    value.push(bytes[index]);
                } else {
                    value.push(b'\\');
                }
            }
            _ => {
                raw.push(byte);
                value.push(byte);
            }
        }

        index += 1;
    }

    flush_parsed_token(&mut raw, &mut value, &mut tokens);
    flush_segment(&mut tokens, &mut segments);
    segments
}

fn flush_parsed_token(raw: &mut Vec<u8>, value: &mut Vec<u8>, tokens: &mut Vec<ParsedToken>) {
    if raw.is_empty() {
        return;
    }

    tokens.push(ParsedToken {
        raw: String::from_utf8_lossy(raw).into_owned(),
        value: String::from_utf8_lossy(value).into_owned(),
    });
    raw.clear();
    value.clear();
}

fn flush_segment(tokens: &mut Vec<ParsedToken>, segments: &mut Vec<ParsedSegment>) {
    if tokens.is_empty() {
        return;
    }

    segments.push(ParsedSegment {
        tokens: std::mem::take(tokens),
    });
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SegmentState {
    SeekCommand,
}

fn evaluate_segment(tokens: &[ParsedToken]) -> Option<BlockDecision> {
    let state = SegmentState::SeekCommand;
    let mut wrapper = None;
    let mut skip_next_value = false;

    for (index, token) in tokens.iter().enumerate() {
        let value = token.value.as_bytes();

        if skip_next_value {
            skip_next_value = false;
            continue;
        }

        match state {
            SegmentState::SeekCommand => {
                if value.starts_with(b"-") {
                    if let Some(wrapper_kind) = wrapper {
                        skip_next_value = wrapper_option_takes_value(wrapper_kind, value);
                    }
                    continue;
                }

                if is_shell_assignment(value) {
                    continue;
                }

                match classify_token(value) {
                    TokenKind::Wrapper => {
                        wrapper = wrapper_kind(value);
                    }
                    TokenKind::Rg => return None,
                    TokenKind::GrepLike => return Some(build_grep_decision(tokens, index)),
                    TokenKind::Other => return None,
                }
            }
        }
    }

    None
}

fn build_grep_decision(tokens: &[ParsedToken], command_index: usize) -> BlockDecision {
    let suggestion = rewrite_grep_to_rg(tokens, command_index);
    BlockDecision::new(format_exact_suggestion(GREP_MESSAGE, &suggestion))
}

/// Rewrite a grep/egrep/fgrep command to the equivalent rg command.
///
/// - Replaces `grep`/`egrep`/`fgrep` with `rg`
/// - Adds `-F` for `fgrep` or `grep -F`/`grep --fixed-strings`
/// - Drops flags that are default in rg: `-r`, `-n`, `-E`, `--recursive`,
///   `--line-number`, `--extended-regexp`, `--color=auto`, `--color=always`
/// - Strips redundant chars from combined short flags (e.g. `-rni` -> `-i`)
fn rewrite_grep_to_rg(tokens: &[ParsedToken], command_index: usize) -> String {
    let cmd_name = normalized_program_name(tokens[command_index].value.as_bytes());
    let is_fgrep = cmd_name == b"fgrep";
    let is_egrep = cmd_name == b"egrep";

    let mut parts: Vec<String> = Vec::with_capacity(tokens.len() + 2);

    // Preserve wrapper tokens before the command.
    parts.extend(
        tokens[..command_index]
            .iter()
            .map(|token| token.raw.clone()),
    );

    parts.push("rg".to_string());

    if is_fgrep {
        parts.push("-F".to_string());
    }

    let mut need_fixed_strings = false;
    let rest = &tokens[command_index + 1..];

    for token in rest {
        let val = token.value.as_str();

        // Drop long flags that are redundant in rg.
        if matches!(
            val,
            "--recursive"
                | "--line-number"
                | "--extended-regexp"
                | "--color=auto"
                | "--color=always"
                | "--colour=auto"
                | "--colour=always"
        ) {
            continue;
        }

        if val == "--fixed-strings" {
            need_fixed_strings = true;
            continue;
        }

        // Drop single-char flags that are redundant in rg, handling combined flags.
        if val.starts_with('-') && !val.starts_with("--") && val.len() > 1 {
            let cleaned = strip_redundant_short_flags(val, is_egrep);
            match cleaned {
                FlagResult::Drop => continue,
                FlagResult::NeedFixedStrings(remaining) => {
                    need_fixed_strings = true;
                    if let Some(flags) = remaining {
                        parts.push(flags);
                    }
                    continue;
                }
                FlagResult::Keep(flags) => {
                    parts.push(flags);
                    continue;
                }
            }
        }

        parts.push(token.raw.clone());
    }

    // Insert -F right after rg if needed and not already present from fgrep.
    if need_fixed_strings && !is_fgrep {
        let rg_pos = command_index; // rg is at this position in parts after wrappers
        parts.insert(rg_pos + 1, "-F".to_string());
    }

    parts.join(" ")
}

enum FlagResult {
    /// Drop the entire flag token.
    Drop,
    /// Keep the flag (possibly cleaned).
    Keep(String),
    /// The flag contained -F; optionally keep remaining flags.
    NeedFixedStrings(Option<String>),
}

/// Strip redundant short flag characters (r, n, E) from a combined flag like `-rnli`.
/// Returns the cleaned flag or signals it should be dropped entirely.
fn strip_redundant_short_flags(flag: &str, _is_egrep: bool) -> FlagResult {
    let chars: Vec<char> = flag[1..].chars().collect();
    let mut kept = Vec::new();
    let mut had_fixed = false;

    for &ch in &chars {
        match ch {
            'r' | 'n' | 'E' => {} // redundant in rg, drop
            'F' => had_fixed = true,
            _ => kept.push(ch),
        }
    }

    if had_fixed {
        let remaining = if kept.is_empty() {
            None
        } else {
            let mut s = String::with_capacity(kept.len() + 1);
            s.push('-');
            s.extend(kept);
            Some(s)
        };
        return FlagResult::NeedFixedStrings(remaining);
    }

    if kept.is_empty() {
        return FlagResult::Drop;
    }

    let mut s = String::with_capacity(kept.len() + 1);
    s.push('-');
    s.extend(kept);
    FlagResult::Keep(s)
}

fn format_exact_suggestion(base: &str, suggestion: &str) -> String {
    format!("{base}\nSuggested replacement:\n  {suggestion}")
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WrapperKind {
    Sudo,
    Env,
    Command,
    Time,
    Nohup,
    Builtin,
}

fn wrapper_kind(token: &[u8]) -> Option<WrapperKind> {
    match normalized_program_name(token) {
        b"sudo" => Some(WrapperKind::Sudo),
        b"env" => Some(WrapperKind::Env),
        b"command" => Some(WrapperKind::Command),
        b"time" => Some(WrapperKind::Time),
        b"nohup" => Some(WrapperKind::Nohup),
        b"builtin" => Some(WrapperKind::Builtin),
        _ => None,
    }
}

fn wrapper_option_takes_value(kind: WrapperKind, token: &[u8]) -> bool {
    match kind {
        WrapperKind::Sudo => matches!(
            token,
            b"-u"
                | b"--user"
                | b"-g"
                | b"--group"
                | b"-h"
                | b"--host"
                | b"-p"
                | b"--prompt"
                | b"-R"
                | b"--chroot"
                | b"-D"
                | b"--chdir"
                | b"-C"
                | b"--close-from"
                | b"-T"
                | b"--command-timeout"
        ),
        WrapperKind::Env => matches!(
            token,
            b"-u" | b"--unset" | b"-C" | b"--chdir" | b"-S" | b"--split-string" | b"--argv0"
        ),
        WrapperKind::Command | WrapperKind::Time | WrapperKind::Nohup | WrapperKind::Builtin => {
            false
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TokenKind {
    Wrapper,
    Rg,
    GrepLike,
    Other,
}

fn classify_token(token: &[u8]) -> TokenKind {
    let name = normalized_program_name(token);
    match name {
        b"rg" | b"ripgrep" => TokenKind::Rg,
        b"sudo" | b"env" | b"command" | b"nohup" | b"time" | b"builtin" => TokenKind::Wrapper,
        _ if is_grep_name(name) => TokenKind::GrepLike,
        _ => TokenKind::Other,
    }
}

fn normalized_program_name(token: &[u8]) -> &[u8] {
    let mut start = 0usize;
    for (index, byte) in token.iter().enumerate() {
        if *byte == b'/' || *byte == b'\\' {
            start = index + 1;
        }
    }

    let name = &token[start..];
    strip_exe_suffix(name)
}

fn strip_exe_suffix(token: &[u8]) -> &[u8] {
    if token.len() > 4 && token[token.len() - 4..].eq_ignore_ascii_case(b".exe") {
        &token[..token.len() - 4]
    } else {
        token
    }
}

fn is_grep_name(name: &[u8]) -> bool {
    matches!(name, b"grep" | b"egrep" | b"fgrep")
}

fn is_shell_assignment(token: &[u8]) -> bool {
    let Some(index) = token.iter().position(|byte| *byte == b'=') else {
        return false;
    };
    let head = &token[..index];

    if head.is_empty() {
        return false;
    }

    if !(head[0].is_ascii_alphabetic() || head[0] == b'_') {
        return false;
    }

    head[1..]
        .iter()
        .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
}

fn extract_claude_command(input: &str) -> Result<String, String> {
    let mut parser = JsonParser::new(input);
    let command = parser
        .parse_root_for_tool_input_command()?
        .ok_or_else(|| "stdin JSON did not contain tool_input.command".to_string())?;
    parser.skip_whitespace();
    if !parser.is_eof() {
        return Err("unexpected trailing JSON input".to_string());
    }
    Ok(command)
}

struct JsonParser<'a> {
    bytes: &'a [u8],
    index: usize,
}

impl<'a> JsonParser<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            bytes: input.as_bytes(),
            index: 0,
        }
    }

    fn parse_root_for_tool_input_command(&mut self) -> Result<Option<String>, String> {
        self.skip_whitespace();
        self.expect_byte(b'{')?;

        loop {
            self.skip_whitespace();
            if self.consume_byte(b'}') {
                return Ok(None);
            }

            let key = self.parse_string()?;
            self.skip_whitespace();
            self.expect_byte(b':')?;

            if key == "tool_input" {
                let command = self.parse_tool_input_object()?;
                self.skip_object_tail()?;
                return Ok(command);
            }

            self.skip_value()?;
            self.skip_whitespace();
            if self.consume_byte(b',') {
                continue;
            }
            if self.consume_byte(b'}') {
                return Ok(None);
            }
            return Err("expected ',' or '}' in root object".to_string());
        }
    }

    fn parse_tool_input_object(&mut self) -> Result<Option<String>, String> {
        self.skip_whitespace();
        self.expect_byte(b'{')?;

        loop {
            self.skip_whitespace();
            if self.consume_byte(b'}') {
                return Ok(None);
            }

            let key = self.parse_string()?;
            self.skip_whitespace();
            self.expect_byte(b':')?;

            if key == "command" {
                let command = self.parse_string()?;
                self.skip_object_tail()?;
                return Ok(Some(command));
            }

            self.skip_value()?;
            self.skip_whitespace();
            if self.consume_byte(b',') {
                continue;
            }
            if self.consume_byte(b'}') {
                return Ok(None);
            }
            return Err("expected ',' or '}' in tool_input object".to_string());
        }
    }

    fn skip_object_tail(&mut self) -> Result<(), String> {
        self.skip_whitespace();
        while !self.consume_byte(b'}') {
            self.expect_byte(b',')?;
            self.skip_whitespace();
            let _ = self.parse_string()?;
            self.skip_whitespace();
            self.expect_byte(b':')?;
            self.skip_value()?;
            self.skip_whitespace();
        }
        Ok(())
    }

    fn skip_value(&mut self) -> Result<(), String> {
        self.skip_whitespace();
        match self.peek_byte() {
            Some(b'{') => self.skip_object(),
            Some(b'[') => self.skip_array(),
            Some(b'"') => {
                let _ = self.parse_string()?;
                Ok(())
            }
            Some(b'-' | b'0'..=b'9') => self.skip_number(),
            Some(b't') => self.expect_bytes(b"true"),
            Some(b'f') => self.expect_bytes(b"false"),
            Some(b'n') => self.expect_bytes(b"null"),
            _ => Err("unexpected JSON value".to_string()),
        }
    }

    fn skip_object(&mut self) -> Result<(), String> {
        self.expect_byte(b'{')?;
        loop {
            self.skip_whitespace();
            if self.consume_byte(b'}') {
                return Ok(());
            }
            let _ = self.parse_string()?;
            self.skip_whitespace();
            self.expect_byte(b':')?;
            self.skip_value()?;
            self.skip_whitespace();
            if self.consume_byte(b',') {
                continue;
            }
            if self.consume_byte(b'}') {
                return Ok(());
            }
            return Err("expected ',' or '}' in object".to_string());
        }
    }

    fn skip_array(&mut self) -> Result<(), String> {
        self.expect_byte(b'[')?;
        loop {
            self.skip_whitespace();
            if self.consume_byte(b']') {
                return Ok(());
            }
            self.skip_value()?;
            self.skip_whitespace();
            if self.consume_byte(b',') {
                continue;
            }
            if self.consume_byte(b']') {
                return Ok(());
            }
            return Err("expected ',' or ']' in array".to_string());
        }
    }

    fn skip_number(&mut self) -> Result<(), String> {
        if self.consume_byte(b'-') {}

        match self.peek_byte() {
            Some(b'0') => {
                self.index += 1;
            }
            Some(b'1'..=b'9') => {
                self.index += 1;
                while matches!(self.peek_byte(), Some(b'0'..=b'9')) {
                    self.index += 1;
                }
            }
            _ => return Err("invalid number".to_string()),
        }

        if self.consume_byte(b'.') {
            if !matches!(self.peek_byte(), Some(b'0'..=b'9')) {
                return Err("invalid fractional number".to_string());
            }
            while matches!(self.peek_byte(), Some(b'0'..=b'9')) {
                self.index += 1;
            }
        }

        if matches!(self.peek_byte(), Some(b'e' | b'E')) {
            self.index += 1;
            if matches!(self.peek_byte(), Some(b'+' | b'-')) {
                self.index += 1;
            }
            if !matches!(self.peek_byte(), Some(b'0'..=b'9')) {
                return Err("invalid exponent".to_string());
            }
            while matches!(self.peek_byte(), Some(b'0'..=b'9')) {
                self.index += 1;
            }
        }

        Ok(())
    }

    fn parse_string(&mut self) -> Result<String, String> {
        self.expect_byte(b'"')?;
        let mut output = String::new();

        loop {
            let byte = self
                .next_byte()
                .ok_or_else(|| "unterminated JSON string".to_string())?;

            match byte {
                b'"' => return Ok(output),
                b'\\' => {
                    let escaped = self
                        .next_byte()
                        .ok_or_else(|| "unterminated JSON escape".to_string())?;
                    match escaped {
                        b'"' => output.push('"'),
                        b'\\' => output.push('\\'),
                        b'/' => output.push('/'),
                        b'b' => output.push('\u{0008}'),
                        b'f' => output.push('\u{000C}'),
                        b'n' => output.push('\n'),
                        b'r' => output.push('\r'),
                        b't' => output.push('\t'),
                        b'u' => output.push(self.parse_unicode_escape()?),
                        _ => return Err("invalid JSON escape".to_string()),
                    }
                }
                byte if byte < 0x20 => return Err("control character in JSON string".to_string()),
                _ => output.push(byte as char),
            }
        }
    }

    fn parse_unicode_escape(&mut self) -> Result<char, String> {
        let first = self.parse_hex_u16()?;
        if !(0xD800..=0xDBFF).contains(&first) {
            return char::from_u32(first as u32)
                .ok_or_else(|| "invalid unicode scalar".to_string());
        }

        self.expect_byte(b'\\')?;
        self.expect_byte(b'u')?;
        let second = self.parse_hex_u16()?;
        if !(0xDC00..=0xDFFF).contains(&second) {
            return Err("invalid unicode surrogate pair".to_string());
        }

        let scalar = 0x10000 + (((first as u32 - 0xD800) << 10) | (second as u32 - 0xDC00));
        char::from_u32(scalar).ok_or_else(|| "invalid unicode scalar".to_string())
    }

    fn parse_hex_u16(&mut self) -> Result<u16, String> {
        let mut value = 0u16;
        for _ in 0..4 {
            let digit = self
                .next_byte()
                .ok_or_else(|| "incomplete unicode escape".to_string())?;
            value = (value << 4)
                | match digit {
                    b'0'..=b'9' => (digit - b'0') as u16,
                    b'a'..=b'f' => (digit - b'a' + 10) as u16,
                    b'A'..=b'F' => (digit - b'A' + 10) as u16,
                    _ => return Err("invalid unicode escape".to_string()),
                };
        }
        Ok(value)
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.peek_byte(), Some(b' ' | b'\n' | b'\r' | b'\t')) {
            self.index += 1;
        }
    }

    fn consume_byte(&mut self, expected: u8) -> bool {
        if self.peek_byte() == Some(expected) {
            self.index += 1;
            true
        } else {
            false
        }
    }

    fn expect_byte(&mut self, expected: u8) -> Result<(), String> {
        match self.next_byte() {
            Some(actual) if actual == expected => Ok(()),
            Some(actual) => Err(format!(
                "expected '{}', found '{}'",
                expected as char, actual as char
            )),
            None => Err(format!(
                "expected '{}', found end of input",
                expected as char
            )),
        }
    }

    fn expect_bytes(&mut self, expected: &[u8]) -> Result<(), String> {
        for byte in expected {
            self.expect_byte(*byte)?;
        }
        Ok(())
    }

    fn next_byte(&mut self) -> Option<u8> {
        let byte = self.peek_byte()?;
        self.index += 1;
        Some(byte)
    }

    fn peek_byte(&self) -> Option<u8> {
        self.bytes.get(self.index).copied()
    }

    fn is_eof(&self) -> bool {
        self.index >= self.bytes.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decision_message(command: &str) -> String {
        evaluate_command(command).unwrap().message
    }

    #[test]
    fn blocks_grep() {
        let message = decision_message("grep -rn pattern .");
        assert!(message.contains(GREP_MESSAGE));
        assert!(message.contains("rg pattern ."));
    }

    #[test]
    fn blocks_egrep() {
        let message = decision_message("egrep 'foo|bar' file.txt");
        assert!(message.contains(GREP_MESSAGE));
        assert!(message.contains("rg 'foo|bar' file.txt"));
    }

    #[test]
    fn blocks_fgrep() {
        let message = decision_message("fgrep 'literal.string' file.txt");
        assert!(message.contains(GREP_MESSAGE));
        assert!(message.contains("rg -F 'literal.string' file.txt"));
    }

    #[test]
    fn suggests_exact_rg_rewrites() {
        // Simple grep -> rg
        let message = decision_message("grep pattern file.txt");
        assert!(message.contains("rg pattern file.txt"));

        // Strips redundant -r -n flags
        let message = decision_message("grep -rn pattern .");
        assert!(message.contains("rg pattern ."));

        // Strips redundant -E flag
        let message = decision_message("grep -E 'foo|bar' .");
        assert!(message.contains("rg 'foo|bar' ."));

        // Keeps meaningful flags
        let message = decision_message("grep -i pattern file.txt");
        assert!(message.contains("rg -i pattern file.txt"));

        // Combined flags: strips r and n, keeps i
        let message = decision_message("grep -rni pattern .");
        assert!(message.contains("rg -i pattern ."));

        // Keeps -l flag
        let message = decision_message("grep -rl pattern .");
        assert!(message.contains("rg -l pattern ."));

        // Keeps context flags
        let message = decision_message("grep -A 3 pattern file.txt");
        assert!(message.contains("rg -A 3 pattern file.txt"));
    }

    #[test]
    fn strips_redundant_long_flags() {
        let message = decision_message("grep --recursive --line-number pattern .");
        assert!(message.contains("rg pattern ."));

        let message = decision_message("grep --extended-regexp 'foo|bar' .");
        assert!(message.contains("rg 'foo|bar' ."));

        let message = decision_message("grep --color=auto pattern file.txt");
        assert!(message.contains("rg pattern file.txt"));
    }

    #[test]
    fn converts_fixed_strings_flag() {
        let message = decision_message("grep -F 'literal' file.txt");
        assert!(message.contains("rg -F 'literal' file.txt"));

        let message = decision_message("grep --fixed-strings 'literal' file.txt");
        assert!(message.contains("rg -F 'literal' file.txt"));
    }

    #[test]
    fn allows_rg_usage() {
        assert_eq!(evaluate_command("rg pattern ."), None);
        assert_eq!(evaluate_command("rg -i pattern file.txt"), None);
        assert_eq!(evaluate_command("sudo rg pattern /var/log"), None);
    }

    #[test]
    fn allows_non_grep_commands() {
        assert_eq!(evaluate_command("echo grep"), None);
        assert_eq!(evaluate_command("ls -la"), None);
        assert_eq!(evaluate_command("cat file.txt"), None);
        assert_eq!(evaluate_command("find . -name '*.rs'"), None);
    }

    #[test]
    fn handles_wrappers() {
        let message = decision_message("sudo grep -rn pattern /var/log");
        assert!(message.contains("sudo rg pattern /var/log"));

        let message = decision_message("sudo -u root grep pattern /etc/hosts");
        assert!(message.contains("sudo -u root rg pattern /etc/hosts"));

        let message = decision_message("env FOO=1 grep pattern file.txt");
        assert!(message.contains("env FOO=1 rg pattern file.txt"));
    }

    #[test]
    fn handles_pipes() {
        // grep after a pipe should still be blocked
        let result = evaluate_command("cat file.txt | grep pattern");
        assert!(result.is_some());
        assert!(result.unwrap().message.contains("rg pattern"));
    }

    #[test]
    fn handles_chained_commands() {
        // grep in a chained command should be blocked
        let result = evaluate_command("cd /tmp && grep -rn TODO .");
        assert!(result.is_some());
        assert!(result.unwrap().message.contains("rg TODO ."));
    }

    #[test]
    fn handles_full_path_grep() {
        let message = decision_message("/usr/bin/grep pattern file.txt");
        assert!(message.contains("rg pattern file.txt"));

        let message = decision_message("/bin/grep -rn pattern .");
        assert!(message.contains("rg pattern ."));
    }

    #[test]
    fn handles_shell_assignments() {
        let message = decision_message("FOO=bar grep pattern file.txt");
        assert!(message.contains("FOO=bar rg pattern file.txt"));
    }

    #[test]
    fn parses_claude_hook_json() {
        let input =
            r#"{"tool_name":"Bash","tool_input":{"command":"grep -rn pattern .","cwd":"/tmp"}}"#;
        assert_eq!(
            extract_claude_command(input).unwrap(),
            "grep -rn pattern .".to_string()
        );
    }

    #[test]
    fn parses_escaped_json_command() {
        let input = r#"{"tool_input":{"command":"grep -rn \"pattern\" .","cwd":"/tmp"}}"#;
        assert_eq!(
            extract_claude_command(input).unwrap(),
            "grep -rn \"pattern\" .".to_string()
        );
    }
}
