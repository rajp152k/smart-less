# smart-less (`sl`)

A safe, read-only terminal reader for structured documents. `sl` detects common file types, renders them for terminal reading, and opens an internal `less`-style pager when stdout is a TTY.

## Install / run

Install from GitHub:

```sh
cargo install --git https://github.com/rajp152k/smart-less --locked
```

After the crate is published, use `cargo install smart-less` instead.

```sh
sl README.md
sl data.json
cat config.yaml | sl -
```

Or build from a checkout:

```sh
cargo build --release --locked
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

Each input is capped at 8 MiB by default; override deliberately with `--max-bytes` (which must be greater than zero). Reads are bounded per input, CSV/TSV rendering is limited to 100 rows and 12 columns, and binary previews are limited to 4 KiB. Truncated or invalid structured inputs emit a warning on stderr and fall back to safe text rendering.

Exit status is `0` when every requested input was rendered (including safe fallback after a non-fatal parse warning), `2` when at least one requested input could not be read while another was rendered, and `1` for invalid CLI limits, pager/output failures, or when no input could be rendered. Broken pipes are treated as normal output termination.

## CLI

```text
sl [OPTIONS] [FILE]...

-t, --type <KIND>       Force text, markdown, json, yaml, toml, csv, tsv, code, or binary
    --plain             Disable styles and paging
    --color <MODE>      auto, always, or never
    --theme <THEME>     default, dark, or mono (mono emits no ANSI)
    --width <COLUMNS>   Deterministic render width
    --max-bytes <N>     Per-input read limit
    --no-pager          Write directly instead of paging
    --no-line-numbers   Omit text line numbers
    --list-types        Print supported kinds
-q, --quiet             Suppress non-fatal warnings
```

Type selection order is: `--type`, well-known basename, extension, bounded content sniffing, then text/binary fallback. Recognized source extensions select code highlighting; JSON, YAML, TOML, CSV/TSV, and Markdown keep their dedicated renderers.

When stdout is a TTY, paging is active unless `--plain` or `--no-pager` is supplied. The built-in [`minus`](https://crates.io/crates/minus) pager provides the familiar screen navigation and text search controls shown by its on-screen help (use `q` to quit and `/` to search). `sl` does not implement structural tree navigation, folding, or jump-to-key navigation; those are deliberately deferred rather than claimed as pager features.

`--theme default` uses the standard ANSI palette; `--theme dark` uses brighter foregrounds for dark terminals; and `--theme mono`, `--color never`, or `--plain` emits no ANSI styling. Regardless of theme, input escape/control sequences are shown visibly and are never passed through.

## V1 boundaries

V1 intentionally does not provide a full-screen document editor, structural tree navigation or structured-data folding, image protocols, browser-backed Mermaid rendering, plugins, network access, archive/PDF/image parsing, or source-code execution. Those are deferred rather than silently approximated.

## Development

```sh
cargo fmt --check
cargo test
cargo clippy -- -D warnings
```
