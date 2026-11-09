//! Retrieval over archived tool results: BM25 with a code-aware tokenizer.
//!
//! Once a result has been cleared from the window or folded into a summary,
//! the model can still need one detail of it: the exact error line, the
//! signature of a function it read forty turns ago. The full text is kept
//! losslessly in the transcript log; this index finds the right passage.
//!
//! Why BM25 and not embeddings: REX talks to many providers, most without
//! an embeddings endpoint, and a local model would add a heavy dependency
//! and a download. Agent transcripts are dominated by identifiers, paths and
//! error codes, which lexical ranking handles well. The tokenizer is where
//! the effort goes: `src/autonomous.rs`, `read_file` and `parseConfig` are
//! each indexed whole and as their natural sub-words, so a query for
//! "parse config" finds `parseConfig`.

use std::collections::{HashMap, HashSet};

/// Characters per indexed passage.
pub const CHUNK_CHARS: usize = 1_400;
/// Characters shared between neighbouring passages, so a line that sits on
/// a boundary is whole in at least one of them.
pub const CHUNK_OVERLAP: usize = 140;
/// Only this many leading characters of one result are indexed. The rest
/// stays readable by turn and offset.
pub const MAX_INDEXED_CHARS: usize = 80_000;

const K1: f64 = 1.2;
const B: f64 = 0.75;
/// Extra weight of the tool name and target (the path or command) relative
/// to a word in the body.
const TARGET_WEIGHT: u16 = 3;

const STOPWORDS: &[&str] = &[
    "the", "an", "of", "to", "in", "is", "it", "and", "or", "for", "on", "at", "by", "be", "as",
    "with", "this", "that", "from", "are", "was", "if", "not", "no",
];

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '.' | '-' | '/')
}

/// Split `run` into sub-words at separators and camelCase boundaries.
fn sub_words(run: &str, out: &mut Vec<String>) {
    let mut cur = String::new();
    let mut prev: Option<char> = None;
    let flush = |cur: &mut String, out: &mut Vec<String>| {
        if cur.chars().count() >= 2 {
            out.push(cur.to_lowercase());
        }
        cur.clear();
    };
    for c in run.chars() {
        if matches!(c, '_' | '.' | '-' | '/') {
            flush(&mut cur, out);
            prev = None;
            continue;
        }
        // fooBar -> foo | Bar ;  HTTPServer -> HTTP | Server needs the
        // look-ahead below, so only the lower->upper edge is handled here
        if let Some(p) = prev {
            if p.is_lowercase() && c.is_uppercase() {
                flush(&mut cur, out);
            }
        }
        cur.push(c);
        prev = Some(c);
    }
    flush(&mut cur, out);
}

/// Lower-cased terms of `text`: each identifier-like run whole, plus its
/// sub-words. Stopwords and one-character terms are dropped.
pub fn tokenize(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut run = String::new();
    let emit = |run: &mut String, out: &mut Vec<String>| {
        let trimmed = run.trim_matches(|c| matches!(c, '.' | '-' | '/' | '_'));
        let n = trimmed.chars().count();
        if (2..=64).contains(&n) {
            let whole = trimmed.to_lowercase();
            let mut subs = Vec::new();
            sub_words(trimmed, &mut subs);
            // a plain word is its own only term; anything with separators
            // or case changes also yields its parts
            let plain = subs.len() == 1 && subs[0] == whole;
            out.push(whole);
            if !plain {
                out.extend(subs);
            }
        }
        run.clear();
    };
    for c in text.chars() {
        if is_word_char(c) {
            run.push(c);
        } else if !run.is_empty() {
            emit(&mut run, &mut out);
        }
    }
    if !run.is_empty() {
        emit(&mut run, &mut out);
    }
    out.retain(|t| !STOPWORDS.contains(&t.as_str()));
    out
}

/// Passages of at most [`CHUNK_CHARS`] characters, as `(char offset, text)`,
/// preferring to break at a newline. Only the first [`MAX_INDEXED_CHARS`]
/// characters are covered.
pub fn chunk(text: &str) -> Vec<(usize, String)> {
    let chars: Vec<char> = text.chars().take(MAX_INDEXED_CHARS).collect();
    let mut out = Vec::new();
    let mut start = 0usize;
    while start < chars.len() {
        let mut end = (start + CHUNK_CHARS).min(chars.len());
        if end < chars.len() {
            if let Some(nl) = chars[start..end].iter().rposition(|c| *c == '\n') {
                if nl > CHUNK_CHARS / 2 {
                    end = start + nl + 1;
                }
            }
        }
        let piece: String = chars[start..end].iter().collect();
        if !piece.trim().is_empty() {
            out.push((start, piece));
        }
        if end >= chars.len() {
            break;
        }
        start = end.saturating_sub(CHUNK_OVERLAP).max(start + 1);
    }
    out
}

