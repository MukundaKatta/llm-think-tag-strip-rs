/*!
llm-think-tag-strip: strip `<thinking>`/`<think>` reasoning blocks from LLM output.

Covers Claude extended thinking (`<thinking>`), DeepSeek-R1 (`<think>`),
and the bracketed-pipe form (`<|thinking|>`). Unclosed open tags are
treated as "from here to end of string is thinking."

```rust
use llm_think_tag_strip::strip_thinking;

let out = strip_thinking(
    "Let me think. <thinking>step 1</thinking>The answer is 42.",
    &["thinking", "think"],
    false,
);
assert_eq!(out.clean, "Let me think. The answer is 42.");
assert_eq!(out.thinking, vec!["step 1"]);
assert!(out.had_thinking);
```
*/

use regex::Regex;

/// Result of stripping thinking tags.
#[derive(Debug, Clone, PartialEq)]
pub struct StrippedResult {
    /// Text with all thinking blocks removed, whitespace collapsed.
    pub clean: String,
    /// Extracted thinking block bodies in source order.
    pub thinking: Vec<String>,
    /// `true` if at least one thinking block was found and removed.
    pub had_thinking: bool,
}

/// Default tag names: `thinking` (Claude) and `think` (DeepSeek-R1, others).
pub const DEFAULT_TAGS: &[&str] = &["thinking", "think"];

// ---- whitespace collapse --------------------------------------------------

fn collapse_whitespace(text: &str) -> String {
    // Collapse runs of 2+ horizontal whitespace to single space.
    let re_hspace = Regex::new(r"[ \t]{2,}").unwrap();
    let s = re_hspace.replace_all(text, " ");
    // Space before newline / after newline → clean newline.
    let re_nl1 = Regex::new(r"[ \t]+\n").unwrap();
    let s = re_nl1.replace_all(&s, "\n");
    let re_nl2 = Regex::new(r"\n[ \t]+").unwrap();
    let s = re_nl2.replace_all(&s, "\n");
    // 3+ consecutive newlines → 2.
    let re_nls = Regex::new(r"\n{3,}").unwrap();
    let s = re_nls.replace_all(&s, "\n\n");
    s.trim().to_owned()
}

// ---- per-tag patterns -----------------------------------------------------

fn angle_closed_re(tag: &str) -> Regex {
    let escaped = regex::escape(tag);
    Regex::new(&format!(r"(?is)<{escaped}\s*>(.*?)</{escaped}\s*>")).unwrap()
}

fn pipe_closed_re(tag: &str) -> Regex {
    let escaped = regex::escape(tag);
    Regex::new(&format!(r"(?is)<\|{escaped}\|>(.*?)</\|{escaped}\|>")).unwrap()
}

fn markdown_closed_re(tag: &str) -> Regex {
    let escaped = regex::escape(tag);
    Regex::new(&format!(
        r"(?is)#{{1,6}}\s*{escaped}\s*\n(.*?)\n#{{1,6}}\s*end\s+{escaped}\s*(?:\n|$)"
    ))
    .unwrap()
}

fn angle_unclosed_re(tag: &str) -> Regex {
    let escaped = regex::escape(tag);
    Regex::new(&format!(r"(?is)<{escaped}\s*>(.*)\z")).unwrap()
}

fn pipe_unclosed_re(tag: &str) -> Regex {
    let escaped = regex::escape(tag);
    Regex::new(&format!(r"(?is)<\|{escaped}\|>(.*)\z")).unwrap()
}

fn markdown_unclosed_re(tag: &str) -> Regex {
    let escaped = regex::escape(tag);
    Regex::new(&format!(r"(?is)#{{1,6}}\s*{escaped}\s*\n(.*)\z")).unwrap()
}

// ---- closed-match collection ----------------------------------------------

/// A located closed thinking block: byte range in the source plus its body.
struct ClosedMatch {
    start: usize,
    end: usize,
    body: String,
}

/// Push every non-overlapping match of `re` in `text` into `out`.
///
/// Group 0 spans the whole block (used to know what to delete) and group 1
/// captures the trimmed body that becomes an extracted thinking entry.
fn collect_closed(re: &Regex, text: &str, out: &mut Vec<ClosedMatch>) {
    for caps in re.captures_iter(text) {
        let whole = caps.get(0).unwrap();
        out.push(ClosedMatch {
            start: whole.start(),
            end: whole.end(),
            body: caps[1].trim().to_owned(),
        });
    }
}

// ---- Stripper -------------------------------------------------------------

/// Reusable stripper holding the configured tag list and syntax options.
///
/// Construct one with [`Stripper::new`] and call [`Stripper::strip`] for each
/// piece of text. The convenience functions [`strip_thinking`] and
/// [`extract_thinking`] build a `Stripper` on every call, so prefer reusing a
/// `Stripper` when processing many inputs with the same configuration.
pub struct Stripper {
    /// Tag names to recognize, e.g. `["thinking", "think"]`.
    tags: Vec<String>,
    /// When `true`, also strip `### Thinking ... ### End thinking` blocks.
    markdown_style: bool,
}

