use std::{
    cmp::Reverse,
    collections::{BTreeSet, HashSet},
    sync::OnceLock,
};

use regex::Regex;

use crate::{DiagnosticAnchor, DiagnosticChannel, ExecutionObservation};

/// Version of the diagnostic normalization contract included in fingerprints.
pub const NORMALIZATION_SCHEMA: u16 = 4;
const MAX_ANCHORS: usize = 4;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
enum DiscriminatorKind {
    FailingTest,
    CompilerDiagnostic,
    RootFailure,
    Assertion,
    Message,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct EligibleLine {
    channel: DiagnosticChannel,
    text: String,
    kind: DiscriminatorKind,
    score: usize,
    position: usize,
}

pub(crate) fn stable_discriminators(
    channel: DiagnosticChannel,
    baselines: &[ExecutionObservation],
) -> Vec<DiagnosticAnchor> {
    let stdout = stream_discriminators(channel, DiagnosticChannel::Stdout, baselines);
    let stderr = stream_discriminators(channel, DiagnosticChannel::Stderr, baselines);
    match channel {
        DiagnosticChannel::Auto => select_auto_anchors(stdout, stderr),
        DiagnosticChannel::Stdout => select_anchors(stdout, true),
        DiagnosticChannel::Stderr => select_anchors(stderr, true),
        DiagnosticChannel::Combined => {
            if stdout.is_empty() || stderr.is_empty() {
                Vec::new()
            } else {
                select_combined_anchors(stdout, stderr)
            }
        }
    }
}

fn stream_discriminators(
    requested: DiagnosticChannel,
    stream: DiagnosticChannel,
    baselines: &[ExecutionObservation],
) -> Vec<EligibleLine> {
    if !matches!(
        requested,
        DiagnosticChannel::Auto | DiagnosticChannel::Combined
    ) && requested != stream
    {
        return Vec::new();
    }
    let first = eligible_lines(
        stream,
        &normalize_bytes(selected_bytes(&baselines[0], stream)),
    );
    if first.is_empty() {
        return Vec::new();
    }
    let intersections = baselines
        .iter()
        .skip(1)
        .map(|observation| {
            eligible_lines(
                stream,
                &normalize_bytes(selected_bytes(observation, stream)),
            )
            .into_iter()
            .map(|line| line.text)
            .collect::<HashSet<_>>()
        })
        .collect::<Vec<_>>();
    first
        .into_iter()
        .filter(|line| intersections.iter().all(|lines| lines.contains(&line.text)))
        .collect()
}

fn selected_bytes(observation: &ExecutionObservation, stream: DiagnosticChannel) -> &[u8] {
    match stream {
        DiagnosticChannel::Stdout => observation.stdout(),
        DiagnosticChannel::Stderr => observation.stderr(),
        DiagnosticChannel::Auto | DiagnosticChannel::Combined => &[],
    }
}

fn select_anchors(
    lines: impl IntoIterator<Item = EligibleLine>,
    allow_generic: bool,
) -> Vec<DiagnosticAnchor> {
    let lines = rank_lines(lines, allow_generic);
    let mut selected = Vec::with_capacity(MAX_ANCHORS);
    fill_categories(&mut selected, &lines);
    fill_ranked(&mut selected, lines);
    into_anchors(selected)
}

fn select_auto_anchors(
    stdout: Vec<EligibleLine>,
    stderr: Vec<EligibleLine>,
) -> Vec<DiagnosticAnchor> {
    let stdout = rank_lines(stdout, false);
    let stderr = rank_lines(stderr, false);
    let mut selected = Vec::with_capacity(MAX_ANCHORS);
    if let Some(line) = stdout.first() {
        selected.push(line.clone());
    }
    if let Some(line) = stderr.first() {
        selected.push(line.clone());
    }
    let globally_ranked = rank_lines(stdout.into_iter().chain(stderr), false);
    fill_categories(&mut selected, &globally_ranked);
    fill_ranked(&mut selected, globally_ranked);
    into_anchors(selected)
}

fn select_combined_anchors(
    stdout: Vec<EligibleLine>,
    stderr: Vec<EligibleLine>,
) -> Vec<DiagnosticAnchor> {
    let stdout = rank_lines(stdout, true);
    let stderr = rank_lines(stderr, true);
    let mut selected = Vec::with_capacity(MAX_ANCHORS);
    selected.push(stdout[0].clone());
    selected.push(stderr[0].clone());
    let globally_ranked = rank_lines(stdout.into_iter().chain(stderr), true);
    fill_ranked(&mut selected, globally_ranked);
    into_anchors(selected)
}

fn rank_lines(
    lines: impl IntoIterator<Item = EligibleLine>,
    allow_generic: bool,
) -> Vec<EligibleLine> {
    let mut lines = lines
        .into_iter()
        .filter(|line| allow_generic || line.kind != DiscriminatorKind::Message)
        .collect::<Vec<_>>();
    lines.sort_unstable_by(|left, right| {
        (
            left.kind,
            Reverse(left.score),
            left.position,
            channel_order(left.channel),
            &left.text,
        )
            .cmp(&(
                right.kind,
                Reverse(right.score),
                right.position,
                channel_order(right.channel),
                &right.text,
            ))
    });
    lines
}

fn channel_order(channel: DiagnosticChannel) -> u8 {
    match channel {
        DiagnosticChannel::Stdout => 0,
        DiagnosticChannel::Stderr => 1,
        DiagnosticChannel::Auto => 2,
        DiagnosticChannel::Combined => 3,
    }
}

fn fill_categories(selected: &mut Vec<EligibleLine>, lines: &[EligibleLine]) {
    if selected.len() >= MAX_ANCHORS {
        return;
    }
    let mut categories = selected
        .iter()
        .map(|line| line.kind)
        .collect::<BTreeSet<_>>();
    for line in lines {
        if categories.insert(line.kind)
            && !selected
                .iter()
                .any(|item| item.channel == line.channel && item.text == line.text)
        {
            selected.push(line.clone());
            if selected.len() >= MAX_ANCHORS {
                break;
            }
        }
    }
}

fn fill_ranked(selected: &mut Vec<EligibleLine>, lines: Vec<EligibleLine>) {
    if selected.len() >= MAX_ANCHORS {
        return;
    }
    for line in lines {
        if selected
            .iter()
            .any(|item| item.channel == line.channel && item.text == line.text)
        {
            continue;
        }
        selected.push(line);
        if selected.len() >= MAX_ANCHORS {
            break;
        }
    }
}

fn into_anchors(selected: Vec<EligibleLine>) -> Vec<DiagnosticAnchor> {
    selected
        .into_iter()
        .map(|line| DiagnosticAnchor::new(line.channel, line.text))
        .collect()
}

fn eligible_lines(channel: DiagnosticChannel, diagnostic: &str) -> Vec<EligibleLine> {
    diagnostic
        .lines()
        .enumerate()
        .filter_map(|(position, text)| {
            discriminator_kind(text).map(|kind| EligibleLine {
                channel,
                text: text.to_owned(),
                kind,
                score: discriminator_score(text),
                position,
            })
        })
        .collect()
}

fn discriminator_kind(line: &str) -> Option<DiscriminatorKind> {
    static PYTEST: OnceLock<Regex> = OnceLock::new();
    static COMPILER: OnceLock<Regex> = OnceLock::new();
    static ROOT: OnceLock<Regex> = OnceLock::new();
    static ASSERTION: OnceLock<Regex> = OnceLock::new();
    static MESSAGE: OnceLock<Regex> = OnceLock::new();
    if is_boilerplate(line) {
        return None;
    }
    let pytest = PYTEST.get_or_init(|| {
        Regex::new(r"^(?:failed|error)[ \t]+[^ \t\r\n]+(?:::[^ \t\r\n]+)+")
            .expect("pytest discriminator regex is valid")
    });
    let lowercase = line.to_ascii_lowercase();
    if pytest.is_match(&lowercase) {
        return Some(DiscriminatorKind::FailingTest);
    }
    let compiler = COMPILER.get_or_init(|| {
        Regex::new(r"(?:error\[[a-z][0-9]{2,}\]|(?:fatal )?error[ \t]+[a-z]{1,5}[0-9]{2,})")
            .expect("compiler discriminator regex is valid")
    });
    if compiler.is_match(&lowercase) {
        return Some(DiscriminatorKind::CompilerDiagnostic);
    }
    let root = ROOT.get_or_init(|| {
        Regex::new(r"(?:[a-z_][a-z0-9_.]*(?:error|exception)|panicked at|^panic:|^fatal:)")
            .expect("root failure regex is valid")
    });
    if root.is_match(&lowercase) {
        return Some(DiscriminatorKind::RootFailure);
    }
    let assertion = ASSERTION.get_or_init(|| {
        Regex::new(r"(?:assert(?:ion)?|expected|actual|left.*right)")
            .expect("assertion discriminator regex is valid")
    });
    if assertion.is_match(&lowercase) {
        return Some(DiscriminatorKind::Assertion);
    }
    let message = MESSAGE.get_or_init(|| {
        Regex::new(r"(?:error|failed|failure|panic|exception|fatal)")
            .expect("generic failure regex is valid")
    });
    message
        .is_match(&lowercase)
        .then_some(DiscriminatorKind::Message)
}

fn discriminator_score(line: &str) -> usize {
    let distinct = line
        .split(|character: char| !character.is_alphanumeric() && character != '_')
        .filter(|token| token.chars().any(char::is_alphabetic))
        .map(str::to_ascii_lowercase)
        .collect::<BTreeSet<_>>()
        .len();
    distinct.saturating_mul(16).saturating_add(
        line.chars()
            .filter(|character| character.is_alphabetic())
            .count(),
    )
}

fn is_boilerplate(line: &str) -> bool {
    static SUMMARY: OnceLock<Regex> = OnceLock::new();
    static LOCATION: OnceLock<Regex> = OnceLock::new();
    static LIFECYCLE: OnceLock<Regex> = OnceLock::new();
    let trimmed = line.trim();
    if trimmed.is_empty() || !trimmed.chars().any(char::is_alphanumeric) {
        return true;
    }
    let lowercase = trimmed.to_ascii_lowercase();
    if matches!(
        lowercase.as_str(),
        "traceback (most recent call last):"
            | "stack backtrace:"
            | "backtrace:"
            | "short test summary info"
            | "failures"
    ) {
        return true;
    }
    let summary = SUMMARY.get_or_init(|| {
        Regex::new(r"^[^A-Za-z0-9_]*(?:[0-9]+[ \t]+(?:failed|passed|skipped|error)s?)(?:[^A-Za-z0-9_]|$).*(?:(?:ms|s|sec|seconds?))?[^A-Za-z0-9_]*$")
            .expect("summary regex is valid")
    });
    if summary.is_match(&lowercase) {
        return true;
    }
    let location = LOCATION.get_or_init(|| {
        Regex::new(r#"^(?:at[ \t]+[^ \t\r\n]+|file[ \t]+\"[^\"]+\",[ \t]+line[ \t]+<location>|[ \t]*-->[ \t]+[^ \t\r\n]+)(?::?[0-9<>]+)*[ \t]*$"#)
            .expect("location regex is valid")
    });
    if location.is_match(&lowercase) {
        return true;
    }
    let lifecycle = LIFECYCLE.get_or_init(|| {
        Regex::new(r"^(?:process|command|child)[ \t]+(?:exited|failed)[ \t]+with[ \t]+(?:code|status)[ \t]+[-0-9]+[^A-Za-z0-9_]*$")
            .expect("lifecycle regex is valid")
    });
    lifecycle.is_match(&lowercase)
}

/// Removes only context-qualified volatile fragments from diagnostic text.
pub fn normalize_diagnostic(input: &str) -> String {
    static UUID: OnceLock<Regex> = OnceLock::new();
    static ISO_TIMESTAMP: OnceLock<Regex> = OnceLock::new();
    static UNIX_TEMP: OnceLock<Regex> = OnceLock::new();
    static WINDOWS_TEMP: OnceLock<Regex> = OnceLock::new();
    static ADDRESS: OnceLock<Regex> = OnceLock::new();
    static PROCESS_ID: OnceLock<Regex> = OnceLock::new();
    static LOOPBACK_PORT: OnceLock<Regex> = OnceLock::new();
    static NAMED_PORT: OnceLock<Regex> = OnceLock::new();
    static DURATION: OnceLock<Regex> = OnceLock::new();
    static PATH_LOCATION: OnceLock<Regex> = OnceLock::new();
    static NAMED_LOCATION: OnceLock<Regex> = OnceLock::new();
    static HORIZONTAL_SPACE: OnceLock<Regex> = OnceLock::new();

    let uuid = UUID.get_or_init(|| {
        Regex::new(r"[0-9A-Fa-f]{8}-[0-9A-Fa-f]{4}-[1-5][0-9A-Fa-f]{3}-[89ABab][0-9A-Fa-f]{3}-[0-9A-Fa-f]{12}")
            .expect("UUID regex is valid")
    });
    let timestamp = ISO_TIMESTAMP.get_or_init(|| {
        Regex::new(r"[0-9]{4}-[0-9]{2}-[0-9]{2}[T ][0-9]{2}:[0-9]{2}:[0-9]{2}(?:\.[0-9]+)?(?:Z|[+-][0-9]{2}:[0-9]{2})?")
            .expect("timestamp regex is valid")
    });
    let unix_temp = UNIX_TEMP.get_or_init(|| {
        Regex::new(r"/(?:tmp|var/tmp)(?:/[^ \t\r\n:]+)*")
            .expect("Unix temporary path regex is valid")
    });
    let windows_temp = WINDOWS_TEMP.get_or_init(|| {
        Regex::new(r"[A-Za-z]:\\(?:[Tt][Mm][Pp]|[Tt][Ee][Mm][Pp]|[Uu]sers\\[^\\ \t\r\n:]+\\[Aa]pp[Dd]ata\\[Ll]ocal\\[Tt]emp)(?:\\[^ \t\r\n:]+)*")
            .expect("Windows temporary path regex is valid")
    });
    let address = ADDRESS.get_or_init(|| {
        Regex::new(
            r"(?:address|addr|pointer|ptr|Address|Pointer)[ \t]*[:=]?[ \t]*0x[0-9A-Fa-f]{7,}",
        )
        .expect("contextual address regex is valid")
    });
    let process_id = PROCESS_ID.get_or_init(|| {
        Regex::new(r"(pid|PID|process[ \t]+[Ii][Dd]|thread[ \t]+[Ii][Dd]|thread|Thread)[ \t]*[:=#]?[ \t]*[0-9]+")
            .expect("process identifier regex is valid")
    });
    let loopback_port = LOOPBACK_PORT.get_or_init(|| {
        Regex::new(r"(localhost|LOCALHOST|127\.0\.0\.1|\[::1\]):[0-9]{1,5}")
            .expect("loopback port regex is valid")
    });
    let named_port = NAMED_PORT.get_or_init(|| {
        Regex::new(r"(?:port|Port|PORT)[ \t]*[:=]?[ \t]*[0-9]{1,5}").expect("port regex is valid")
    });
    let duration = DURATION.get_or_init(|| {
        Regex::new(r"[0-9]+(?:\.[0-9]+)?[ \t]*(?:seconds|second|minutes|minute|secs|sec|mins|min|ms|ns|us|s)")
            .expect("duration regex is valid")
    });
    let path_location = PATH_LOCATION.get_or_init(|| {
        Regex::new(r"(?m)(?P<token>[^ \t\r\n:]+):[0-9]+(?::[0-9]+)?")
            .expect("path location candidate regex is valid")
    });
    let named_location = NAMED_LOCATION.get_or_init(|| {
        Regex::new(r"([Ll]ine|[Cc]olumn)[ \t]+[0-9]+").expect("named location regex is valid")
    });
    let horizontal_space = HORIZONTAL_SPACE
        .get_or_init(|| Regex::new(r"[\t ]+").expect("horizontal whitespace regex is valid"));

    let mut text = input.replace("\r\n", "\n").replace('\r', "\n");
    text = uuid.replace_all(&text, "<uuid>").into_owned();
    text = timestamp.replace_all(&text, "<timestamp>").into_owned();
    text = windows_temp.replace_all(&text, "<temp>").into_owned();
    text = unix_temp.replace_all(&text, "<temp>").into_owned();
    text = address.replace_all(&text, "address <address>").into_owned();
    text = replace_lexically_bounded(process_id, &text, "$1 <id>");
    text = loopback_port.replace_all(&text, "$1:<port>").into_owned();
    text = replace_lexically_bounded(named_port, &text, "port <port>");
    text = replace_lexically_bounded(duration, &text, "<duration>");
    text = path_location
        .replace_all(&text, |captures: &regex::Captures<'_>| {
            let token = &captures["token"];
            let start = captures.get(0).map_or(0, |matched| matched.start());
            if is_source_location_token(token, has_compiler_source_context(&text[..start])) {
                format!("{token}:<location>")
            } else {
                captures[0].to_owned()
            }
        })
        .into_owned();
    text = replace_lexically_bounded(named_location, &text, "$1 <location>");
    text.lines()
        .map(|line| horizontal_space.replace_all(line.trim(), " "))
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn replace_lexically_bounded(pattern: &Regex, input: &str, replacement: &str) -> String {
    pattern
        .replace_all(input, |captures: &regex::Captures<'_>| {
            let matched = captures.get(0).expect("the complete match always exists");
            if has_lexical_boundaries(input, matched.start(), matched.end()) {
                let mut expanded = String::new();
                captures.expand(replacement, &mut expanded);
                expanded
            } else {
                matched.as_str().to_owned()
            }
        })
        .into_owned()
}

fn has_lexical_boundaries(input: &str, start: usize, end: usize) -> bool {
    input[..start]
        .chars()
        .next_back()
        .is_none_or(|character| !is_lexical_character(character))
        && input[end..]
            .chars()
            .next()
            .is_none_or(|character| !is_lexical_character(character))
}

fn is_lexical_character(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

fn has_compiler_source_context(prefix: &str) -> bool {
    let line = prefix.rsplit('\n').next().unwrap_or(prefix);
    let trimmed = line.trim_end_matches([' ', '\t']);
    trimmed.ends_with("-->")
}

fn is_source_location_token(token: &str, compiler_context: bool) -> bool {
    const SOURCE_EXTENSIONS: &[&str] = &[
        "bash", "c", "cc", "cjs", "cpp", "cs", "cts", "cxx", "fish", "go", "h", "hh", "hpp", "hxx",
        "java", "js", "json", "jsx", "kt", "kts", "mjs", "mts", "php", "py", "pyi", "rb", "rs",
        "scala", "sh", "swift", "toml", "ts", "tsx", "yaml", "yml", "zsh",
    ];
    const EXTENSIONLESS_SOURCE_FILES: &[&str] = &["BUILD", "Dockerfile", "Makefile", "WORKSPACE"];
    let basename = token.rsplit(['/', '\\']).next().unwrap_or(token);
    token == "<temp>"
        || compiler_context
        || has_source_tree_root(token)
        || EXTENSIONLESS_SOURCE_FILES.contains(&basename)
        || token.rsplit_once('.').is_some_and(|(_, extension)| {
            SOURCE_EXTENSIONS
                .iter()
                .any(|known| extension.eq_ignore_ascii_case(known))
        })
}

fn has_source_tree_root(token: &str) -> bool {
    const SOURCE_TREE_ROOTS: &[&str] = &[
        "app", "apps", "benches", "crates", "examples", "lib", "packages", "src", "test", "tests",
    ];
    let relative = token
        .strip_prefix("./")
        .or_else(|| token.strip_prefix(".\\"))
        .unwrap_or(token);
    if relative.starts_with('/') || relative.starts_with('\\') {
        return false;
    }
    relative
        .split(['/', '\\'])
        .next()
        .is_some_and(|root| SOURCE_TREE_ROOTS.contains(&root))
}

pub(crate) fn normalize_bytes(bytes: &[u8]) -> String {
    normalize_diagnostic(&String::from_utf8_lossy(bytes))
}

#[cfg(test)]
mod tests {
    use super::super::DiagnosticChannel;
    use super::{rank_lines, DiscriminatorKind, EligibleLine};

    #[test]
    fn rank_lines_uses_stdout_before_stderr_for_an_exact_tie() {
        let line = |channel| EligibleLine {
            channel,
            text: "ValueError: shared failure".to_owned(),
            kind: DiscriminatorKind::RootFailure,
            score: 32,
            position: 0,
        };

        let ranked = rank_lines(
            [
                line(DiagnosticChannel::Stderr),
                line(DiagnosticChannel::Stdout),
            ],
            true,
        );

        assert_eq!(ranked[0].channel, DiagnosticChannel::Stdout);
        assert_eq!(ranked[1].channel, DiagnosticChannel::Stderr);
    }
}
