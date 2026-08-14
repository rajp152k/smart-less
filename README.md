# smart-less (`sl`)

A safe, read-only terminal reader for structured documents. `sl` detects common file types, renders them for terminal reading, and opens an internal `less`-style pager when stdout is a TTY.

## Install / run

```sh
cargo run -- README.md
cargo run -- data.json
cat config.yaml | cargo run -- -
```

The built binary is named `sl`:

```sh
cargo build --release
./target/release/sl --help
```

## Supported input

| Type | Detection | Rendering |
| --- | --- | --- |
| Markdown | `.md`, `.markdown`, `.mdown`, or conservative Markdown sniffing | reflowed prose, lists, quotes, readable links, and wrapped GFM pipe tables; fenced blocks/Mermaid remain safe source text |
| JSON | `.json`, `.jsonc`, `.ndjson`, or valid JSON sniffing | validated and pretty-printed |
| YAML | `.yaml`, `.yml` | validated, structured terminal text |
| TOML | `.toml` | validated, structured terminal text |
| CSV / TSV | `.csv`, `.tsv` | bounded terminal table (first 100 rows / 12 columns) |
| Source / config code | Rust, Python, JavaScript/TypeScript, shell, Go, and common code/config extensions | safe, bounded syntax highlighting with optional line numbers |
| Text | fallback and conventional text names | safe text with optional line numbers |
| Binary | NUL or invalid UTF-8 | bounded hex/ASCII preview |

## Safety contract

`sl` only reads its specified input or stdin. It never executes document content, follows links, launches renderers, fetches assets, or writes source files. Input control characters (including terminal escapes) are rendered visibly rather than replayed.

Each input is capped at 8 MiB by default; override deliberately with `--max-bytes`. Truncated or invalid structured inputs emit a warning on stderr and fall back to safe text rendering.

## CLI

```text
sl [OPTIONS] [FILE]...

-t, --type <KIND>       Force text, markdown, json, yaml, toml, csv, tsv, code, or binary
    --plain             Disable styles and paging
    --color <MODE>      auto, always, or never
    --width <COLUMNS>   Deterministic render width
    --max-bytes <N>     Per-input read limit
    --no-pager          Write directly instead of paging
    --no-line-numbers   Omit text line numbers
    --list-types        Print supported kinds
-q, --quiet             Suppress non-fatal warnings
```

Type selection order is: `--type`, well-known basename, extension, bounded content sniffing, then text/binary fallback. Recognized source extensions select code highlighting; JSON, YAML, TOML, CSV/TSV, and Markdown keep their dedicated renderers.

When paging is active, embedded navigation and search are provided by [`minus`](https://crates.io/crates/minus). Structural tree navigation and folding are deliberately deferred rather than approximated with a full TUI.

## V1 boundaries

V1 intentionally does not provide a full-screen document editor, structural tree navigation or structured-data folding, image protocols, browser-backed Mermaid rendering, plugins, network access, archive/PDF/image parsing, or source-code execution. Those belong to future capability-gated work.

## Development

```sh
cargo fmt --check
cargo test
cargo clippy -- -D warnings
```
