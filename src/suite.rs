//! Generate an in-memory Hurl suite and compare it with a directory on disk.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::error::{Error, Result};
use crate::render;
use crate::spec::{self, Document};

/// A deterministic set of files `spechurl generate` would write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suite {
    /// File name to UTF-8 contents. Names are flat (`get_pets.hurl`, `README.md`).
    pub files: BTreeMap<String, String>,
}

/// Generate a suite from an OpenAPI file on disk.
pub fn generate_suite(spec_path: &Path) -> Result<Suite> {
    let document = spec::load_path(spec_path)?;
    suite_from_document(&document)
}

/// Generate a suite from an OpenAPI document already in memory.
///
/// `source_name` is the file name recorded in generated comments. It should be
/// a file name, not a directory path, so the output stays stable.
pub fn generate_from_str(source_name: &str, source: &str) -> Result<Suite> {
    let document = spec::load_str(Path::new(source_name), source_name, source)?;
    suite_from_document(&document)
}

fn suite_from_document(document: &Document) -> Result<Suite> {
    let rendered = render::render_suite(document)?;
    let readme = render::render_readme(document, &rendered);
    let mut files = BTreeMap::new();
    for file in &rendered {
        if !is_safe_file_name(&file.name) {
            return Err(Error::spec(format!(
                "refusing to write unsafe file name '{}'",
                file.name
            )));
        }
        files.insert(file.name.clone(), file.body.clone());
    }
    files.insert("README.md".to_string(), readme);
    Ok(Suite { files })
}

fn is_safe_file_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains('\0')
}

/// Write every file in `suite` into `out_dir`, creating the directory if needed.
///
/// Existing files with the same names are overwritten. Files spechurl does not
/// generate are left in place; `check_suite` reports them when they are `.hurl`
/// files.
pub fn write_suite(suite: &Suite, out_dir: &Path) -> Result<()> {
    fs::create_dir_all(out_dir).map_err(|source| Error::Write {
        path: out_dir.to_path_buf(),
        source,
    })?;
    for (name, contents) in &suite.files {
        if !is_safe_file_name(name) {
            return Err(Error::spec(format!(
                "refusing to write unsafe file name '{name}'"
            )));
        }
        let path = out_dir.join(name);
        fs::write(&path, contents).map_err(|source| Error::Write { path, source })?;
    }
    Ok(())
}

/// Regenerate the suite and compare it to `dir`.
///
/// Returns the number of compared files on success.
pub fn check_suite(spec_path: &Path, dir: &Path) -> Result<usize> {
    let generated = generate_suite(spec_path)?;
    if !dir.exists() {
        return Err(Error::drift(format!(
            "Hurl suite is out of sync with '{}':\n  directory '{}' does not exist\nRegenerate with: spechurl generate {} --out {}",
            spec_path.display(),
            dir.display(),
            spec_path.display(),
            dir.display()
        )));
    }
    if !dir.is_dir() {
        return Err(Error::spec(format!(
            "'{}' is not a directory",
            dir.display()
        )));
    }

    let mut missing = Vec::new();
    let mut mismatched = Vec::new();
    for (name, expected) in &generated.files {
        let path = dir.join(name);
        match fs::read(&path) {
            Ok(bytes) => match String::from_utf8(bytes) {
                Ok(actual) => {
                    if normalize(&actual) != normalize(expected) {
                        mismatched.push(format!(
                            "mismatch: {name} ({})",
                            first_difference(&normalize(&actual), &normalize(expected))
                        ));
                    }
                }
                Err(_) => mismatched.push(format!("mismatch: {name} (not valid UTF-8)")),
            },
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                missing.push(format!("missing: {name}"));
            }
            Err(source) => {
                return Err(Error::Read { path, source });
            }
        }
    }

    let mut unexpected = Vec::new();
    let entries = fs::read_dir(dir).map_err(|source| Error::Read {
        path: dir.to_path_buf(),
        source,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| Error::Read {
            path: dir.to_path_buf(),
            source,
        })?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if !name.ends_with(".hurl") {
            continue;
        }
        if !generated.files.contains_key(name) {
            unexpected.push(format!("unexpected: {name}"));
        }
    }

    unexpected.sort();
    let mut problems = Vec::new();
    problems.extend(missing);
    problems.extend(mismatched);
    problems.extend(unexpected);
    if problems.is_empty() {
        return Ok(generated.files.len());
    }

    let mut message = format!("Hurl suite is out of sync with '{}':", spec_path.display());
    for problem in problems.iter().take(100) {
        message.push_str("\n  ");
        message.push_str(problem);
    }
    if problems.len() > 100 {
        message.push_str(&format!("\n  ... and {} more", problems.len() - 100));
    }
    message.push_str(&format!(
        "\nRegenerate with: spechurl generate {} --out {}",
        spec_path.display(),
        dir.display()
    ));
    Err(Error::drift(message))
}

fn normalize(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

fn first_difference(actual: &str, expected: &str) -> String {
    let actual_lines: Vec<&str> = actual.split('\n').collect();
    let expected_lines: Vec<&str> = expected.split('\n').collect();
    let shared = actual_lines.len().min(expected_lines.len());
    for index in 0..shared {
        if actual_lines[index] != expected_lines[index] {
            return format!("first difference at line {}", index + 1);
        }
    }
    format!(
        "generated file has {} lines, committed file has {} lines",
        expected_lines.len(),
        actual_lines.len()
    )
}
