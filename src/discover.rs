use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use glob::Pattern as GlobPattern;
use nucleo_matcher::{Config, Matcher, Utf32Str};
use regex::Regex;
use serde_json::Value as JsonValue;
use walkdir::{DirEntry, WalkDir};

use crate::cli::Cli;
use crate::diagnostic::Reporter;
use crate::error::AppError;

#[derive(Clone, Debug, PartialEq)]
pub enum Input {
    Stdin,
    File(PathBuf),
    /// A file selected by `--path-spec`, with its named captures attached.
    Matched {
        path: PathBuf,
        captures: Arc<JsonValue>,
    },
}

pub(crate) const DEFAULT_IGNORED_DIRS: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    "target",
    "node_modules",
    ".venv",
    "__pycache__",
    ".trae",
];

/// Expand CLI path arguments into a deterministic, de-duplicated input list.
///
/// Ordering contract (P1): command-line argument order is preserved; within each
/// argument, files are sorted lexicographically by their normalized relative path.
/// `-` is inserted at its argument position and never sorted.
pub fn discover(cli: &Cli, reporter: &mut Reporter<'_>) -> Result<Vec<Input>, AppError> {
    if let Some(spec) = &cli.path_spec {
        return crate::pathspec::discover(spec, cli, reporter);
    }
    if cli.paths.is_empty() {
        return Ok(vec![Input::Stdin]);
    }

    let excludes = cli
        .exclude
        .iter()
        .map(|pattern| GlobPattern::new(pattern))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| AppError::Usage(format!("invalid --exclude glob: {error}")))?;

    let path_regex = cli
        .path_regex
        .iter()
        .map(|pattern| {
            Regex::new(pattern).map_err(|error| {
                AppError::Query(format!("invalid --path-regex `{pattern}`: {error}"))
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    let mut matcher = cli
        .fuzzy
        .as_ref()
        .map(|_| Matcher::new(Config::DEFAULT.match_paths()));

    let mut inputs = Vec::new();
    let mut seen = HashSet::new();
    let mut any_matched = false;

    for spec in &cli.paths {
        if spec == "-" {
            inputs.push(Input::Stdin);
            any_matched = true;
            continue;
        }
        // --root re-anchors relative paths; absolute paths are untouched.
        let spec = match &cli.root {
            Some(root) if Path::new(spec).is_relative() => {
                Path::new(root).join(spec).to_string_lossy().into_owned()
            }
            _ => spec.clone(),
        };
        let candidates = expand(&spec, cli, &excludes, reporter)?;
        if candidates.is_empty() {
            continue;
        }
        any_matched = true;
        for (path, relative) in candidates {
            if let Some(matcher) = matcher.as_mut() {
                let query = cli.fuzzy.as_deref().unwrap_or_default();
                if !fuzzy_hit(matcher, &relative, query) {
                    continue;
                }
            }
            // --path-regex is a post-filter over the normalized relative path;
            // every pattern must match (AND), unlike --exclude (OR of excluded).
            if !path_regex.is_empty() && !path_regex.iter().all(|re| re.is_match(&relative)) {
                continue;
            }
            let key = path.canonicalize().unwrap_or_else(|_| path.clone());
            if seen.insert(key) {
                inputs.push(Input::File(path));
            }
        }
    }

    if !any_matched {
        reporter.warn("no input files matched");
    }
    Ok(inputs)
}

fn expand(
    spec: &str,
    cli: &Cli,
    excludes: &[GlobPattern],
    reporter: &mut Reporter<'_>,
) -> Result<Vec<(PathBuf, String)>, AppError> {
    let path = Path::new(spec);
    if has_glob_metacharacter(spec) {
        expand_glob(spec, excludes)
    } else if path.is_dir() {
        walk_directory(path, cli, excludes, reporter)
    } else if path.exists() {
        let relative = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| normalize_relative(path));
        Ok(vec![(path.to_path_buf(), relative)])
    } else {
        Err(AppError::Runtime(format!(
            "{}: cannot open: no such file or directory",
            path.display()
        )))
    }
}

fn has_glob_metacharacter(spec: &str) -> bool {
    spec.contains(['*', '?', '['])
}

fn expand_glob(spec: &str, excludes: &[GlobPattern]) -> Result<Vec<(PathBuf, String)>, AppError> {
    let base = glob_base(spec);
    let mut found = Vec::new();
    let paths = glob::glob(spec)
        .map_err(|error| AppError::Usage(format!("invalid glob `{spec}`: {error}")))?;
    for entry in paths {
        let path = entry.map_err(|error| {
            AppError::Runtime(format!(
                "{}: cannot read glob match: {error}",
                error.path().display()
            ))
        })?;
        if !path.is_file() {
            continue;
        }
        let relative = normalize_relative(path.strip_prefix(&base).unwrap_or(&path));
        if is_excluded(&relative, excludes) {
            continue;
        }
        found.push((path, relative));
    }
    found.sort_by(|left, right| left.1.cmp(&right.1));
    Ok(found)
}

fn glob_base(spec: &str) -> PathBuf {
    let wildcard = spec.find(['*', '?', '[']).unwrap_or(spec.len());
    let prefix = &spec[..wildcard];
    let directory = prefix
        .rfind(['/', '\\'])
        .map(|index| &prefix[..index])
        .unwrap_or(".");
    if directory.is_empty() {
        PathBuf::from(".")
    } else {
        PathBuf::from(directory)
    }
}

fn walk_directory(
    directory: &Path,
    cli: &Cli,
    excludes: &[GlobPattern],
    reporter: &mut Reporter<'_>,
) -> Result<Vec<(PathBuf, String)>, AppError> {
    let depth = cli
        .max_depth
        .map(|value| value.saturating_add(1))
        .unwrap_or(usize::MAX);
    let iterator = WalkDir::new(directory)
        .max_depth(depth)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| !prune(entry, directory, cli, excludes));

    let mut found = Vec::new();
    for entry in iterator {
        match entry {
            Ok(entry) => {
                if !entry.file_type().is_file() {
                    continue;
                }
                let path = entry.path().to_path_buf();
                let relative = normalize_relative(
                    entry.path().strip_prefix(directory).unwrap_or(entry.path()),
                );
                found.push((path, relative));
            }
            Err(error) => {
                let location = error
                    .path()
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| "<unknown>".to_owned());
                let message = format!("cannot traverse `{location}`: {error}");
                if cli.strict {
                    return Err(AppError::Runtime(message));
                }
                reporter.warn(&message);
            }
        }
    }
    found.sort_by(|left, right| left.1.cmp(&right.1));
    Ok(found)
}