/// Where a passage came from.
#[derive(Debug, Clone, PartialEq)]
pub struct ChunkMeta {
    pub turn: usize,
    /// Position of the call within its turn.
    pub call: usize,
    pub tool: String,
    pub target: Option<String>,
    pub ok: Option<bool>,
    /// Character offset of the passage inside the full result.
    pub offset: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Hit<'a> {
    pub meta: &'a ChunkMeta,
    pub text: &'a str,
    pub score: f64,
}

#[derive(Debug, Default, Clone)]
pub struct Index {
    chunks: Vec<(ChunkMeta, String)>,
    /// term -> (chunk id, term frequency)
    postings: HashMap<String, Vec<(u32, u16)>>,
    lens: Vec<u32>,
    total_len: u64,
}

impl Index {
    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }

    /// Index one passage. The tool name and target are indexed with extra
    /// weight but are not part of the returned text.
    pub fn add(&mut self, meta: ChunkMeta, text: String) {
        let id = self.chunks.len() as u32;
        let mut tf: HashMap<String, u16> = HashMap::new();
        let mut len = 0u32;
        for t in tokenize(&text) {
            *tf.entry(t).or_insert(0) += 1;
            len += 1;
        }
        let head = format!("{} {}", meta.tool, meta.target.as_deref().unwrap_or(""));
        for t in tokenize(&head) {
            let slot = tf.entry(t).or_insert(0);
            *slot = slot.saturating_add(TARGET_WEIGHT);
            len += u32::from(TARGET_WEIGHT);
        }
        for (term, f) in tf {
            self.postings.entry(term).or_default().push((id, f));
        }
        self.lens.push(len);
        self.total_len += u64::from(len);
        self.chunks.push((meta, text));
    }

    /// Top `k` passages for `query` among those `accept` allows, best first;
    /// equal scores prefer the newer turn.
    pub fn search<'a>(
        &'a self,
        query: &str,
        k: usize,
        accept: impl Fn(&ChunkMeta) -> bool,
    ) -> Vec<Hit<'a>> {
        if self.is_empty() || k == 0 {
            return Vec::new();
        }
        let terms: HashSet<String> = tokenize(query).into_iter().collect();
        if terms.is_empty() {
            return Vec::new();
        }
        let n = self.chunks.len() as f64;
        let avg = (self.total_len as f64 / n).max(1.0);
        let mut scores: HashMap<u32, f64> = HashMap::new();
        for term in &terms {
            let Some(list) = self.postings.get(term) else {
                continue;
            };
            let df = list.len() as f64;
            let idf = (1.0 + (n - df + 0.5) / (df + 0.5)).ln();
            for &(id, f) in list {
                let dl = f64::from(self.lens[id as usize]);
                let f = f64::from(f);
                let s = idf * (f * (K1 + 1.0)) / (f + K1 * (1.0 - B + B * dl / avg));
                *scores.entry(id).or_insert(0.0) += s;
            }
        }
        let mut ranked: Vec<(u32, f64)> = scores
            .into_iter()
            .filter(|(id, _)| accept(&self.chunks[*id as usize].0))
            .collect();
        ranked.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    self.chunks[b.0 as usize]
                        .0
                        .turn
                        .cmp(&self.chunks[a.0 as usize].0.turn)
                })
                .then_with(|| a.0.cmp(&b.0))
        });
        ranked
            .into_iter()
            .take(k)
            .map(|(id, score)| {
                let (meta, text) = &self.chunks[id as usize];
                Hit { meta, text, score }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(turn: usize, tool: &str, target: Option<&str>) -> ChunkMeta {
        ChunkMeta {
            turn,
            call: 0,
            tool: tool.into(),
            target: target.map(str::to_string),
            ok: Some(true),
            offset: 0,
        }
    }

    #[test]
    fn identifiers_are_indexed_whole_and_as_sub_words() {
        let t = tokenize("fn parseConfig() in crates/rex-providers/src/autonomous.rs");
        for want in [
            "parseconfig",
            "parse",
            "config",
            "crates/rex-providers/src/autonomous.rs",
            "autonomous",
            "rex",
            "providers",
            "rs",
        ] {
            assert!(t.contains(&want.to_string()), "missing {want} in {t:?}");
        }
        let snake = tokenize("read_file");
        assert_eq!(snake, ["read_file", "read", "file"]);
        assert!(!tokenize("the cat is on the mat").contains(&"the".to_string()));
        assert!(
            tokenize("a b c").is_empty(),
            "single characters carry no signal"
        );
        assert!(tokenize("error[E0432]: unresolved").contains(&"e0432".to_string()));
    }

    #[test]
    fn chunks_cover_the_text_and_overlap() {
        let line = "0123456789abcdefghij0123456789abcdefghij0123456789\n";
        let text = line.repeat(120); // 6_120 chars
        let chunks = chunk(&text);
        assert!(chunks.len() >= 4);
        assert!(chunks.iter().all(|(_, c)| c.chars().count() <= CHUNK_CHARS));
        assert_eq!(chunks[0].0, 0);
        // every character of the text lies in some chunk
        let chars: Vec<char> = text.chars().collect();
        let mut covered = vec![false; chars.len()];
        for (off, c) in &chunks {
            for slot in covered.iter_mut().skip(*off).take(c.chars().count()) {
                *slot = true;
            }
        }
        assert!(covered.iter().all(|c| *c), "a gap between passages");
        // neighbours overlap
        for w in chunks.windows(2) {
            let end = w[0].0 + w[0].1.chars().count();
            assert!(w[1].0 < end, "no overlap at {}", w[1].0);
        }
        assert!(chunk("").is_empty());
        assert!(chunk("  \n \n").is_empty());
    }

    #[test]
    fn chunking_handles_one_enormous_line_and_multibyte_text() {
        let one = "é".repeat(5_000);
        let chunks = chunk(&one);
        assert!(chunks.len() >= 4);
        assert!(chunks.iter().all(|(_, c)| c.chars().all(|ch| ch == 'é')));
        let huge = "x".repeat(MAX_INDEXED_CHARS * 2);
        let covered: usize = chunk(&huge)
            .last()
            .map(|(o, c)| o + c.chars().count())
            .unwrap();
        assert_eq!(covered, MAX_INDEXED_CHARS, "only the head is indexed");
    }

    #[test]
    fn bm25_ranks_the_relevant_passage_first() {
        let mut ix = Index::default();
        ix.add(
            meta(1, "read_file", Some("src/net.rs")),
            "fn connect() { open the socket and retry on timeout }".into(),
        );
        ix.add(
            meta(2, "read_file", Some("src/ui.rs")),
            "fn render() { draw the button and the label }".into(),
        );
        ix.add(
            meta(3, "run_command", Some("cargo test")),
            "error[E0432]: unresolved import `serde_yaml`".into(),
        );
        let hits = ix.search("unresolved import serde_yaml", 3, |_| true);
        assert_eq!(hits[0].meta.turn, 3);
        let hits = ix.search("socket timeout retry", 3, |_| true);
        assert_eq!(hits[0].meta.turn, 1);
        assert!(ix.search("zebra", 3, |_| true).is_empty());
        assert!(ix.search("", 3, |_| true).is_empty());
        assert!(ix.search("socket", 0, |_| true).is_empty());
    }

    #[test]
    fn the_target_counts_for_more_than_a_body_word() {
        let mut ix = Index::default();
        ix.add(
            meta(1, "read_file", Some("src/config.rs")),
            "struct Settings { port: u16 }".into(),
        );
        ix.add(
            meta(2, "read_file", Some("src/other.rs")),
            "see config in the docs for config".into(),
        );
        let hits = ix.search("config.rs", 2, |_| true);
        assert_eq!(hits[0].meta.turn, 1, "a path match must beat a stray word");
    }

    #[test]
    fn camel_case_queries_find_snake_case_code() {
        let mut ix = Index::default();
        ix.add(
            meta(4, "read_file", Some("a.rs")),
            "fn load_user_settings() {}".into(),
        );
        ix.add(
            meta(5, "read_file", Some("b.rs")),
            "fn unrelated() {}".into(),
        );
        let hits = ix.search("loadUserSettings", 2, |_| true);
        assert_eq!(hits[0].meta.turn, 4);
    }

    #[test]
    fn the_filter_and_recency_tie_break_apply() {
        let mut ix = Index::default();
        for t in 1..=3 {
            ix.add(
                meta(t, "read_file", None),
                "identical passage about widgets".into(),
            );
        }
        let hits = ix.search("widgets", 3, |_| true);
        assert_eq!(
            hits.iter().map(|h| h.meta.turn).collect::<Vec<_>>(),
            [3, 2, 1],
            "equal scores prefer the newer turn"
        );
        let only_old = ix.search("widgets", 3, |m| m.turn < 3);
        assert_eq!(only_old.len(), 2);
        assert!(only_old.iter().all(|h| h.meta.turn < 3));
    }
}
