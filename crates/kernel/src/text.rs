//! The text relation a citation is judged under.
//!
//! A citation claims that a document contains certain words, and whether it
//! does is decided here. The decision has to forgive exactly the differences
//! that do not change what a document says, and nothing else.
//!
//! What is forgiven:
//!
//! - **Unicode composition.** `é` as one code point and as `e` plus a combining
//!   acute are the same text.
//! - **Case, by folding.** Full Unicode case folding, the operation built for
//!   matching, brings `ß` and `SS`, `σ` and `ς`, `ﬁ` and `fi` to the same
//!   letters. Lowercasing is for display and does none of those.
//! - **Invisible characters.** A default-ignorable code point — a soft hyphen, a
//!   zero-width space or joiner — changes nothing a reader sees.
//! - **Typography.** Curly quotes, apostrophes, dashes and the minus sign are
//!   their plain forms, through an explicit table.
//! - **Whitespace run-length.**
//!
//! What is not: compatibility normalisation (NFKC). It would also turn `x²` into
//! `x2` and `½` into `1⁄2`, making different numbers equal — a forgiveness that
//! changes what a document says.
//!
//! And a term is found only as whole words. A match has to begin and end on a
//! word boundary as Unicode's segmentation rules (UAX #29) draw them, so `Ann`
//! is not in "announced" and `30` is not in "300". Those rules — no dictionary,
//! no model — give each Han ideograph, Hiragana character and Thai grapheme a
//! boundary of its own, so scripts written without spaces still match inside a
//! sentence. One tailoring: an apostrophe is a boundary on both sides. Unicode
//! keeps "Acme's" and "l'accord" whole, which would let a possessive hide the
//! name it is attached to and French elision hide half its nouns.
//!
//! Every table here is part of the claim-reasoning rules. Changing one, a
//! dependency bump that moves a Unicode version included, changes what a
//! citation matches, and has to bump [`crate::family::CLAIM_REASONING_RULES`].
//! The segmentation and normalisation tables are pinned by test; the
//! case-folding and default-ignorable data are pinned by `Cargo.lock`, which
//! every gate builds `--locked`.

use icu_casemap::CaseMapper;
use icu_properties::CodePointSetData;
use icu_properties::props::DefaultIgnorableCodePoint;
use unicode_normalization::UnicodeNormalization;
use unicode_segmentation::UnicodeSegmentation;

/// Whether `term` occurs in `text` as a run of whole words, both [folded](fold).
///
/// An empty term is never found. It would be contained in anything.
#[must_use]
pub fn contains_term(text: &str, term: &str) -> bool {
    contains_folded(&fold(text), &fold(term))
}

/// [`contains_term`] over text both sides have already been [folded](fold).
pub(crate) fn contains_folded(text: &str, term: &str) -> bool {
    if term.is_empty() {
        return false;
    }
    let boundaries = word_boundaries(text);
    // Every boundary is tried as a start, rather than walking non-overlapping
    // matches: the first occurrence of a term may sit inside a word while a
    // later, overlapping one is whole.
    boundaries.iter().any(|&start| {
        text[start..].starts_with(term) && boundaries.binary_search(&(start + term.len())).is_ok()
    })
}

/// The form two pieces of text are compared in.
#[must_use]
pub fn fold(text: &str) -> String {
    let ignorable = CodePointSetData::new::<DefaultIgnorableCodePoint>();
    let plain: String = text
        .chars()
        .filter(|&character| !ignorable.contains(character))
        .map(plain_typography)
        .collect();
    // Decomposed before folding and composed after: canonical caseless
    // matching, so a precomposed letter and its decomposition fold alike.
    let decomposed: String = plain.nfd().collect();
    let folded = CaseMapper::new().fold_string(&decomposed);
    let composed: String = folded.nfc().collect();
    composed.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Every offset in `text` a whole-word match may begin or end at, sorted.
fn word_boundaries(text: &str) -> Vec<usize> {
    let mut boundaries: Vec<usize> = text.split_word_bound_indices().map(|(at, _)| at).collect();
    boundaries.push(text.len());
    for (at, _) in text.match_indices('\'') {
        boundaries.push(at);
        boundaries.push(at + 1);
    }
    boundaries.sort_unstable();
    boundaries.dedup();
    boundaries
}

/// The plain form of a typographic character.
///
/// An explicit table on purpose. Each entry is a character that typesetting
/// or a word processor substitutes for a plain one without changing what the
/// text says; a broad compatibility mapping would reach characters that do.
const fn plain_typography(character: char) -> char {
    match character {
        // Apostrophes, single quotation marks, and the prime.
        '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}' | '\u{2032}' | '\u{02BC}' => '\'',
        // Double quotation marks, and the double prime.
        '\u{201C}' | '\u{201D}' | '\u{201E}' | '\u{201F}' | '\u{2033}' => '"',
        // Hyphens, dashes, and the minus sign.
        '\u{2010}' | '\u{2011}' | '\u{2012}' | '\u{2013}' | '\u{2014}' | '\u{2015}'
        | '\u{2212}' => '-',
        other => other,
    }
}
