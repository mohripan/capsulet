//! What a cited clause leaves out.
//!
//! An answer can quote a contract byte for byte and still misread it, because
//! the clause it quotes does not stand alone. "Either party may terminate on
//! thirty days' notice" reads one way, and differently once "subject to
//! Section 12" or "notwithstanding Section 9" elsewhere is read with it. A
//! defined term such as "Affiliate" means what the definition says, not what
//! the word usually does.
//!
//! Whether a reading of the clause is right is not decidable here. Whether the
//! derivation read the clause with what could change it is. A contract declares
//! how its documents mark out their provisions, how one refers to another, which
//! cues make a reference a qualification, which make a sentence conditional, and
//! how a term is defined. This module finds, for a cited span:
//!
//! - **qualifications:** a declared condition in the cited sentence, a
//!   `subject_to` cue before a reference in it, or an `overrides` cue elsewhere
//!   in the document before a reference to the cited provision. A qualified
//!   clause does not state its relation outright, so the citation only mentions
//!   it, and getting to the relation is a reading;
//! - **what it depends on:** each qualifying, referenced or defining provision
//!   the derivation does not also read the span with.
//!
//! Every cue, template and term comes from the contract, pinned in the
//! snapshot. The kernel learns no legal vocabulary: a legal pack supplies legal
//! ones, and a contract that declares none decides exactly as before.

use serde::{Deserialize, Serialize};
use unicode_segmentation::UnicodeSegmentation;

/// How a contract marks out a document's provisions and what qualifies them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextPolicy {
    /// Heading templates, such as `"Section {ref}."`, matched at a line start.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provisions: Vec<String>,
    /// Reference templates, such as `"Section {ref}"`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub references: Vec<String>,
    /// Cues that, right before a reference, make the referenced provision
    /// qualify the sentence they are in: `"subject to"`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subject_to: Vec<String>,
    /// Cues that, right before a reference, make the sentence they are in
    /// qualify the referenced provision: `"notwithstanding"`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub overrides: Vec<String>,
    /// Cues that make the sentence they are in conditional: `"provided that"`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conditions: Vec<String>,
    /// Definition templates, such as `"\"{term}\" means"`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub definitions: Vec<String>,
}

impl ContextPolicy {
    /// Whether nothing is declared, in which case nothing here runs.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.provisions.is_empty()
            && self.references.is_empty()
            && self.subject_to.is_empty()
            && self.overrides.is_empty()
            && self.conditions.is_empty()
            && self.definitions.is_empty()
    }
}

/// What reading a cited span needs, beyond the span.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Context {
    /// Why the cited clause does not state its relation outright.
    pub qualified: Vec<String>,
    /// A residual for each provision or definition it depends on and the
    /// derivation did not read it with.
    pub uncited: Vec<String>,
}

/// A text with each character's plain, lower-case form and byte offset, so a
/// template can be matched without folding away the offsets a span is in.
struct Shadow<'a> {
    text: &'a str,
    chars: Vec<char>,
    offsets: Vec<usize>,
}

impl<'a> Shadow<'a> {
    fn new(text: &'a str) -> Self {
        let mut chars = Vec::with_capacity(text.len());
        let mut offsets = Vec::with_capacity(text.len() + 1);
        for (at, character) in text.char_indices() {
            offsets.push(at);
            chars.push(lower(character));
        }
        offsets.push(text.len());
        Self {
            text,
            chars,
            offsets,
        }
    }

    /// The character index a byte offset starts, rounding down.
    fn index_of(&self, byte: usize) -> usize {
        self.offsets.partition_point(|&at| at < byte)
    }

    fn is_word(&self, index: usize) -> bool {
        self.chars.get(index).is_some_and(|c| c.is_alphanumeric())
    }

