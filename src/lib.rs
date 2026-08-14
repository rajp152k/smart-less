use std::{
    fs,
    io::{self, IsTerminal, Read, Write},
    path::Path,
};

use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};

const DEFAULT_MAX_BYTES: usize = 8 * 1024 * 1024;
const ESC: char = '\u{1b}';

#[derive(Debug, Parser)]
#[command(
    name = "sl",
    version,
    about = "A safe, read-only terminal document reader"
)]
pub struct Cli {
    /// Files to read. Use - for standard input.
    #[arg(value_name = "FILE", default_value = "-")]
    pub files: Vec<String>,

    /// Force the input type instead of detecting it.
    #[arg(short = 't', long = "type", value_enum)]
    pub kind: Option<DocumentKind>,

    /// Emit unstyled text and do not open a pager.
    #[arg(long)]
    pub plain: bool,

    /// Control ANSI colour output.
    #[arg(long, value_enum, default_value_t = ColorMode::Auto)]
    pub color: ColorMode,

    /// Render as though the terminal has this many columns.
    #[arg(long, value_name = "COLUMNS")]
    pub width: Option<usize>,

    /// Maximum bytes read per input.
    #[arg(long, default_value_t = DEFAULT_MAX_BYTES, value_name = "BYTES")]
    pub max_bytes: usize,

    /// Print output directly rather than using the built-in pager.
    #[arg(long)]
    pub no_pager: bool,

    /// Omit text line numbers.
    #[arg(long)]
    pub no_line_numbers: bool,

    /// List supported document types.
    #[arg(long)]
    pub list_types: bool,

