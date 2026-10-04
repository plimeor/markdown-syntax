//! Memoized forward scans over one input.
//!
//! The inline parser and the text escaper ask "where is the first X at or
//! after position p" for many start positions in the same input. Answering
//! each question with a fresh scan makes a paragraph full of unclosed openers
//! quadratic (or worse, when a scan step itself scans). The structures here
//! answer every such question for one input in amortized linear total time.
//! They know nothing about Markdown: the caller supplies the per-position step,
//! and each structure is only ever used with one step function for one input.
//! A step moves to the next position it would examine rather than jumping over
//! positions it would only pass: walks from different starts merge only on
//! positions they both land on. `path_walk` and `bracket_walk` run the same
//! steps without memoization for one-off questions.

use alloc::vec::Vec;

/// Every start position of `pattern` in `input`, overlapping ones included,
/// in ascending order.
pub(crate) fn pattern_starts(input: &str, pattern: &str) -> Vec<usize> {
    (0..input.len())
        .filter(|index| input.as_bytes()[*index..].starts_with(pattern.as_bytes()))
        .collect()
}

/// Sorted positions of every match of a position-intrinsic predicate (one whose
/// verdict at a position does not depend on where a scan started), collected in
/// one pass on first use.
#[derive(Default)]
pub(crate) struct Positions {
    positions: Option<Vec<usize>>,
}

impl Positions {
    /// The first matching position at or after `from`.
    pub(crate) fn first_at_or_after(
        &mut self,
        from: usize,
        collect: impl FnOnce() -> Vec<usize>,
    ) -> Option<usize> {
        let positions = self.positions.get_or_insert_with(collect);
        positions
            .get(positions.partition_point(|position| *position < from))
            .copied()
    }
}

/// One step of a memoryless forward walk.
pub(crate) enum Step {
    /// The walk ends here with this answer.
    Done(Option<usize>),
    /// The walk continues at this (strictly greater) position.
    Next(usize),
}

const UNKNOWN: usize = usize::MAX;
const NO_ANSWER: usize = usize::MAX - 1;

fn encode(answer: Option<usize>) -> usize {
    answer.unwrap_or(NO_ANSWER)
}

fn decode(slot: usize) -> Option<usize> {
    (slot != NO_ANSWER).then_some(slot)
}

/// Runs a memoryless walk from `start` to its answer.
pub(crate) fn path_walk(start: usize, mut step: impl FnMut(usize) -> Step) -> Option<usize> {
    let mut node = start;
    loop {
        match step(node) {
            Step::Done(answer) => return answer,
            Step::Next(next) => node = next,
        }
    }
}

/// Answers for a walk whose next node and verdict depend only on the current
/// node. Walks from different starts merge once they meet, so each node is
/// stepped at most once.
#[derive(Default)]
pub(crate) struct PathMemo {
    answers: Vec<usize>,
    pending: Vec<usize>,
}

impl PathMemo {
    /// The answer of the walk starting at `start`, in a walk over the nodes
    /// `0..nodes`.
    pub(crate) fn resolve(
        &mut self,
        nodes: usize,
        start: usize,
        mut step: impl FnMut(usize) -> Step,
    ) -> Option<usize> {
        if self.answers.is_empty() {
            self.answers = alloc::vec![UNKNOWN; nodes];
        }
        let mut cursor = start;
        let answer = loop {
            let known = self.answers[cursor];
            if known != UNKNOWN {
                break decode(known);
            }
            self.pending.push(cursor);
            match step(cursor) {
                Step::Done(answer) => break answer,
                Step::Next(next) => cursor = next,
            }
        };
        for position in self.pending.drain(..) {
            self.answers[position] = encode(answer);
        }
        answer
    }
}

/// How one position moves a bracket-depth walk.
pub(crate) enum BracketStep {
    /// The walk has run out of input.
    End,
    /// Depth +1, then continue at the position.
    Open(usize),
    /// Depth -1; the walk ends here when this brings it back to its start depth
    /// minus one, otherwise it continues at the position.
    Close(usize),
    /// Depth unchanged; continue at the position.
    Pass(usize),
}

/// Runs a depth walk from `start` to the first close that drops the depth below
/// the depth it started at.
pub(crate) fn bracket_walk(
    start: usize,
    mut step: impl FnMut(usize) -> BracketStep,
) -> Option<usize> {
    let mut position = start;
    let mut depth = 0usize;
    loop {
        match step(position) {
            BracketStep::End => return None,
            BracketStep::Open(next) => {
                depth += 1;
                position = next;
            }
            BracketStep::Close(next) => {
                if depth == 0 {
                    return Some(position);
                }
                depth -= 1;
                position = next;
            }
            BracketStep::Pass(next) => position = next,
        }
    }
}