    /// Whether `needle` occurs at `index`, at word boundaries.
    fn literal_at(&self, needle: &[char], index: usize) -> bool {
        if needle.is_empty() || index + needle.len() > self.chars.len() {
            return false;
        }
        if self.chars[index..index + needle.len()] != *needle {
            return false;
        }
        let starts_word = needle.first().is_some_and(|c| c.is_alphanumeric());
        let ends_word = needle.last().is_some_and(|c| c.is_alphanumeric());
        (!starts_word || index == 0 || !self.is_word(index - 1))
            && (!ends_word || !self.is_word(index + needle.len()))
    }

    /// Every character index in `from..to` at which `needle` occurs.
    fn find_all(&self, needle: &[char], from: usize, to: usize) -> Vec<usize> {
        (from..to.min(self.chars.len()))
            .filter(|&index| index + needle.len() <= to && self.literal_at(needle, index))
            .collect()
    }

    /// The reference id that starts at `index`: letters and digits, with dots
    /// between them. Returns its end and its text.
    fn ref_at(&self, index: usize) -> Option<(usize, String)> {
        let mut end = index;
        loop {
            let run = self.chars[end..]
                .iter()
                .take_while(|c| c.is_alphanumeric())
                .count();
            if run == 0 {
                break;
            }
            end += run;
            if self.chars.get(end) == Some(&'.') && self.is_word(end + 1) {
                end += 1;
            } else {
                break;
            }
        }
        (end > index).then(|| {
            (
                end,
                self.text[self.offsets[index]..self.offsets[end]].to_string(),
            )
        })
    }
}

fn lower(character: char) -> char {
    let plain = match character {
        '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}' | '\u{2032}' | '\u{02BC}' => '\'',
        '\u{201C}' | '\u{201D}' | '\u{201E}' | '\u{201F}' | '\u{2033}' => '"',
        '\u{2010}' | '\u{2011}' | '\u{2012}' | '\u{2013}' | '\u{2014}' | '\u{2015}'
        | '\u{2212}' => '-',
        '\u{00A0}' | '\t' => ' ',
        other => other,
    };
    let mut lowered = plain.to_lowercase();
    match (lowered.next(), lowered.next()) {
        (Some(single), None) => single,
        _ => plain,
    }
}

fn chars(text: &str) -> Vec<char> {
    text.chars().map(lower).collect()
}

/// A template's literal text either side of its placeholder.
fn template(text: &str, placeholder: &str) -> Option<(Vec<char>, Vec<char>)> {
    let (prefix, suffix) = text.split_once(placeholder)?;
    (!suffix.contains(placeholder)).then(|| (chars(prefix), chars(suffix)))
}

/// A reference found in the text.
struct Reference {
    /// Character index the template starts at.
    start: usize,
    target: String,
}

fn references(
    shadow: &Shadow<'_>,
    policy: &ContextPolicy,
    from: usize,
    to: usize,
) -> Vec<Reference> {
    let mut found = Vec::new();
    for raw in &policy.references {
        let Some((prefix, suffix)) = template(raw, "{ref}") else {
            continue;
        };
        for start in from..to {
            if !prefix.is_empty() && !shadow.literal_at(&prefix, start) {
                continue;
            }
            let Some((end, target)) = shadow.ref_at(start + prefix.len()) else {
                continue;
            };
            if end > to || (!suffix.is_empty() && !shadow.literal_at(&suffix, end)) {
                continue;
            }
            if prefix.is_empty() && start > 0 && shadow.is_word(start - 1) {
                continue;
            }
            found.push(Reference { start, target });
        }
    }
    found.sort_by_key(|reference| reference.start);
    found.dedup_by_key(|reference| reference.start);
    found
}

/// Whether one of `cues` ends right before character `index`, past spaces.
fn cue_before(shadow: &Shadow<'_>, cues: &[String], index: usize) -> Option<String> {
    let mut end = index;
    while end > 0 && shadow.chars[end - 1] == ' ' {
        end -= 1;
    }
    cues.iter().find_map(|cue| {
        let needle = chars(cue.trim());
        (end >= needle.len() && shadow.literal_at(&needle, end - needle.len()))
            .then(|| cue.trim().to_string())
    })
}

/// A provision: its reference id and the bytes it runs over.
struct Provision {
    target: String,
    start: usize,
    end: usize,
}