    /// Suppress non-fatal warnings.
    #[arg(short, long)]
    pub quiet: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum ColorMode {
    Auto,
    Always,
    Never,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum DocumentKind {
    Text,
    Markdown,
    Json,
    Yaml,
    Toml,
    Csv,
    Tsv,
    Binary,
}

impl DocumentKind {
    fn detect(name: Option<&str>, bytes: &[u8]) -> Self {
        let lower = name.unwrap_or_default().to_ascii_lowercase();
        let file = Path::new(&lower)
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        if matches!(file, "makefile" | "dockerfile" | ".gitignore") {
            return Self::Text;
        }
        let extension = file.rsplit_once('.').map(|(_, extension)| extension);
        match extension {
            Some("md" | "markdown" | "mdown") => Self::Markdown,
            Some("json" | "jsonc" | "ndjson") => Self::Json,
            Some("yaml" | "yml") => Self::Yaml,
            Some("toml") => Self::Toml,
            Some("csv") => Self::Csv,
            Some("tsv") => Self::Tsv,
            _ if bytes.contains(&0) || std::str::from_utf8(bytes).is_err() => Self::Binary,
            _ if looks_like_json(bytes) => Self::Json,
            _ => Self::Text,
        }
    }
}

#[derive(Clone, Copy)]
struct RenderOptions {
    color: bool,
    width: usize,
    line_numbers: bool,
}

pub fn run(cli: Cli) -> i32 {
    if cli.list_types {
        println!("text\nmarkdown\njson\nyaml\ntoml\ncsv\ntsv\nbinary");
        return 0;
    }
    if cli.max_bytes == 0 {
        eprintln!("sl: --max-bytes must be greater than zero");
        return 1;
    }

    let stdout_is_tty = io::stdout().is_terminal();
    let color = !cli.plain
        && match cli.color {
            ColorMode::Always => true,
            ColorMode::Never => false,
            ColorMode::Auto => stdout_is_tty && std::env::var_os("NO_COLOR").is_none(),
        };
    let options = RenderOptions {
        color,
        width: cli.width.unwrap_or(100).max(20),
        line_numbers: !cli.no_line_numbers,
    };

    let mut documents = Vec::new();
    let mut failures = 0;
    for file in &cli.files {
        match read_and_render(file, cli.kind, cli.max_bytes, options) {
            Ok((rendered, warning)) => {
                if let Some(warning) = warning.filter(|_| !cli.quiet) {
                    eprintln!("sl: {file}: {warning}");
                }
                documents.push((file, rendered));
            }
            Err(error) => {
                eprintln!("sl: {file}: {error:#}");
                failures += 1;
            }
        }
    }
    if documents.is_empty() {
        return 1;
    }

    let multiple = documents.len() > 1;
    let mut output = String::new();
    for (index, (name, document)) in documents.iter().enumerate() {
        if multiple {
            if index > 0 {
                output.push('\n');
            }
            output.push_str(&format!(
                "{}\n",
                style(&format!("==> {name} <=="), "1;36", options.color)
            ));
        }
        output.push_str(document);
        if !document.ends_with('\n') {
            output.push('\n');
        }
    }

    if stdout_is_tty && !cli.plain && !cli.no_pager {
        if let Err(error) = page(&output) {
            eprintln!("sl: pager error: {error:#}");
            return 1;
        }
    } else if let Err(error) = write_stdout(&output)
        && error.kind() != io::ErrorKind::BrokenPipe
    {
        eprintln!("sl: write error: {error}");
        return 1;
    }
    if failures > 0 { 2 } else { 0 }
}

fn page(output: &str) -> Result<()> {
    let pager = minus::Pager::new();
    pager
        .push_str(output)
        .context("could not prepare pager output")?;
    minus::page_all(pager).context("could not start pager")?;
    Ok(())
}

fn write_stdout(output: &str) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    stdout.write_all(output.as_bytes())?;
    stdout.flush()
}

fn read_and_render(
    source: &str,
    forced_kind: Option<DocumentKind>,
    max_bytes: usize,
    options: RenderOptions,
) -> Result<(String, Option<String>)> {
    let (bytes, truncated) = read_source(source, max_bytes)?;
    let kind = forced_kind
        .unwrap_or_else(|| DocumentKind::detect((source != "-").then_some(source), &bytes));
    let mut warning = truncated.then(|| format!("input was truncated at {max_bytes} bytes"));
    let rendered = match render(kind, &bytes, options) {
        Ok(rendered) => rendered,
        Err(error) => {
            warning = Some(format!("{}; rendered safely as text", error));
            render_text(&String::from_utf8_lossy(&bytes), options)
        }
    };
    Ok((rendered, warning))
}

fn read_source(source: &str, max_bytes: usize) -> Result<(Vec<u8>, bool)> {
    let mut reader: Box<dyn Read> = if source == "-" {
        Box::new(io::stdin().lock())
    } else {
        Box::new(fs::File::open(source).with_context(|| "could not open input")?)
    };
    let mut bytes = Vec::with_capacity(max_bytes.min(64 * 1024));
    let mut limited = reader.by_ref().take(max_bytes as u64 + 1);
    limited
        .read_to_end(&mut bytes)
        .context("could not read input")?;
    let truncated = bytes.len() > max_bytes;
    if truncated {
        bytes.truncate(max_bytes);
    }
    Ok((bytes, truncated))
}

fn render(kind: DocumentKind, bytes: &[u8], options: RenderOptions) -> Result<String> {
    match kind {
        DocumentKind::Text => Ok(render_text(&String::from_utf8_lossy(bytes), options)),
        DocumentKind::Markdown => Ok(render_markdown(&String::from_utf8_lossy(bytes), options)),
        DocumentKind::Json => render_json(bytes, options),
        DocumentKind::Yaml => render_yaml(bytes, options),
        DocumentKind::Toml => render_toml(bytes, options),
        DocumentKind::Csv => render_delimited(bytes, b',', options),
        DocumentKind::Tsv => render_delimited(bytes, b'\t', options),
        DocumentKind::Binary => Ok(render_binary(bytes, options)),
    }
}

fn render_json(bytes: &[u8], options: RenderOptions) -> Result<String> {
    let value: serde_json::Value = serde_json::from_slice(bytes).context("invalid JSON")?;
    let pretty = serde_json::to_string_pretty(&value)?;
    Ok(render_structured(&pretty, options))
}

fn render_yaml(bytes: &[u8], options: RenderOptions) -> Result<String> {
    let text = std::str::from_utf8(bytes).context("YAML is not UTF-8")?;
    let _: serde_yaml_ng::Value = serde_yaml_ng::from_str(text).context("invalid YAML")?;
    Ok(render_structured(text, options))
}

fn render_toml(bytes: &[u8], options: RenderOptions) -> Result<String> {
    let text = std::str::from_utf8(bytes).context("TOML is not UTF-8")?;
    let _: toml::Value = toml::from_str(text).context("invalid TOML")?;
    Ok(render_structured(text, options))
}

fn render_structured(text: &str, options: RenderOptions) -> String {
    text.lines()
        .map(|line| {
            let safe = sanitize(line);
            let styled = if safe.trim_start().starts_with(['{', '}', '[', ']']) {
                style(&safe, "36", options.color)
            } else if safe.contains(':') || safe.contains('=') {
                style(&safe, "33", options.color)
            } else {
                safe
            };
            format!("{}\n", styled)
        })
        .collect()
}

fn render_text(text: &str, options: RenderOptions) -> String {
    let line_count = text.lines().count().max(1);
    let digits = line_count.to_string().len();
    text.lines()
        .enumerate()
        .map(|(index, line)| {
            let line = truncate_display(&sanitize(line), options.width);
            if options.line_numbers {
                format!(
                    "{} {}\n",
                    style(&format!("{:>digits$}", index + 1), "2;90", options.color),
                    line
                )
            } else {
                format!("{line}\n")
            }
        })
        .collect()
}

fn render_markdown(text: &str, options: RenderOptions) -> String {
    let mut fenced = false;
    text.lines()
        .map(|line| {
            let safe = sanitize(line);
            let trimmed = safe.trim_start();
            let (content, style_code) = if trimmed.starts_with("```") || trimmed.starts_with("~~~")
            {
                fenced = !fenced;
                (safe, "2;35")
            } else if fenced {
                (safe, "2")
            } else if let Some(title) = trimmed.strip_prefix("# ") {
                (title.to_owned(), "1;36")
            } else if trimmed.starts_with("##") {
                (
                    trimmed.trim_start_matches('#').trim_start().to_owned(),
                    "1;34",
                )
            } else if trimmed.starts_with("> ") {
                (safe, "2;33")
            } else if trimmed.starts_with("- ")
                || trimmed.starts_with("* ")
                || trimmed.starts_with("+ ")
            {
                (safe, "32")
            } else {
                (safe, "")
            };
            format!(
                "{}\n",
                style(
                    &truncate_display(&content, options.width),
                    style_code,
                    options.color
                )
            )
        })
        .collect()
}

fn render_delimited(bytes: &[u8], delimiter: u8, options: RenderOptions) -> Result<String> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .delimiter(delimiter)
        .from_reader(bytes);
    let mut rows = Vec::new();
    for record in reader.records().take(100) {
        rows.push(record?.iter().map(sanitize).collect::<Vec<_>>());
    }
    if rows.is_empty() {
        return Ok(String::new());
    }
    let columns = rows.iter().map(Vec::len).max().unwrap_or(0).min(12);
    let mut widths = vec![3; columns];
    for row in &rows {
        for (index, value) in row.iter().take(columns).enumerate() {
            widths[index] =
                widths[index].max(unicode_width::UnicodeWidthStr::width(value.as_str()).min(30));
        }
    }
    let mut output = String::new();
    for (row_index, row) in rows.iter().enumerate() {
        let cells = (0..columns)
            .map(|index| {
                pad_display(
                    row.get(index).map(String::as_str).unwrap_or(""),
                    widths[index],
                )
            })
            .collect::<Vec<_>>();
        let line = format!("| {} |", cells.join(" | "));
        output.push_str(&style(
            &truncate_display(&line, options.width),
            if row_index == 0 { "1;36" } else { "" },
            options.color,
        ));
        output.push('\n');
    }
    Ok(output)
}

