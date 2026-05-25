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
    )).unwrap()
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

// ---- Stripper -------------------------------------------------------------

/// Reusable stripper with pre-compiled patterns.
pub struct Stripper {
    tags: Vec<String>,
    markdown_style: bool,
}

impl Stripper {
    pub fn new(tags: &[&str], markdown_style: bool) -> Self {
        Self {
            tags: tags.iter().map(|s| s.to_string()).collect(),
            markdown_style,
        }
    }

    pub fn strip(&self, text: &str) -> StrippedResult {
        if text.is_empty() {
            return StrippedResult { clean: String::new(), thinking: vec![], had_thinking: false };
        }

        let mut thinking: Vec<String> = Vec::new();
        let mut out = text.to_owned();

        // --- closed blocks ---
        for tag in &self.tags {
            let re = angle_closed_re(tag);
            out = re.replace_all(&out, |caps: &regex::Captures| {
                thinking.push(caps[1].trim().to_owned());
                String::new()
            }).into_owned();
        }
        for tag in &self.tags {
            let re = pipe_closed_re(tag);
            out = re.replace_all(&out, |caps: &regex::Captures| {
                thinking.push(caps[1].trim().to_owned());
                String::new()
            }).into_owned();
        }
        if self.markdown_style {
            for tag in &self.tags {
                let re = markdown_closed_re(tag);
                out = re.replace_all(&out, |caps: &regex::Captures| {
                    thinking.push(caps[1].trim().to_owned());
                    String::new()
                }).into_owned();
            }
        }

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
        StrippedResult { clean, thinking, had_thinking }
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
}