fn provisions(shadow: &Shadow<'_>, policy: &ContextPolicy) -> Vec<Provision> {
    let templates: Vec<(Vec<char>, Vec<char>)> = policy
        .provisions
        .iter()
        .filter_map(|raw| template(raw, "{ref}"))
        .collect();
    let mut found: Vec<Provision> = Vec::new();
    let mut line_start = 0;
    while line_start < shadow.chars.len() {
        let mut index = line_start;
        while shadow.chars.get(index) == Some(&' ') {
            index += 1;
        }
        for (prefix, suffix) in &templates {
            if !prefix.is_empty() && !shadow.literal_at(prefix, index) {
                continue;
            }
            if let Some((end, target)) = shadow.ref_at(index + prefix.len())
                && (suffix.is_empty() || shadow.literal_at(suffix, end))
            {
                found.push(Provision {
                    target,
                    start: shadow.offsets[line_start],
                    end: shadow.text.len(),
                });
                break;
            }
        }
        line_start = shadow.chars[line_start..]
            .iter()
            .position(|&c| c == '\n')
            .map_or(shadow.chars.len(), |at| line_start + at + 1);
    }
    // A provision runs to the next one that is not inside it.
    for index in 0..found.len() {
        let nested = format!("{}.", found[index].target);
        if let Some(next) = found[index + 1..]
            .iter()
            .find(|later| !later.target.starts_with(&nested))
        {
            found[index].end = next.start;
        }
    }
    found
}

/// The provisions containing `start..end`, outermost first.
fn containing(provisions: &[Provision], start: usize, end: usize) -> Vec<&Provision> {
    provisions
        .iter()
        .filter(|provision| provision.start <= start && end <= provision.end)
        .collect()
}

fn sentences(text: &str) -> Vec<(usize, usize)> {
    text.split_sentence_bound_indices()
        .map(|(at, sentence)| (at, at + sentence.len()))
        .collect()
}

/// What a derivation read: the cited span and the spans it is read with.
struct Read<'a> {
    start: usize,
    end: usize,
    with: &'a [(usize, usize)],
}

impl Read<'_> {
    /// Whether any of it overlaps `from..to`.
    fn covers(&self, from: usize, to: usize) -> bool {
        (self.start < to && from < self.end) || self.with.iter().any(|&(a, b)| a < to && from < b)
    }
}

/// Everything one decision about a span looks things up in.
struct Document<'a> {
    text: &'a str,
    shadow: Shadow<'a>,
    provisions: Vec<Provision>,
    policy: &'a ContextPolicy,
}

impl Document<'_> {
    fn provision(&self, target: &str) -> Option<&Provision> {
        self.provisions
            .iter()
            .find(|provision| provision.target == target)
    }

    /// The provision a byte is in, or else the sentence, named for a reviewer,
    /// with the bytes it runs over.
    fn place_of(&self, at: usize) -> (String, usize, usize) {
        if let Some(provision) = containing(&self.provisions, at, at + 1).last() {
            return (
                format!("provision {}", provision.target),
                provision.start,
                provision.end,
            );
        }
        let (a, b) = sentences(self.text)
            .into_iter()
            .find(|&(a, b)| a <= at && at < b)
            .unwrap_or((at, at + 1));
        (format!("the sentence at bytes {a}..{b}"), a, b)
    }
}

impl Context {
    fn uncited(&mut self, rationale: String) {
        if !self.uncited.contains(&rationale) {
            self.uncited.push(rationale);
        }
    }

    fn qualified(&mut self, reason: String) {
        if !self.qualified.contains(&reason) {
            self.qualified.push(reason);
        }
    }
}

/// What the cited `start..end` of `text` needs reading with, given the spans
/// the derivation reads it with.
pub(crate) fn context_of(
    text: &str,
    start: usize,
    end: usize,
    read_with: &[(usize, usize)],
    policy: &ContextPolicy,
) -> Context {
    let mut context = Context::default();
    if policy.is_empty() {
        return context;
    }
    let shadow = Shadow::new(text);
    let provisions = provisions(&shadow, policy);
    let document = Document {
        text,
        shadow,
        provisions,
        policy,
    };
    let read = Read {
        start,
        end,
        with: read_with,
    };
    in_the_cited_sentences(&document, &read, &mut context);
    prevailing_elsewhere(&document, &read, &mut context);
    defined_terms(&document, &read, &mut context);
    context
}