fn render_binary(bytes: &[u8], options: RenderOptions) -> String {
    let mut output = String::new();
    for (offset, chunk) in bytes.chunks(16).take(256).enumerate() {
        let hex = chunk
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<Vec<_>>()
            .join(" ");
        let ascii: String = chunk
            .iter()
            .map(|byte| {
                if byte.is_ascii_graphic() {
                    *byte as char
                } else {
                    '.'
                }
            })
            .collect();
        output.push_str(&format!(
            "{}  {:<47}  {}\n",
            style(&format!("{:08x}", offset * 16), "2;90", options.color),
            hex,
            ascii
        ));
    }
    output
}

fn sanitize(input: &str) -> String {
    input
        .chars()
        .map(|character| match character {
            ESC => '␛',
            character if character.is_control() && character != '\t' => '�',
            character => character,
        })
        .collect()
}

fn truncate_display(input: &str, width: usize) -> String {
    let mut result = String::new();
    let mut used = 0;
    for character in input.chars() {
        // ANSI escape sequences generated by us are left intact; their display width is zero.
        if character == ESC {
            result.push(character);
            continue;
        }
        let character_width = unicode_width::UnicodeWidthChar::width(character).unwrap_or(0);
        if used + character_width > width.saturating_sub(1) {
            result.push('…');
            break;
        }
        used += character_width;
        result.push(character);
    }
    result
}