fn prune(entry: &DirEntry, base: &Path, cli: &Cli, excludes: &[GlobPattern]) -> bool {
    if entry.depth() == 0 {
        return false;
    }
    if !cli.no_default_ignore && entry.file_type().is_dir() {
        let name = entry.file_name().to_string_lossy();
        if DEFAULT_IGNORED_DIRS.contains(&name.as_ref()) {
            return true;
        }
    }
    let relative = normalize_relative(entry.path().strip_prefix(base).unwrap_or(entry.path()));
    is_excluded(&relative, excludes)
}

pub(crate) fn is_excluded(relative: &str, excludes: &[GlobPattern]) -> bool {
    excludes.iter().any(|pattern| pattern.matches(relative))
}

fn normalize_relative(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// `fuzzy_match` already requires every needle character to be present in order,
/// so a positive score is the documented "hit" condition. Scores are used only
/// for filtering; output order stays lexicographic for reproducibility (P1).
pub(crate) fn fuzzy_hit(matcher: &mut Matcher, haystack: &str, needle: &str) -> bool {
    let mut haystack_buf = Vec::new();
    let mut needle_buf = Vec::new();
    let haystack = Utf32Str::new(haystack, &mut haystack_buf);
    let needle = Utf32Str::new(needle, &mut needle_buf);
    matcher
        .fuzzy_match(haystack, needle)
        .is_some_and(|score| score > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::ErrorFormat;
    use std::fs;
    use tempfile::tempdir;

    fn cli(args: &[&str]) -> Cli {
        use clap::Parser;
        Cli::try_parse_from(args).unwrap()
    }

    fn discover_for_test(cli: &Cli) -> (Vec<Input>, String) {
        let mut stderr = Vec::new();
        let inputs = {
            let mut reporter = Reporter::new(&mut stderr, ErrorFormat::Text, false);
            discover(cli, &mut reporter).unwrap()
        };
        (inputs, String::from_utf8(stderr).unwrap())
    }

    fn write(dir: &Path, relative: &str, content: &str) {
        let path = dir.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    #[test]
    fn discovers_files_in_stable_lexicographic_order() {
        let dir = tempdir().unwrap();
        write(dir.path(), "b.json", "{}");
        write(dir.path(), "a.json", "{}");
        write(dir.path(), "sub/c.json", "{}");
        let cli = cli(&["ux", "-e", "text()", dir.path().to_str().unwrap()]);
        let (inputs, _) = discover_for_test(&cli);
        let names: Vec<_> = inputs
            .iter()
            .map(|input| match input {
                Input::File(path) | Input::Matched { path, .. } => {
                    path.file_name().unwrap().to_string_lossy().into_owned()
                }
                Input::Stdin => "<stdin>".to_owned(),
            })
            .collect();
        assert_eq!(names, vec!["a.json", "b.json", "c.json"]);
    }

    #[test]
    fn prunes_default_ignored_directories_during_walk() {
        let dir = tempdir().unwrap();
        write(dir.path(), "keep.txt", "x");
        write(dir.path(), "target/generated.txt", "x");
        write(dir.path(), ".git/config", "x");
        let cli = cli(&["ux", "-e", "text()", dir.path().to_str().unwrap()]);
        let (inputs, _) = discover_for_test(&cli);
        assert_eq!(inputs.len(), 1);
        match &inputs[0] {
            Input::File(path) | Input::Matched { path, .. } => {
                assert!(path.ends_with("keep.txt"));
            }
            Input::Stdin => panic!("unexpected stdin input"),
        }
    }

    #[test]
    fn respects_max_depth_and_exclude() {
        let dir = tempdir().unwrap();
        write(dir.path(), "top.txt", "x");
        write(dir.path(), "sub/deep.txt", "x");
        let cli = cli(&[
            "ux",
            "-e",
            "text()",
            "--max-depth",
            "0",
            "--exclude",
            "top.*",
            dir.path().to_str().unwrap(),
        ]);
        let (inputs, _) = discover_for_test(&cli);
        assert!(inputs.is_empty());
    }

    #[test]
    fn fuzzy_filters_by_relative_path() {
        // Verify the matcher directly: the hit predicate is the whole contract.
        let mut matcher = Matcher::new(Config::DEFAULT.match_paths());
        assert!(fuzzy_hit(
            &mut matcher,
            "config/prodconfig.yaml",
            "prodconfig"
        ));
        assert!(!fuzzy_hit(
            &mut matcher,
            "config/prodconfig.yaml",
            "zzzzzzzz"
        ));
    }

    #[test]
    fn glob_expansion_is_sorted_and_deduplicated() {
        let dir = tempdir().unwrap();
        write(dir.path(), "b.json", "{}");
        write(dir.path(), "a.json", "{}");
        let pattern = format!("{}/*.json", dir.path().to_str().unwrap());
        let cli = cli(&["ux", "-e", "text()", &pattern]);
        let (inputs, _) = discover_for_test(&cli);
        assert_eq!(inputs.len(), 2);
    }

    #[test]
    fn empty_glob_is_success_with_warning() {
        let dir = tempdir().unwrap();
        let pattern = format!("{}/*.none", dir.path().to_str().unwrap());
        let cli = cli(&["ux", "-e", "text()", &pattern]);
        let (inputs, stderr) = discover_for_test(&cli);
        assert!(inputs.is_empty());
        assert!(stderr.contains("no input files matched"));
    }

    #[test]
    fn quiet_suppresses_discovery_warning() {
        let dir = tempdir().unwrap();
        let pattern = format!("{}/*.none", dir.path().to_str().unwrap());
        let cli = cli(&["ux", "-e", "text()", &pattern]);
        let mut stderr = Vec::new();
        {
            let mut reporter = Reporter::new(&mut stderr, ErrorFormat::Text, true);
            let inputs = discover(&cli, &mut reporter).unwrap();
            assert!(inputs.is_empty());
        }
        assert!(stderr.is_empty());
    }
}