/// Conditions in the cited sentences, and the references they make.
fn in_the_cited_sentences(document: &Document<'_>, read: &Read<'_>, context: &mut Context) {
    let shadow = &document.shadow;
    for (sentence_start, sentence_end) in sentences(document.text) {
        if sentence_end <= read.start || read.end <= sentence_start {
            continue;
        }
        let from = shadow.index_of(sentence_start.max(read.start));
        let to = shadow.index_of(sentence_end.min(read.end));
        for cue in &document.policy.conditions {
            if !shadow.find_all(&chars(cue.trim()), from, to).is_empty() {
                context.qualified(format!("its sentence is conditional (\"{}\")", cue.trim()));
            }
        }
        for reference in references(shadow, document.policy, from, to) {
            let target = &reference.target;
            let provision = document.provision(target);
            let covered = provision.is_some_and(|p| read.covers(p.start, p.end));
            if let Some(cue) = cue_before(shadow, &document.policy.subject_to, reference.start) {
                context.qualified(format!("it is \"{cue}\" provision {target}"));
                if !covered {
                    context.uncited(format!(
                        "the cited clause is qualified by provision {target} (\"{cue}\"), which the \
                         derivation does not read it with"
                    ));
                }
            } else if provision.is_none() {
                context.uncited(format!(
                    "the cited clause refers to provision {target}, which is not among the \
                     document's provisions"
                ));
            } else if !covered {
                context.uncited(format!(
                    "the cited clause refers to provision {target}, which the derivation does not \
                     read it with"
                ));
            }
        }
    }
}

/// Provisions elsewhere that say they prevail over the cited one.
fn prevailing_elsewhere(document: &Document<'_>, read: &Read<'_>, context: &mut Context) {
    let shadow = &document.shadow;
    let ours: Vec<&str> = containing(&document.provisions, read.start, read.end)
        .iter()
        .map(|provision| provision.target.as_str())
        .collect();
    if ours.is_empty() {
        return;
    }
    let cited = shadow.index_of(read.start)..shadow.index_of(read.end);
    for reference in references(shadow, document.policy, 0, shadow.chars.len()) {
        if cited.contains(&reference.start) || !ours.contains(&reference.target.as_str()) {
            continue;
        }
        let Some(cue) = cue_before(shadow, &document.policy.overrides, reference.start) else {
            continue;
        };
        let (name, a, b) = document.place_of(shadow.offsets[reference.start]);
        context.qualified(format!("{name} applies \"{cue}\" it"));
        if !read.covers(a, b) {
            context.uncited(format!(
                "{name} prevails over the cited clause (\"{cue}\" provision {}), and the \
                 derivation does not read it with it",
                reference.target
            ));
        }
    }
}

/// Defined terms the cited span uses without their definitions.
fn defined_terms(document: &Document<'_>, read: &Read<'_>, context: &mut Context) {
    let excerpt = &document.text[read.start..read.end];
    for (term, (a, b)) in definitions(&document.shadow, document.policy) {
        if read.covers(a, b) || !uses_term(excerpt, &term) {
            continue;
        }
        let (place, ..) = document.place_of(a);
        context.uncited(format!(
            "the cited clause uses the defined term \"{term}\", defined in {place}, which the \
             derivation does not read it with"
        ));
    }
}