fn pad_display(input: &str, width: usize) -> String {
    let clipped = truncate_display(input, width + 1);
    let padding = width.saturating_sub(unicode_width::UnicodeWidthStr::width(clipped.as_str()));
    format!("{clipped}{}", " ".repeat(padding))
}

fn style(input: &str, code: &str, enabled: bool) -> String {
    if enabled && !code.is_empty() {
        format!("\x1b[{code}m{input}\x1b[0m")
    } else {
        input.to_owned()
    }
}

fn looks_like_json(bytes: &[u8]) -> bool {
    let text = String::from_utf8_lossy(bytes);
    let trimmed = text.trim_start();
    (trimmed.starts_with('{') || trimmed.starts_with('['))
        && serde_json::from_str::<serde_json::Value>(&text).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extensions_take_priority() {
        assert_eq!(
            DocumentKind::detect(Some("report.JSON"), b"not JSON"),
            DocumentKind::Json
        );
        assert_eq!(
            DocumentKind::detect(Some("README.md"), b"{}"),
            DocumentKind::Markdown
        );
        assert_eq!(
            DocumentKind::detect(Some("unknown"), b"{}"),
            DocumentKind::Json
        );
    }

    #[test]
    fn terminal_controls_are_visible() {
        assert_eq!(sanitize("hello\x1b[2J"), "hello␛[2J");
    }

    #[test]
    fn json_is_pretty_printed() {
        let rendered = render_json(
            br#"{"a":[1,2]}"#,
            RenderOptions {
                color: false,
                width: 80,
                line_numbers: false,
            },
        )
        .unwrap();
        assert!(rendered.contains("\"a\": ["));
    }

    #[test]
    fn csv_is_tabular() {
        let rendered = render_delimited(
            b"name,age\nAda,37\n",
            b',',
            RenderOptions {
                color: false,
                width: 80,
                line_numbers: false,
            },
        )
        .unwrap();
        assert!(rendered.contains("| name | age |"));
    }

    #[test]
    fn yaml_and_toml_are_validated() {
        let options = RenderOptions {
            color: false,
            width: 80,
            line_numbers: false,
        };
        assert!(render_yaml(b"title: smart-less\n", options).is_ok());
        assert!(render_toml(b"title = 'smart-less'\n", options).is_ok());
        assert!(render_yaml(b"title: [\n", options).is_err());
        assert!(render_toml(b"title = [\n", options).is_err());
    }

    #[test]
    fn markdown_keeps_mermaid_as_safe_source() {
        let rendered = render_markdown(
            "# Diagram\n```mermaid\ngraph TD\n  A-->B\n```",
            RenderOptions {
                color: false,
                width: 80,
                line_numbers: false,
            },
        );
        assert!(rendered.contains("graph TD"));
        assert!(rendered.contains("A-->B"));
    }
}
