//! Parse and `to_markdown` time over inputs built from syntax fragments, so
//! that a cost growing faster than the input is found without anyone having
//! foreseen its shape. Every pair and triple of fragments, as one unit, is
//! repeated `n` and `2n` times, and every pair also after a paragraph's first
//! word; doubling the input must at most roughly double the time.
//!
//! Each unit is screened with quick measurements on all cores, and the
//! suspects again at `n` and `4n`, where a quadratic cost stands further from
//! a linear one. Only units that look superlinear twice are measured once
//! more, one at a time, before they fail the test.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use markdown_syntax::parse;

/// Openers, closers, and prefixes of the constructs the parser reads. The
/// literal autolinks start with a space, after which they may begin.
const FRAGMENTS: &[&str] = &[
    "[",
    "]",
    "](",
    "(",
    ")",
    "![",
    "[[",
    "]]",
    "![[",
    "^[",
    "[^",
    ":a[",
    ":a{",
    "}",
    "<",
    ">",
    "`",
    "$",
    "*",
    "_",
    "~",
    "~~",
    "==",
    "\\",
    "&amp;",
    "|",
    " www.a",
    " http://a",
    "a@b.c",
    "a",
    " ",
    "\n",
    "> ",
    "- ",
];

/// Length of the smallest input of each unit.
const SMALL_BYTES: usize = 2_048;

/// Doubling a linear input about doubles its time, and a quadratic cost
/// about quadruples it: a unit is a suspect when twice the input takes more
/// than `SCREEN_RATIO` times as long.
const SCREEN_RATIO: f64 = 2.3;

/// Four times the input of a suspect may take at most `RESCREEN_RATIO` times
/// as long in a second screen, and `LIMIT_RATIO` times when measured alone,
/// where a quadratic cost takes sixteen.
const RESCREEN_RATIO: f64 = 6.0;
const LIMIT_RATIO: f64 = 8.0;

/// Below this, a difference in time is timer and scheduler noise.
const NOISE_FLOOR: Duration = Duration::from_micros(20);

/// A suspect is measured this often before it fails, so that a burst of load
/// on the machine does not fail a linear unit.
const ATTEMPTS: usize = 2;

/// Thread stack for every parse, as in `pathological_inputs.rs`.
const STACK_BYTES: usize = 2 << 20;