/// Answers "the first position whose close drops the depth below the depth the
/// walk started at" for a depth walk whose moves depend only on the current
/// position. That answer is the match of an opener whose walk starts at the
/// position just after it.
///
/// For a position `x`: a close answers itself; a pass answers what its
/// successor answers; an open skips its own match `y` (the successor's answer)
/// and answers what the position after `y` answers. Each position is resolved
/// once, iteratively, so neither time nor native stack grows with nesting.
#[derive(Default)]
pub(crate) struct BracketMemo {
    answers: Vec<usize>,
    frames: Vec<(usize, Phase)>,
}

#[derive(Clone, Copy)]
enum Phase {
    Enter,
    AfterPass,
    AfterInnerMatch,
    AfterOuter,
}

impl BracketMemo {
    /// The answer of the walk starting at `start`, in a walk over the
    /// positions `0..positions`.
    pub(crate) fn resolve(
        &mut self,
        positions: usize,
        start: usize,
        mut step: impl FnMut(usize) -> BracketStep,
    ) -> Option<usize> {
        if self.answers.is_empty() {
            self.answers = alloc::vec![UNKNOWN; positions];
        }
        self.frames.push((start, Phase::Enter));
        let mut answer = None;
        while let Some((position, phase)) = self.frames.pop() {
            match phase {
                Phase::Enter => {
                    let known = self.answers[position];
                    if known != UNKNOWN {
                        answer = decode(known);
                        continue;
                    }
                    match step(position) {
                        BracketStep::End => {
                            answer = None;
                            self.answers[position] = NO_ANSWER;
                        }
                        BracketStep::Close(_) => {
                            answer = Some(position);
                            self.answers[position] = position;
                        }
                        BracketStep::Pass(next) => {
                            self.frames.push((position, Phase::AfterPass));
                            self.frames.push((next, Phase::Enter));
                        }
                        BracketStep::Open(next) => {
                            self.frames.push((position, Phase::AfterInnerMatch));
                            self.frames.push((next, Phase::Enter));
                        }
                    }
                }
                Phase::AfterPass | Phase::AfterOuter => {
                    self.answers[position] = encode(answer);
                }
                Phase::AfterInnerMatch => match answer {
                    None => self.answers[position] = NO_ANSWER,
                    Some(inner_match) => {
                        let BracketStep::Close(after_match) = step(inner_match) else {
                            unreachable!("a walk answer is always a close");
                        };
                        self.frames.push((position, Phase::AfterOuter));
                        self.frames.push((after_match, Phase::Enter));
                    }
                },
            }
        }
        answer
    }
}

/// Versions of a map from the small integer keys `0..keys` to positions. Each
/// `insert` returns a new version and leaves the old one readable, sharing all
/// unchanged structure (path copying over a segment tree), so a version per
/// position costs `O(log keys)` each.
pub(crate) struct PersistentMap {
    keys: usize,
    /// `(left child, right child, leaf value)`; node 0 is the empty version.
    nodes: Vec<(usize, usize, usize)>,
    path: Vec<(usize, bool)>,
}

impl PersistentMap {
    /// The version that maps no key.
    pub(crate) const EMPTY: usize = 0;

    pub(crate) fn new(keys: usize) -> Self {
        Self {
            keys: keys.max(1),
            nodes: alloc::vec![(0, 0, NO_ANSWER)],
            path: Vec::new(),
        }
    }

    /// `version` with `key` mapped to `value`, as a new version.
    pub(crate) fn insert(&mut self, version: usize, key: usize, value: usize) -> usize {
        let (mut low, mut high, mut node) = (0, self.keys, version);
        while high - low > 1 {
            let middle = (low + high) / 2;
            let right = key >= middle;
            self.path.push((node, right));
            let (left_child, right_child, _) = self.nodes[node];
            node = if right { right_child } else { left_child };
            if right {
                low = middle;
            } else {
                high = middle;
            }
        }
        self.nodes.push((0, 0, value));
        let mut child = self.nodes.len() - 1;
        while let Some((node, right)) = self.path.pop() {
            let (left_child, right_child, value) = self.nodes[node];
            self.nodes.push(if right {
                (left_child, child, value)
            } else {
                (child, right_child, value)
            });
            child = self.nodes.len() - 1;
        }
        child
    }

    /// What `version` maps `key` to.
    pub(crate) fn get(&self, version: usize, key: usize) -> Option<usize> {
        let (mut low, mut high, mut node) = (0, self.keys, version);
        while high - low > 1 {
            let middle = (low + high) / 2;
            let (left_child, right_child, _) = self.nodes[node];
            if key >= middle {
                node = right_child;
                low = middle;
            } else {
                node = left_child;
                high = middle;
            }
        }
        decode(self.nodes[node].2)
    }
}
