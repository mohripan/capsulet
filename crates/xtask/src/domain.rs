//! `domain check`: the kernel and the runtime name no domain (ADR 0021 §5).
//!
//! Everything a domain needs lives in its pack: relations, phrasings and cues,
//! qualifier and reference declarations, obligations, authorities and tools.
//! This check reads that vocabulary from every pack under `packs/` and fails
//! when the source of the kernel, the runtime or the graph worker names any of
//! it in code: in a string literal, or as an identifier.
//!
//! What counts as naming:
//!
//! - a string literal that holds a pack's name, a tool's name, an authority, an
//!   obligation id, a relation's name, or a phrase of two words or more from a
//!   phrasing, a cue or a declaration, matched on word boundaries;
//! - a string literal that is exactly a one-word phrasing, cue or relation;
//! - an identifier that is a relation's or a pack's name.
//!
//! Comments, `#[cfg(test)]` modules and test files are not read: an example in
//! a doc comment, or a test that uses a pack, is not the kernel deciding
//! anything. A one-word cue that is also a word the platform uses for itself is
//! listed in [`SHARED`], each with why, so sharing one is a decision someone
//! made rather than one the check let through.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::release::split_root;

const USAGE: &str = "usage: cargo run -p capsulet-xtask -- domain check [--root <path>]";

/// The source a domain may not reach.
const DOMAIN_FREE: &[&str] = &[
    "crates/kernel/src",
    "crates/runtime/src",
    "crates/graph-worker/src",
];

/// Packs that are not a domain: the template a new pack starts from.
const NOT_A_DOMAIN: &[&str] = &["template"];

/// One-word terms a pack declares that are also the platform's own words, and
/// why the platform may use them.
const SHARED: &[(&str, &str)] = &[
    (
        "if",
        "the kernel's own hedge: a sentence that says `if` states nothing outright, in any domain",
    ),
    (
        "unless",
        "the kernel's own hedge: a sentence that says `unless` states nothing outright, in any \
         domain",
    ),
    ("may", "a month, which the kernel reads dates by"),
    (
        "proposed",
        "a node's result in a run's bundle: proposed and not checked",
    ),
    (
        "disputed",
        "a node's result in a run's bundle: a model said it does not hold",
    ),
];

pub(crate) fn run(arguments: &[String]) -> Result<(), String> {
    let (root, arguments) = split_root(arguments)?;
    if arguments != ["check"] {
        return Err(USAGE.to_string());
    }
    let vocabulary = vocabulary(&root.join("packs"))?;
    let findings = check(&root, &vocabulary)?;
    if findings.is_empty() {
        println!(
            "[domain] {} terms from {} packs; the kernel, the runtime and the graph worker name none of them",
            vocabulary.terms(),
            vocabulary.packs.len()
        );
        return Ok(());
    }
    Err(format!(
        "the kernel or the runtime names a domain:\n{}",
        findings.join("\n")
    ))
}

/// What the packs declare, by kind.
#[derive(Debug, Default)]
pub(crate) struct Vocabulary {
    pub(crate) packs: BTreeSet<String>,
    /// Pack names, tool names, authorities and obligation ids: matched
    /// anywhere in a literal.
    names: BTreeSet<String>,
    relations: BTreeSet<String>,
    /// Phrasings, cues and declarations, with their placeholders taken out.
    phrases: BTreeSet<String>,
}

impl Vocabulary {
    fn terms(&self) -> usize {
        self.names.len() + self.relations.len() + self.phrases.len()
    }
}

/// Reads the vocabulary of every pack under `packs`.
pub(crate) fn vocabulary(packs: &Path) -> Result<Vocabulary, String> {
    let mut vocabulary = Vocabulary::default();
    let mut directories: Vec<PathBuf> = fs::read_dir(packs)
        .map_err(|error| format!("{}: {error}", packs.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.join("pack.json").is_file())
        .collect();
    directories.sort();
    for directory in directories {
        let manifest: Value = read_json(&directory.join("pack.json"))?;
        let name = manifest["name"].as_str().unwrap_or_default().to_string();
        if name.is_empty() || NOT_A_DOMAIN.contains(&name.as_str()) {
            continue;
        }
        vocabulary.packs.insert(name.clone());
        vocabulary.names.insert(name);
        for entry in manifest["validators"].as_array().into_iter().flatten() {
            if let Some(path) = entry["registration"].as_str() {
                let registration = read_json(&directory.join(path))?;
                if let Some(tool) = registration["name"].as_str() {
                    vocabulary.names.insert(tool.to_string());
                }
            }
        }
        for path in manifest["obligations"].as_array().into_iter().flatten() {
            let Some(path) = path.as_str() else { continue };
            let contract = read_json(&directory.join(path))?;
            for obligation in contract["obligations"].as_array().into_iter().flatten() {
                for field in ["id", "authority", "waiver_authority"] {
                    if let Some(value) = obligation[field].as_str() {
                        vocabulary.names.insert(value.to_string());
                    }
                }
            }
        }
        for path in manifest["contracts"].as_array().into_iter().flatten() {
            let Some(path) = path.as_str() else { continue };
            let source = fs::read_to_string(directory.join(path))
                .map_err(|error| format!("{}: {error}", directory.join(path).display()))?;
            read_contract(&source, &mut vocabulary);
        }
    }
    Ok(vocabulary)
}

fn read_json(path: &Path) -> Result<Value, String> {
    let text = fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    serde_json::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))
}