/// Text before the repeated unit, and the unit.
type Unit = (&'static str, String);

/// Every pair and triple of fragments, one unit for each rotation class:
/// `abc` repeated is `bca` repeated but for its ends. Pairs come again after
/// a word, so that a run of one fragment is read inside a paragraph rather
/// than, say, as a code fence.
fn units() -> Vec<Unit> {
    let mut units = Vec::new();
    for prefix in ["", "a "] {
        for (a, first) in FRAGMENTS.iter().enumerate() {
            for second in &FRAGMENTS[a..] {
                units.push((prefix, format!("{first}{second}")));
            }
        }
    }
    for (a, first) in FRAGMENTS.iter().enumerate() {
        for (b, second) in FRAGMENTS.iter().enumerate() {
            for (c, third) in FRAGMENTS.iter().enumerate() {
                let canonical = [a, b, c] <= [b, c, a] && [a, b, c] <= [c, a, b];
                if canonical && !(a == b && b == c) {
                    units.push(("", format!("{first}{second}{third}")));
                }
            }
        }
    }
    units
}

#[derive(Clone, Copy)]
struct Times {
    parse: Duration,
    serialize: Duration,
}

fn time(input: &str) -> Times {
    let started = Instant::now();
    let document = parse(input).document;
    let parse = started.elapsed();
    let started = Instant::now();
    let _ = document.to_markdown();
    Times {
        parse,
        serialize: started.elapsed(),
    }
}

/// The best times of a unit repeated `n` and `factor * n` times.
struct Growth {
    unit: Unit,
    n: usize,
    factor: usize,
    small: Times,
    large: Times,
}

impl Growth {
    /// The best of `runs` timings of each input, taken in turn so that a
    /// change in load falls on both.
    fn measure((prefix, unit): &Unit, factor: usize, runs: usize) -> Growth {
        let n = SMALL_BYTES.div_ceil(unit.len());
        let small_input = String::from(*prefix) + &unit.repeat(n);
        let large_input = String::from(*prefix) + &unit.repeat(factor * n);
        let slowest = Times {
            parse: Duration::MAX,
            serialize: Duration::MAX,
        };
        let (mut small, mut large) = (slowest, slowest);
        for _ in 0..runs {
            for (input, best) in [(&small_input, &mut small), (&large_input, &mut large)] {
                let times = time(input);
                best.parse = best.parse.min(times.parse);
                best.serialize = best.serialize.min(times.serialize);
            }
        }
        Growth {
            unit: (prefix, unit.clone()),
            n,
            factor,
            small,
            large,
        }
    }

    fn within(&self, ratio: f64) -> bool {
        let linear = |small: Duration, large: Duration| large <= small.mul_f64(ratio) + NOISE_FLOOR;
        linear(self.small.parse, self.large.parse)
            && linear(self.small.serialize, self.large.serialize)
    }

    fn report(&self) -> String {
        let ratio = |small: Duration, large: Duration| large.as_secs_f64() / small.as_secs_f64();
        format!(
            "{:?} + {:?} x{} -> x{}: parse {:?} -> {:?} ({:.1}x), to_markdown {:?} -> {:?} ({:.1}x)",
            self.unit.0,
            self.unit.1,
            self.n,
            self.factor * self.n,
            self.small.parse,
            self.large.parse,
            ratio(self.small.parse, self.large.parse),
            self.small.serialize,
            self.large.serialize,
            ratio(self.small.serialize, self.large.serialize),
        )
    }
}

/// Screens `units` on all cores at `n` and `factor * n`, best of two, and
/// returns those that grow past `ratio`.
fn screen(units: &[Unit], factor: usize, ratio: f64) -> Vec<Unit> {
    let next = AtomicUsize::new(0);
    let suspects = Mutex::new(Vec::new());
    let workers = thread::available_parallelism().map_or(1, |cores| cores.get());
    thread::scope(|scope| {
        for worker in 0..workers {
            thread::Builder::new()
                .name(format!("screen {worker}"))
                .stack_size(STACK_BYTES)
                .spawn_scoped(scope, || {
                    while let Some(unit) = units.get(next.fetch_add(1, Ordering::Relaxed)) {
                        if !Growth::measure(unit, factor, 2).within(ratio) {
                            suspects.lock().unwrap().push(unit.clone());
                        }
                    }
                })
                .expect("spawn screen thread");
        }
    });
    suspects.into_inner().unwrap()
}

/// Measures each suspect alone at `n` and `4n`, best of three, and returns
/// the measurements of those that grow past `LIMIT_RATIO` in every attempt.
fn confirm(suspects: Vec<Unit>) -> Vec<Growth> {
    thread::Builder::new()
        .name("confirm".to_owned())
        .stack_size(STACK_BYTES)
        .spawn(move || {
            let mut superlinear = Vec::new();
            for unit in &suspects {
                let mut growth = Growth::measure(unit, 4, 3);
                for _ in 1..ATTEMPTS {
                    if growth.within(LIMIT_RATIO) {
                        break;
                    }
                    growth = Growth::measure(unit, 4, 3);
                }
                if !growth.within(LIMIT_RATIO) {
                    superlinear.push(growth);
                }
            }
            superlinear
        })
        .expect("spawn confirm thread")
        .join()
        .unwrap_or_else(|_| panic!("confirm thread panicked"))
}

#[test]
fn repeated_fragment_combinations_grow_linearly() {
    let units = units();
    let started = Instant::now();
    // A second screen of the suspects, at four times the input, clears most
    // units a burst of load spoiled before the measurement one at a time.
    let suspects = screen(&screen(&units, 2, SCREEN_RATIO), 4, RESCREEN_RATIO);
    let screened = started.elapsed();
    let suspect_count = suspects.len();
    let superlinear = confirm(suspects);
    eprintln!(
        "{} units screened in {screened:?}; {suspect_count} measured again in {:?}",
        units.len(),
        started.elapsed() - screened
    );
    let report: Vec<String> = superlinear.iter().map(Growth::report).collect();
    assert!(
        superlinear.is_empty(),
        "{} of {} units grow faster than their input:\n{}",
        superlinear.len(),
        units.len(),
        report.join("\n")
    );
}