/// Each defined term, and the sentence that defines it.
fn definitions(shadow: &Shadow<'_>, policy: &ContextPolicy) -> Vec<(String, (usize, usize))> {
    let sentences = sentences(shadow.text);
    let mut found = Vec::new();
    for raw in &policy.definitions {
        let Some((prefix, suffix)) = template(raw, "{term}") else {
            continue;
        };
        if prefix.is_empty() || suffix.is_empty() {
            continue;
        }
        for start in shadow.find_all(&prefix, 0, shadow.chars.len()) {
            let from = start + prefix.len();
            let Some(length) = (1..=80)
                .take_while(|length| from + length <= shadow.chars.len())
                .find(|&length| shadow.literal_at(&suffix, from + length))
            else {
                continue;
            };
            let term = shadow.text[shadow.offsets[from]..shadow.offsets[from + length]]
                .trim()
                .to_string();
            if term.is_empty() || term.contains('\n') {
                continue;
            }
            let at = shadow.offsets[start];
            let sentence = sentences
                .iter()
                .copied()
                .find(|&(a, b)| a <= at && at < b)
                .unwrap_or((at, shadow.offsets[from + length]));
            if !found.iter().any(|(known, _): &(String, _)| known == &term) {
                found.push((term, sentence));
            }
        }
    }
    found
}

/// Whether `term` occurs in `excerpt` as written, as whole words. Case matters:
/// "Affiliate" is the defined term and "affiliate" is the word.
fn uses_term(excerpt: &str, term: &str) -> bool {
    excerpt.match_indices(term).any(|(at, _)| {
        let before = excerpt[..at].chars().next_back();
        let after = excerpt[at + term.len()..].chars().next();
        !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const AGREEMENT: &str = "1. Definitions\n\"Affiliate\" means an entity controlling a party.\n\
                             9. Termination\nSubject to Section 12, either party may terminate this \
                             agreement on thirty days' notice.\n12. Minimum term\nNotwithstanding Section 9, \
                             no party may terminate in the first year.\n14. Assignment\nAcme may assign to an \
                             Affiliate.\n";

    fn legal() -> ContextPolicy {
        ContextPolicy {
            provisions: vec!["{ref}.".to_string()],
            references: vec!["Section {ref}".to_string()],
            subject_to: vec!["subject to".to_string()],
            overrides: vec!["notwithstanding".to_string()],
            conditions: vec!["provided that".to_string()],
            definitions: vec!["\"{term}\" means".to_string()],
        }
    }

    fn cited(sentence: &str) -> (usize, usize) {
        let start = AGREEMENT.find(sentence).expect("in the agreement");
        (start, start + sentence.len())
    }

    #[test]
    fn a_clause_subject_to_another_and_overridden_by_it_is_qualified_twice() {
        let (start, end) = cited(
            "Subject to Section 12, either party may terminate this agreement on thirty days' notice.",
        );
        let context = context_of(AGREEMENT, start, end, &[], &legal());
        assert_eq!(context.qualified.len(), 2, "{context:?}");
        assert!(
            context
                .uncited
                .iter()
                .any(|r| r.contains("qualified by provision 12")),
            "{context:?}"
        );
        assert!(
            context
                .uncited
                .iter()
                .any(|r| r.contains("provision 12 prevails")),
            "{context:?}"
        );

        let (a, b) = cited("Notwithstanding Section 9, no party may terminate in the first year.");
        let read = context_of(AGREEMENT, start, end, &[(a, b)], &legal());
        assert_eq!(read.qualified.len(), 2);
        assert!(read.uncited.is_empty(), "{read:?}");
    }

    #[test]
    fn a_defined_term_needs_its_definition_and_the_ordinary_word_does_not() {
        let (start, end) = cited("Acme may assign to an Affiliate.");
        let context = context_of(AGREEMENT, start, end, &[], &legal());
        assert!(context.qualified.is_empty(), "{context:?}");
        assert_eq!(context.uncited.len(), 1, "{context:?}");
        assert!(context.uncited[0].contains("\"Affiliate\", defined in provision 1"));
        let lower = "acme may assign to an affiliate.";
        assert!(!uses_term(lower, "Affiliate"));
    }

    #[test]
    fn nothing_declared_finds_nothing() {
        let (start, end) = cited("Acme may assign to an Affiliate.");
        assert_eq!(
            context_of(AGREEMENT, start, end, &[], &ContextPolicy::default()),
            Context::default()
        );
    }
}
