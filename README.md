# llm-think-tag-strip

Strip `<thinking>` / `<think>` reasoning blocks from LLM output.

A small, dependency-light Rust library for cleaning up the "chain-of-thought"
sections that models like Claude (extended thinking) and DeepSeek-R1 emit
before their final answer. It removes the reasoning blocks, optionally returns
the extracted reasoning, and tidies up the leftover whitespace.

## What it handles

- **Angle tags** — `<thinking>...</thinking>`, `<think>...</think>`
- **Bracketed-pipe form** — `<|thinking|>...</|thinking|>`
- **Markdown headings** (opt-in) — `## Thinking ... ## End thinking`
- **Unclosed open tags** — an open tag with no closing tag is treated as
  "from here to the end of the string is thinking" and stripped.
- **Case-insensitive** matching and **multiline** block bodies.
- **Custom tag names** — pass your own (e.g. `["reasoning"]`).

After stripping, runs of horizontal whitespace are collapsed, stray spaces
around newlines are cleaned, and 3+ blank lines are reduced to a blank line.

## Install

Add it to your `Cargo.toml`:

```toml
[dependencies]
llm-think-tag-strip = "0.1"
```

## Usage

```rust
use llm_think_tag_strip::strip_thinking;

let out = strip_thinking(
    "Let me think. <thinking>step 1</thinking>The answer is 42.",
    &["thinking", "think"],
    false, // markdown_style
);

assert_eq!(out.clean, "Let me think. The answer is 42.");
assert_eq!(out.thinking, vec!["step 1"]);
assert!(out.had_thinking);
```

### Default tags

```rust
use llm_think_tag_strip::{strip_thinking, DEFAULT_TAGS};

let out = strip_thinking("<think>deepseek reasoning</think>Final answer.", DEFAULT_TAGS, false);
assert_eq!(out.clean, "Final answer.");
```

`DEFAULT_TAGS` is `["thinking", "think"]`.

### Extract only the reasoning

```rust
use llm_think_tag_strip::{extract_thinking, DEFAULT_TAGS};

let blocks = extract_thinking("<thinking>x</thinking>ignored", DEFAULT_TAGS, false);
assert_eq!(blocks, vec!["x"]);
```

### Reusable stripper (pre-compiled patterns)

If you strip many strings with the same configuration, build a `Stripper` once
and reuse it:

```rust
use llm_think_tag_strip::Stripper;

let stripper = Stripper::new(&["thinking", "think"], false);
let result = stripper.strip("<thinking>...</thinking>answer");
```

### Markdown-style blocks

Pass `markdown_style = true` to also strip heading-delimited reasoning:

```rust
use llm_think_tag_strip::{strip_thinking, DEFAULT_TAGS};

let out = strip_thinking(
    "## Thinking\nstep 1\n## End thinking\nFinal",
    DEFAULT_TAGS,
    true,
);
assert_eq!(out.thinking, vec!["step 1"]);
```

## API

| Item | Description |
| --- | --- |
| `strip_thinking(text, tags, markdown_style) -> StrippedResult` | Strip blocks and return cleaned text plus extracted reasoning. |
| `extract_thinking(text, tags, markdown_style) -> Vec<String>` | Return just the reasoning block bodies, in source order. |
| `Stripper::new(tags, markdown_style)` / `.strip(text)` | Reusable stripper that holds its configuration. |
| `StrippedResult { clean, thinking, had_thinking }` | Result type: cleaned text, extracted blocks, and whether any were found. |
| `DEFAULT_TAGS` | `["thinking", "think"]`. |

## Tech stack

- **Rust** (edition 2021)
- [`regex`](https://crates.io/crates/regex) for tag matching and whitespace collapsing

## Testing

```sh
cargo test
```

## License

MIT
