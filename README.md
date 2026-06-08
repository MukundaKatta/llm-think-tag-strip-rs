# llm-think-tag-strip

[![CI](https://github.com/MukundaKatta/llm-think-tag-strip-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/MukundaKatta/llm-think-tag-strip-rs/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/llm-think-tag-strip.svg)](https://crates.io/crates/llm-think-tag-strip)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](#license)

Strip `<thinking>` / `<think>` reasoning blocks from LLM output.

Many models emit their chain-of-thought wrapped in tags before the real answer:

- **Claude** extended thinking — `<thinking>...</thinking>`
- **DeepSeek-R1** and others — `<think>...</think>`
- The bracketed-pipe form some tokenizers use — `<|thinking|>...</|thinking|>`
- Optionally, a markdown-heading form — `### Thinking ... ### End thinking`

This crate removes those blocks from a response, optionally handing you the
extracted reasoning separately, and tidies up the leftover whitespace.

## Features

- Handles the angle (`<thinking>`), pipe (`<|thinking|>`), and (opt-in)
  markdown-heading syntaxes.
- Case-insensitive and multiline aware.
- Removes a trailing **unclosed** block (an opening tag with no close) — useful
  when a stream was cut off mid-thought.
- Returns the cleaned text, the extracted thinking bodies **in source order**,
  and a `had_thinking` flag.
- Configurable tag names, so you can match `<reasoning>`, `<scratchpad>`, etc.
- Tiny: one dependency (`regex`).

## Install

Add it to your `Cargo.toml`:

```toml
[dependencies]
llm-think-tag-strip = "0.1"
```

or:

```sh
cargo add llm-think-tag-strip
```

## Usage

```rust
use llm_think_tag_strip::{strip_thinking, DEFAULT_TAGS};

let raw = "Let me think. <thinking>step 1\nstep 2</thinking>The answer is 42.";
let out = strip_thinking(raw, DEFAULT_TAGS, false);

assert_eq!(out.clean, "Let me think. The answer is 42.");
assert_eq!(out.thinking, vec!["step 1\nstep 2"]);
assert!(out.had_thinking);
```

### Just the cleaned text

```rust
use llm_think_tag_strip::strip_thinking;

let clean = strip_thinking("<think>reasoning</think>Final answer.", &["think"], false).clean;
assert_eq!(clean, "Final answer.");
```

### Just the reasoning

```rust
use llm_think_tag_strip::{extract_thinking, DEFAULT_TAGS};

let blocks = extract_thinking("<thinking>a</thinking>x<thinking>b</thinking>y", DEFAULT_TAGS, false);
assert_eq!(blocks, vec!["a", "b"]);
```

### Reusing a stripper

`strip_thinking` / `extract_thinking` recompile their regexes on every call.
When processing many inputs with the same configuration, build a [`Stripper`]
once and reuse it:

```rust
use llm_think_tag_strip::{Stripper, DEFAULT_TAGS};

let stripper = Stripper::new(DEFAULT_TAGS, false);
for chunk in ["<thinking>1</thinking>A", "<thinking>2</thinking>B"] {
    let result = stripper.strip(chunk);
    println!("{}", result.clean);
}
```

### Custom tags

```rust
use llm_think_tag_strip::strip_thinking;

let out = strip_thinking("<reasoning>calc</reasoning>done", &["reasoning"], false);
assert_eq!(out.clean, "done");
assert_eq!(out.thinking, vec!["calc"]);
```

### Markdown-heading style

Pass `markdown_style = true` to also strip `### Thinking ... ### End thinking`
blocks:

```rust
use llm_think_tag_strip::{strip_thinking, DEFAULT_TAGS};

let out = strip_thinking("## Thinking\nstep 1\n## End thinking\nFinal", DEFAULT_TAGS, true);
assert_eq!(out.thinking, vec!["step 1"]);
assert!(out.clean.contains("Final"));
```

## API

### `strip_thinking(text, tags, markdown_style) -> StrippedResult`

Strip every recognized thinking block from `text`.

- `text: &str` — the model output.
- `tags: &[&str]` — tag names to recognize (use [`DEFAULT_TAGS`] for
  `["thinking", "think"]`).
- `markdown_style: bool` — also strip `### Thinking ... ### End thinking`.

### `extract_thinking(text, tags, markdown_style) -> Vec<String>`

Convenience wrapper returning only the extracted thinking bodies, in source
order.

### `struct Stripper`

A reusable stripper with the tag list and options baked in.

- `Stripper::new(tags: &[&str], markdown_style: bool) -> Stripper`
- `Stripper::strip(&self, text: &str) -> StrippedResult`

### `struct StrippedResult`

| field          | type          | meaning                                          |
| -------------- | ------------- | ------------------------------------------------ |
| `clean`        | `String`      | Text with all thinking blocks removed, whitespace collapsed. |
| `thinking`     | `Vec<String>` | Extracted thinking-block bodies, in source order. |
| `had_thinking` | `bool`        | `true` if at least one block was found.          |

### `const DEFAULT_TAGS: &[&str]`

`["thinking", "think"]` — Claude and DeepSeek-R1 defaults.

## Behavior notes

- Matching is **case-insensitive** (`<THINKING>` is recognized).
- A single trailing **unclosed** block (opening tag, no close) is treated as
  "everything from the tag to the end of the string is thinking" and dropped.
- After removal the remaining text has runs of horizontal whitespace collapsed,
  trailing spaces trimmed, and 3+ blank lines reduced to one blank line.
- Extracted bodies are trimmed of surrounding whitespace.

## Development

```sh
cargo build
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

## License

MIT