impl Stripper {
    /// Create a stripper for the given `tags`, optionally enabling the
    /// markdown-heading syntax (`### Thinking ... ### End thinking`).
    pub fn new(tags: &[&str], markdown_style: bool) -> Self {
        Self {
            tags: tags.iter().map(|s| s.to_string()).collect(),
            markdown_style,
        }
    }

    /// Strip every recognized thinking block from `text`.
    ///
    /// Closed blocks are removed in source order. A single trailing *unclosed*
    /// block (an opening tag with no matching close) is treated as "everything
    /// from the tag to the end of the string is thinking" and dropped.
    pub fn strip(&self, text: &str) -> StrippedResult {
        if text.is_empty() {
            return StrippedResult {
                clean: String::new(),
                thinking: vec![],
                had_thinking: false,
            };
        }

        let mut thinking: Vec<String> = Vec::new();

        // --- closed blocks ---
        //
        // Collect every closed-block match across all tags and all enabled
        // syntaxes, then process them strictly in source order. Iterating one
        // tag at a time (e.g. all `thinking` blocks, then all `think` blocks)
        // would reorder the extracted bodies when different tags interleave,
        // breaking the "source order" guarantee.
        let mut matches: Vec<ClosedMatch> = Vec::new();
        for tag in &self.tags {
            collect_closed(&angle_closed_re(tag), text, &mut matches);
            collect_closed(&pipe_closed_re(tag), text, &mut matches);
            if self.markdown_style {
                collect_closed(&markdown_closed_re(tag), text, &mut matches);
            }
        }
        // Sort by start offset; on ties prefer the longer span so an outer
        // block wins over a nested/overlapping inner one.
        matches.sort_by(|a, b| a.start.cmp(&b.start).then(b.end.cmp(&a.end)));

        // Rebuild the string, dropping non-overlapping matches in order.
        let mut out = String::with_capacity(text.len());
        let mut cursor = 0usize;
        for m in &matches {
            if m.start < cursor {
                // Overlaps a span we already removed; skip it.
                continue;
            }
            out.push_str(&text[cursor..m.start]);
            thinking.push(m.body.clone());
            cursor = m.end;
        }
        out.push_str(&text[cursor..]);

        // --- unclosed blocks (at most one, from first match to end of string) ---
        'unclosed: for tag in &self.tags {
            let re = angle_unclosed_re(tag);
            if let Some(caps) = re.captures(&out) {
                let start = caps.get(0).unwrap().start();
                thinking.push(caps[1].trim().to_owned());
                out.truncate(start);
                break 'unclosed;
            }
        }
        'unclosed_pipe: for tag in &self.tags {
            // Only attempt if no angle match was found (re-check presence).
            let re = pipe_unclosed_re(tag);
            if let Some(caps) = re.captures(&out) {
                let start = caps.get(0).unwrap().start();
                thinking.push(caps[1].trim().to_owned());
                out.truncate(start);
                break 'unclosed_pipe;
            }
        }
        if self.markdown_style {
            for tag in &self.tags {
                let re = markdown_unclosed_re(tag);
                if let Some(caps) = re.captures(&out) {
                    let start = caps.get(0).unwrap().start();
                    thinking.push(caps[1].trim().to_owned());
                    out.truncate(start);
                    break;
                }
            }
        }

        let had_thinking = !thinking.is_empty();
        let clean = collapse_whitespace(&out);
        StrippedResult {
            clean,
            thinking,
            had_thinking,
        }
    }
}

// ---- public functions -----------------------------------------------------

/// Strip thinking-tag blocks from `text`.
///
/// `tags` defaults to `["thinking", "think"]`. Pass `markdown_style=true`
/// to also strip `### Thinking ... ### End thinking` blocks.
pub fn strip_thinking(text: &str, tags: &[&str], markdown_style: bool) -> StrippedResult {
    Stripper::new(tags, markdown_style).strip(text)
}

/// Return just the thinking block bodies in source order.
pub fn extract_thinking(text: &str, tags: &[&str], markdown_style: bool) -> Vec<String> {
    strip_thinking(text, tags, markdown_style).thinking
}

