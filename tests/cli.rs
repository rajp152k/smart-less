use std::{
    io::Write,
    path::PathBuf,
    process::{Command, Output, Stdio},
};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn sl(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_sl"))
        .args(args)
        .output()
        .expect("run sl")
}

fn plain_args(name: &str) -> Vec<String> {
    vec![
        "--plain".into(),
        "--no-pager".into(),
        fixture(name).display().to_string(),
    ]
}

#[test]
fn fixture_formats_render_deterministically() {
    for (name, expected) in [
        (
            "sample.json",
            "{\n  \"ports\": [\n    80,\n    443\n  ],\n  \"service\": \"smart-less\"\n}\n",
        ),
        ("sample.yaml", "---\nservice: smart-less\nport: 8080\n"),
        (
            "sample.toml",
            "title = \"smart-less\"\n[server]\nport = 8080\n",
        ),
        ("sample.csv", "| name | age |\n| Ada  | 37  |\n"),
        ("sample.tsv", "| name  | age |\n| Grace | 85  |\n"),
    ] {
        let args = plain_args(name);
        let output = Command::new(env!("CARGO_BIN_EXE_sl"))
            .args(&args)
            .output()
            .expect("run fixture");
        assert!(output.status.success(), "{name}: {output:?}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), expected, "{name}");
        assert!(output.stderr.is_empty(), "{name}");
    }
}

#[test]
fn markdown_tables_and_links_are_readable() {
    let args = plain_args("sample.md");
    let output = Command::new(env!("CARGO_BIN_EXE_sl"))
        .args(&args)
        .output()
        .expect("run markdown fixture");
    let rendered = String::from_utf8(output.stdout).expect("fixture output is UTF-8");
    assert!(output.status.success());
    assert_eq!(
        rendered,
        "Notes\n\nA documentation link <https://example.test/docs>.\n\n| Name | Value |\n| Ada  |    37 |\n"
    );
}

#[test]
fn source_color_themes_and_plain_mode_are_safe() {
    let path = fixture("sample.rs").display().to_string();
    let default = sl(&[
        "--color",
        "always",
        "--no-pager",
        "--no-line-numbers",
        &path,
    ]);
    assert!(default.status.success());
    assert!(default.stdout.starts_with(b"\x1b[35mfn\x1b[0m"));

    let dark = sl(&[
        "--color",
        "always",
        "--theme",
        "dark",
        "--no-pager",
        "--no-line-numbers",
        &path,
    ]);
    assert!(dark.stdout.starts_with(b"\x1b[95mfn\x1b[0m"));

    for args in [
        vec!["--theme", "mono", "--color", "always", "--no-pager", &path],
        vec!["--plain", "--color", "always", "--no-pager", &path],
    ] {
        let output = sl(&args);
        assert!(output.status.success());
        assert!(!output.stdout.contains(&0x1b), "{args:?}");
    }
}

#[test]
fn stdin_errors_limits_and_controls_have_defined_behavior() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_sl"))
        .args(["--plain", "--no-pager", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"{\"ok\":true}\n")
        .unwrap();
    let stdin = child.wait_with_output().unwrap();
    assert!(stdin.status.success());
    assert!(String::from_utf8_lossy(&stdin.stdout).contains("\"ok\": true"));

    let controls = fixture("controls.txt").display().to_string();
    let output = sl(&["--plain", "--no-pager", &controls]);
    assert!(output.status.success());
    assert!(
        output
            .stdout
            .windows(3)
            .any(|bytes| bytes == "␛".as_bytes())
    );
    assert!(!output.stdout.contains(&0x1b));
    assert!(!output.stdout.contains(&0x07));

    let long = fixture("long.txt").display().to_string();
    let output = sl(&["--plain", "--no-pager", "--max-bytes", "5", &long]);
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("abcde"));
    assert!(String::from_utf8_lossy(&output.stderr).contains("truncated at 5 bytes"));

    let invalid = fixture("invalid.txt").display().to_string();
    let output = sl(&["--plain", "--no-pager", "--type", "json", &invalid]);
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("rendered safely as text"));

    let missing = fixture("missing.txt").display().to_string();
    let output = sl(&["--plain", &missing, &long]);
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("could not open input"));
    assert_eq!(sl(&["--max-bytes", "0", &long]).status.code(), Some(1));
}