/// The relations a memory contract declares, and every phrase it lists:
/// phrasings, cues, and the templates of its provisions, references and
/// definitions. Its review authority is a name.
fn read_contract(source: &str, vocabulary: &mut Vocabulary) {
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("relation ") {
            let name = rest.trim_end_matches(':').trim();
            if !name.is_empty() {
                vocabulary.relations.insert(name.to_string());
            }
        } else if let Some(item) = trimmed.strip_prefix("- ") {
            let phrase = phrase_of(item);
            if !phrase.is_empty() {
                vocabulary.phrases.insert(phrase);
            }
        } else if let Some(authority) = trimmed.strip_prefix("reading_authority:") {
            vocabulary.names.insert(authority.trim().to_string());
        }
    }
}

/// A listed phrase with its placeholders, quotes and punctuation taken out,
/// lower-cased, its words single-spaced.
fn phrase_of(item: &str) -> String {
    let mut text = item.to_string();
    for placeholder in ["{from}", "{to}", "{ref}", "{term}"] {
        text = text.replace(placeholder, " ");
    }
    text.chars()
        .map(|character| {
            if character.is_alphanumeric() || character == '\'' {
                character.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Every place the domain-free source names the vocabulary.
pub(crate) fn check(root: &Path, vocabulary: &Vocabulary) -> Result<Vec<String>, String> {
    let mut findings = Vec::new();
    for directory in DOMAIN_FREE {
        let mut files = Vec::new();
        rust_files(&root.join(directory), &mut files)?;
        files.sort();
        for file in files {
            let source = fs::read_to_string(&file)
                .map_err(|error| format!("{}: {error}", file.display()))?;
            let shown = file
                .strip_prefix(root)
                .unwrap_or(&file)
                .display()
                .to_string()
                .replace('\\', "/");
            for token in tokens(&source) {
                if let Some(what) = names(vocabulary, &token) {
                    findings.push(format!("{shown}:{}: {what}", token.line));
                }
            }
        }
    }
    Ok(findings)
}

/// The Rust files under `directory` that are not tests.
fn rust_files(directory: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in
        fs::read_dir(directory).map_err(|error| format!("{}: {error}", directory.display()))?
    {
        let path = entry.map_err(|error| error.to_string())?.path();
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        if path.is_dir() {
            if name != "tests" {
                rust_files(&path, files)?;
            }
        } else if path.extension().is_some_and(|extension| extension == "rs")
            && name != "tests.rs"
            && !name.ends_with("_tests.rs")
        {
            files.push(path);
        }
    }
    Ok(())
}

/// A piece of code the check reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Token {
    pub(crate) line: usize,
    pub(crate) kind: TokenKind,
    pub(crate) text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TokenKind {
    Literal,
    Identifier,
}

/// What `token` names, if it names anything a pack declares.
fn names(vocabulary: &Vocabulary, token: &Token) -> Option<String> {
    let shared = |term: &str| SHARED.iter().any(|(word, _)| *word == term);
    match token.kind {
        TokenKind::Identifier => {
            let text = token.text.as_str();
            if vocabulary.relations.contains(text) && text.contains('_') {
                return Some(format!("`{text}` is the relation of that name"));
            }
            vocabulary
                .packs
                .iter()
                .find(|pack| pack.replace('-', "_") == text)
                .map(|pack| format!("`{text}` is pack {pack}"))
        }
        TokenKind::Literal => {
            let lower = token.text.to_lowercase();
            let words = phrase_of(&token.text);
            for name in &vocabulary.names {
                if contains_term(&lower, &name.to_lowercase()) {
                    return Some(format!("{:?} names `{name}`", token.text));
                }
            }
            for relation in &vocabulary.relations {
                let named = if relation.contains('_') {
                    contains_term(&lower, relation)
                } else {
                    lower.trim() == relation && !shared(relation)
                };
                if named {
                    return Some(format!("{:?} names the relation `{relation}`", token.text));
                }
            }
            for phrase in &vocabulary.phrases {
                let named = if phrase.contains(' ') {
                    contains_term(&words, phrase)
                } else {
                    words == *phrase && !shared(phrase)
                };
                if named {
                    return Some(format!("{:?} names the phrase \"{phrase}\"", token.text));
                }
            }
            None
        }
    }
}

/// Whether `term` is in `text` with no letter or digit on either side.
fn contains_term(text: &str, term: &str) -> bool {
    if term.is_empty() {
        return false;
    }
    let edge = |character: Option<char>| character.is_none_or(|c| !c.is_alphanumeric() && c != '_');
    let mut start = 0;
    while let Some(found) = text[start..].find(term) {
        let at = start + found;
        let before = text[..at].chars().next_back();
        let after = text[at + term.len()..].chars().next();
        if edge(before) && edge(after) {
            return true;
        }
        start = at + term.len().max(1);
        while !text.is_char_boundary(start) {
            start += 1;
        }
    }
    false
}

/// The string literals and identifiers of `source`, outside comments and
/// `#[cfg(test)]` modules.
pub(crate) fn tokens(source: &str) -> Vec<Token> {
    let characters: Vec<char> = source.chars().collect();
    let mut found = Vec::new();
    let mut index = 0;
    let mut line = 1;
    let mut test_attribute = false;
    while index < characters.len() {
        let character = characters[index];
        let next = characters.get(index + 1).copied();
        match character {
            '\n' => {
                line += 1;
                index += 1;
            }
            '/' if next == Some('/') => {
                while index < characters.len() && characters[index] != '\n' {
                    index += 1;
                }
            }
            '/' if next == Some('*') => {
                let mut depth = 0;
                while index < characters.len() {
                    if characters[index] == '/' && characters.get(index + 1) == Some(&'*') {
                        depth += 1;
                        index += 2;
                    } else if characters[index] == '*' && characters.get(index + 1) == Some(&'/') {
                        depth -= 1;
                        index += 2;
                        if depth == 0 {
                            break;
                        }
                    } else {
                        if characters[index] == '\n' {
                            line += 1;
                        }
                        index += 1;
                    }
                }
            }
            '"' => {
                let start_line = line;
                let (text, end) = string_literal(&characters, index + 1, None, &mut line);
                found.push(Token {
                    line: start_line,
                    kind: TokenKind::Literal,
                    text,
                });
                index = end;
            }
            'r' if matches!(next, Some('"' | '#')) && raw_string_start(&characters, index + 1) => {
                let mut hashes = 0;
                let mut cursor = index + 1;
                while characters.get(cursor) == Some(&'#') {
                    hashes += 1;
                    cursor += 1;
                }
                let start_line = line;
                let (text, end) = string_literal(&characters, cursor + 1, Some(hashes), &mut line);
                found.push(Token {
                    line: start_line,
                    kind: TokenKind::Literal,
                    text,
                });
                index = end;
            }
            '\'' => index = past_character(&characters, index),
            '#' if source_at(&characters, index, "#[cfg(test)]") => {
                test_attribute = true;
                index += "#[cfg(test)]".len();
            }
            '{' if test_attribute => {
                test_attribute = false;
                index = skip_block(&characters, index, &mut line);
            }
            ';' if test_attribute => {
                test_attribute = false;
                index += 1;
            }
            _ if character.is_alphabetic() || character == '_' => {
                let start = index;
                while index < characters.len()
                    && (characters[index].is_alphanumeric() || characters[index] == '_')
                {
                    index += 1;
                }
                let word: String = characters[start..index].iter().collect();
                found.push(Token {
                    line,
                    kind: TokenKind::Identifier,
                    text: word,
                });
            }
            _ => index += 1,
        }
    }
    found
}

fn source_at(characters: &[char], index: usize, text: &str) -> bool {
    text.chars()
        .enumerate()
        .all(|(offset, wanted)| characters.get(index + offset) == Some(&wanted))
}

fn raw_string_start(characters: &[char], mut index: usize) -> bool {
    while characters.get(index) == Some(&'#') {
        index += 1;
    }
    characters.get(index) == Some(&'"')
}

/// Past a character literal at `index`, or past the quote of a lifetime,
/// whose name the next turn reads as an identifier.
fn past_character(characters: &[char], index: usize) -> usize {
    if characters.get(index + 1) == Some(&'\\') {
        let mut cursor = index + 2;
        while cursor < characters.len() && characters[cursor] != '\'' {
            cursor += 1;
        }
        cursor + 1
    } else if characters.get(index + 2) == Some(&'\'') {
        index + 3
    } else {
        index + 1
    }
}

/// A string literal's text from `index`, just past its opening quote, and
/// where it ends. `raw` is a raw string's count of `#`; a raw string has no
/// escapes.
fn string_literal(
    characters: &[char],
    mut index: usize,
    raw: Option<usize>,
    line: &mut usize,
) -> (String, usize) {
    let hashes = raw.unwrap_or(0);
    let mut text = String::new();
    while index < characters.len() {
        let character = characters[index];
        if character == '\n' {
            *line += 1;
        }
        if character == '\\' && raw.is_none() {
            if let Some(escaped) = characters.get(index + 1) {
                text.push(match escaped {
                    'n' | '\n' => ' ',
                    other => *other,
                });
            }
            index += 2;
            continue;
        }
        if character == '"'
            && (index + 1..=index + hashes).all(|at| characters.get(at) == Some(&'#'))
        {
            return (text, index + 1 + hashes);
        }
        text.push(character);
        index += 1;
    }
    (text, index)
}

/// Skips the block whose `{` is at `index`, returning where it ends.
fn skip_block(characters: &[char], mut index: usize, line: &mut usize) -> usize {
    let mut depth = 0usize;
    while index < characters.len() {
        match characters[index] {
            '\n' => *line += 1,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return index + 1;
                }
            }
            '"' => {
                let (_, end) = string_literal(characters, index + 1, None, line);
                index = end;
                continue;
            }
            '\'' => {
                index = past_character(characters, index);
                continue;
            }
            _ => {}
        }
        index += 1;
    }
    index
}

#[cfg(test)]
mod tests {
    use super::*;

    fn legal() -> Vocabulary {
        let mut vocabulary = Vocabulary::default();
        vocabulary.packs.insert("contract-clauses".to_string());
        vocabulary.names.insert("contract-clauses".to_string());
        vocabulary.names.insert("review:legal".to_string());
        vocabulary.relations.insert("may_terminate".to_string());
        vocabulary.relations.insert("may".to_string());
        vocabulary.phrases.insert("subject to".to_string());
        vocabulary.phrases.insert("unless".to_string());
        vocabulary.phrases.insert("notwithstanding".to_string());
        vocabulary
    }

    fn found(source: &str) -> Vec<String> {
        let vocabulary = legal();
        tokens(source)
            .iter()
            .filter_map(|token| names(&vocabulary, token))
            .collect()
    }

    #[test]
    fn a_cue_a_relation_a_pack_or_an_authority_named_in_code_is_found() {
        assert_eq!(found("const CUE: &str = \"Subject to\";").len(), 1);
        assert_eq!(found("fn may_terminate() {}").len(), 1);
        assert_eq!(found("let pack = \"contract-clauses\";").len(), 1);
        assert_eq!(found("let who = r#\"needs review:legal\"#;").len(), 1);
        assert_eq!(found("if text == \"notwithstanding\" {}").len(), 1);
        // A cue's words in snake case are still the cue.
        assert_eq!(found("let key = \"subject_to\";").len(), 1);
        assert_eq!(
            found("let words = \"read it subject to section 3\";").len(),
            1
        );
    }

    #[test]
    fn comments_tests_and_words_inside_other_words_are_not_naming() {
        assert_eq!(
            found("// a clause subject to Section 8.3\n/* \"unless\" */"),
            Vec::<String>::new()
        );
        assert_eq!(
            found("#[cfg(test)]\nmod tests {\n    const CUE: &str = \"subject to\";\n}\n"),
            Vec::<String>::new()
        );
        assert_eq!(
            found("let text = \"notwithstandingly\"; let maybe = \"may be\";"),
            Vec::<String>::new()
        );
        // A field named for a category of cue is not the cue, and a one-word
        // term the platform shares is listed, with why.
        assert_eq!(
            found("let subject_to = vec![]; let hedge = \"unless\";"),
            Vec::<String>::new()
        );
    }

    #[test]
    fn the_reference_packs_vocabulary_reads_and_the_kernel_and_runtime_name_none_of_it() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let vocabulary = vocabulary(&root.join("packs")).expect("the packs read");
        for pack in ["contract-clauses", "code-changes", "infrastructure-policy"] {
            assert!(vocabulary.packs.contains(pack), "{pack}");
        }
        assert!(vocabulary.relations.contains("may_terminate"));
        assert!(vocabulary.phrases.contains("subject to"));
        assert!(vocabulary.names.contains("review:security"));
        assert_eq!(
            check(&root, &vocabulary).expect("reads"),
            Vec::<String>::new()
        );
    }
}