// ---- tests ----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn strip(text: &str) -> StrippedResult {
        strip_thinking(text, DEFAULT_TAGS, false)
    }

    #[test]
    fn no_thinking_tag() {
        let r = strip("Hello, world!");
        assert_eq!(r.clean, "Hello, world!");
        assert!(!r.had_thinking);
        assert!(r.thinking.is_empty());
    }

    #[test]
    fn empty_input() {
        let r = strip("");
        assert_eq!(r.clean, "");
        assert!(!r.had_thinking);
    }

    #[test]
    fn basic_thinking_tag() {
        let r = strip("<thinking>step 1</thinking>The answer is 42.");
        assert_eq!(r.clean, "The answer is 42.");
        assert_eq!(r.thinking, vec!["step 1"]);
        assert!(r.had_thinking);
    }

    #[test]
    fn mid_sentence_thinking() {
        let r = strip("Let me think. <thinking>step 1</thinking>The answer is 42.");
        assert_eq!(r.clean, "Let me think. The answer is 42.");
        assert_eq!(r.thinking, vec!["step 1"]);
    }

    #[test]
    fn think_tag() {
        let r = strip("<think>deepseek reasoning</think>Final answer.");
        assert_eq!(r.thinking, vec!["deepseek reasoning"]);
        assert!(r.clean.contains("Final answer."));
    }

    #[test]
    fn multiple_blocks() {
        let r = strip("<thinking>step 1</thinking>a<thinking>step 2</thinking>b");
        assert_eq!(r.thinking, vec!["step 1", "step 2"]);
        assert_eq!(r.clean, "ab");
    }

    #[test]
    fn pipe_form() {
        let r = strip("<|thinking|>inner</|thinking|>answer");
        assert_eq!(r.thinking, vec!["inner"]);
        assert!(r.clean.contains("answer"));
    }

    #[test]
    fn unclosed_angle_tag() {
        let r = strip("Prefix. <thinking>unfinished");
        assert_eq!(r.thinking, vec!["unfinished"]);
        assert_eq!(r.clean, "Prefix.");
    }

    #[test]
    fn case_insensitive() {
        let r = strip("<THINKING>step</THINKING>done");
        assert_eq!(r.thinking, vec!["step"]);
        assert!(r.clean.contains("done"));
    }

    #[test]
    fn multiline_thinking() {
        let r = strip("<thinking>\nline1\nline2\n</thinking>answer");
        assert_eq!(r.thinking, vec!["line1\nline2"]);
    }

    #[test]
    fn extract_thinking_fn() {
        let blocks = extract_thinking("<thinking>x</thinking>ignored", DEFAULT_TAGS, false);
        assert_eq!(blocks, vec!["x"]);
    }

    #[test]
    fn markdown_style_closed() {
        let r = strip_thinking(
            "## Thinking\nstep 1\n## End thinking\nFinal",
            DEFAULT_TAGS,
            true,
        );
        assert_eq!(r.thinking, vec!["step 1"]);
        assert!(r.clean.contains("Final"));
    }

    #[test]
    fn custom_tags() {
        let r = strip_thinking("<reasoning>calc</reasoning>done", &["reasoning"], false);
        assert_eq!(r.thinking, vec!["calc"]);
        assert!(r.clean.contains("done"));
    }

    #[test]
    fn whitespace_collapses_after_removal() {
        let r = strip("a  <thinking>x</thinking>  b");
        // double spaces around removed block should collapse
        assert!(!r.clean.contains("  "));
    }

    #[test]
    fn had_thinking_false_when_none() {
        let r = strip("plain text");
        assert!(!r.had_thinking);
    }

    #[test]
    fn interleaved_different_tags_preserve_source_order() {
        // `<think>` appears before `<thinking>` in the source, so the
        // extracted bodies must come back as ["a", "b"], not ["b", "a"].
        let r = strip("<think>a</think>x<thinking>b</thinking>y");
        assert_eq!(r.thinking, vec!["a", "b"]);
        assert_eq!(r.clean, "xy");
    }

    #[test]
    fn interleaved_angle_and_pipe_preserve_source_order() {
        let r = strip("<|thinking|>p</|thinking|>m<thinking>q</thinking>n");
        assert_eq!(r.thinking, vec!["p", "q"]);
        assert_eq!(r.clean, "mn");
    }

    #[test]
    fn reuse_stripper_across_inputs() {
        let s = Stripper::new(DEFAULT_TAGS, false);
        let a = s.strip("<thinking>1</thinking>A");
        let b = s.strip("<thinking>2</thinking>B");
        assert_eq!(a.thinking, vec!["1"]);
        assert_eq!(b.thinking, vec!["2"]);
        assert_eq!(a.clean, "A");
        assert_eq!(b.clean, "B");
    }

    #[test]
    fn closed_block_with_unicode_body() {
        // Ensures byte-offset slicing respects char boundaries.
        let r = strip("café <thinking>réfléchir 🤔</thinking>déjà");
        assert_eq!(r.thinking, vec!["réfléchir 🤔"]);
        assert_eq!(r.clean, "café déjà");
    }

    #[test]
    fn closed_then_unclosed_combination() {
        let r = strip("<thinking>first</thinking>middle <thinking>tail");
        assert_eq!(r.thinking, vec!["first", "tail"]);
        assert_eq!(r.clean, "middle");
    }

    #[test]
    fn three_interleaved_blocks_in_order() {
        let r = strip("<thinking>1</thinking>a<think>2</think>b<thinking>3</thinking>c");
        assert_eq!(r.thinking, vec!["1", "2", "3"]);
        assert_eq!(r.clean, "abc");
    }
}
