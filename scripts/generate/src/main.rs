//! Writes `src/Cases.mw` for quickcheck.
//!
//! ```text
//! cargo run --release -- <package root>
//! ```
//!
//! What the crate generates depends on its random numbers, which it does not
//! let anyone seed; what it does with a failure does not. So the cases are the
//! shrinkers -- integers, floats, characters, vectors, strings and the rest --
//! and whole runs of the tester on inputs chosen here, with what it shrank each
//! failure to. The library is ported by hand into `src/`, and the crate's
//! source is fingerprinted.

use quickcheck::{Arbitrary, Gen, QuickCheck, TestResult, Testable};
use std::cell::RefCell;
use std::fmt::Write as _;
use std::path::PathBuf;

/// The crate version pinned in `Cargo.toml`.
const UPSTREAM_VERSION: &str = "1.0.3";

/// The fingerprint of the crate's source, which `src/` ports.
const SOURCES: u64 = 0x2ca6_c16d_232c_e0f6;

/// How many shrinks of one value are written: some sequences never end.
const TAKE: usize = 120;

fn main() {
    let root = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "../..".into()));

    let print = fingerprint(include_str!(concat!(env!("OUT_DIR"), "/sources.rs.txt")));
    if print != SOURCES {
        eprintln!(
            "error: quickcheck is not the version src/ ports.\n\
             Compare its source in {} with the previous version, carry any change\n\
             into src/, then set SOURCES in scripts/generate/src/main.rs to\n\
             {print:#x}",
            env!("UPSTREAM_DIR")
        );
        std::process::exit(1);
    }

    let cases = cases();
    let path = root.join("src/Cases.mw");
    std::fs::write(&path, &cases).unwrap();
    eprintln!("wrote {} ({} bytes)", path.display(), cases.len());
}

/// FNV-1a: stable across builds, which `DefaultHasher` does not promise.
fn fingerprint(text: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

// --- encoding -----------------------------------------------------------------------

/// A number as `digits` base-64 digits, most significant first, each digit the
/// character `'0' + d`: `'0'` to `'o'`, one contiguous run of ASCII.
fn digits(out: &mut String, value: u64, digits: u32) {
    assert!(
        value < 1 << (6 * digits),
        "{value} does not fit in {digits} digits"
    );
    for k in (0..digits).rev() {
        out.push(char::from(b'0' + ((value >> (6 * k)) & 63) as u8));
    }
}

/// A string, as its length in bytes (3 digits) and then its bytes.
fn text(out: &mut String, s: &str) {
    digits(out, s.len() as u64, 3);
    out.push_str(s);
}

/// A list of strings: a count (3 digits) and the strings.
fn texts<S: AsRef<str>>(out: &mut String, items: impl IntoIterator<Item = S>) {
    let items: Vec<S> = items.into_iter().collect();
    digits(out, items.len() as u64, 3);
    for s in items {
        text(out, s.as_ref());
    }
}

/// `text` as one Meadow string literal, broken with `\`-newline every `width`
/// characters. Only printable ASCII is written raw; a space that would start a
/// line is `\x20`, since a continuation drops leading whitespace.
fn long_literal(text: &str, width: usize) -> String {
    let mut out = String::with_capacity(text.len() + text.len() / width * 4 + 2);
    out.push('"');
    for (i, c) in text.chars().enumerate() {
        let line_start = i > 0 && i % width == 0;
        if line_start {
            out.push_str("\\\n    ");
        }
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '$' => out.push_str("\\$"),
            ' ' if line_start => out.push_str("\\x20"),
            ' '..='~' => out.push(c),
            _ => {
                let _ = write!(out, "\\u{{{:X}}}", u32::from(c));
            }
        }
    }
    out.push('"');
    out
}

fn shrinks<T: Arbitrary>(value: &T, show: impl Fn(&T) -> String) -> Vec<String> {
    value.shrink().take(TAKE).map(|v| show(&v)).collect()
}

fn ints(v: &[i32]) -> String {
    v.iter().map(i32::to_string).collect::<Vec<_>>().join(",")
}

// --- inputs -------------------------------------------------------------------------

/// A small deterministic generator, so that the cases are the same on every run.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }

    /// Small numbers often, and now and then anything.
    fn int(&mut self) -> i64 {
        match self.below(4) {
            0 => self.next() as i64,
            1 => self.below(2000) as i64 - 1000,
            2 => (self.next() >> self.below(64)) as i64,
            _ => [0, 1, -1, 2, -2, i64::MIN, i64::MAX, 127, -128, 255][self.below(10) as usize],
        }
    }

    fn small_vec(&mut self, len: u64, range: i64) -> Vec<i32> {
        (0..self.below(len))
            .map(|_| (self.below(2 * range as u64) as i64 - range) as i32)
            .collect()
    }
}

// --- the tester, on chosen inputs -----------------------------------------------------

thread_local! {
    static NEXT_INT: RefCell<i32> = const { RefCell::new(0) };
    static NEXT_VEC: RefCell<Vec<i32>> = const { RefCell::new(Vec::new()) };
    static BOUND: RefCell<i64> = const { RefCell::new(0) };
}

/// A value the tester is handed rather than one it makes up, shrunk as the
/// value inside is.
#[derive(Clone)]
struct Fixed<T>(T);

impl std::fmt::Debug for Fixed<i32> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::fmt::Debug for Fixed<Vec<i32>> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}]", ints(&self.0))
    }
}

impl Arbitrary for Fixed<i32> {
    fn arbitrary(_: &mut Gen) -> Self {
        Fixed(NEXT_INT.with(|n| *n.borrow()))
    }
    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        Box::new(self.0.shrink().map(Fixed))
    }
}

impl Arbitrary for Fixed<Vec<i32>> {
    fn arbitrary(_: &mut Gen) -> Self {
        Fixed(NEXT_VEC.with(|v| v.borrow().clone()))
    }
    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        Box::new(self.0.shrink().map(Fixed))
    }
}

fn bound() -> i64 {
    BOUND.with(|b| *b.borrow())
}

fn sum(v: &[i32]) -> i64 {
    v.iter().map(|&x| i64::from(x)).sum()
}

fn p0(v: Fixed<Vec<i32>>) -> bool {
    (v.0.len() as i64) < bound()
}
fn p1(v: Fixed<Vec<i32>>) -> bool {
    sum(&v.0) < bound()
}
fn p2(v: Fixed<Vec<i32>>) -> bool {
    v.0.iter().all(|&x| i64::from(x) < bound())
}
fn p3(v: Fixed<Vec<i32>>) -> bool {
    v.0.windows(2).all(|w| w[0] <= w[1])
}
fn p4(v: Fixed<Vec<i32>>) -> bool {
    !v.0.iter().any(|&x| i64::from(x) == bound())
}
fn p5(v: Fixed<Vec<i32>>) -> TestResult {
    if v.0.len() % 2 == 1 {
        TestResult::discard()
    } else {
        TestResult::from_bool(sum(&v.0) < bound())
    }
}
fn p6(x: Fixed<i32>, v: Fixed<Vec<i32>>) -> bool {
    i64::from(x.0) + (v.0.len() as i64) < bound()
}
fn p7(x: Fixed<i32>, v: Fixed<Vec<i32>>) -> bool {
    v.0.iter().all(|&e| e != x.0)
}
fn p8(x: Fixed<i32>) -> TestResult {
    if i64::from(x.0) > bound() {
        TestResult::error(format!("too big: {}", x.0))
    } else {
        TestResult::passed()
    }
}

fn run<A: Testable>(prop: A) -> String {
    match QuickCheck::new().tests(1).max_tests(1).quicktest(prop) {
        Ok(n) => format!("ok {n}"),
        Err(r) => format!("{r:?}"),
    }
}

fn tester_case(which: u64) -> String {
    match which {
        0 => run(p0 as fn(Fixed<Vec<i32>>) -> bool),
        1 => run(p1 as fn(Fixed<Vec<i32>>) -> bool),
        2 => run(p2 as fn(Fixed<Vec<i32>>) -> bool),
        3 => run(p3 as fn(Fixed<Vec<i32>>) -> bool),
        4 => run(p4 as fn(Fixed<Vec<i32>>) -> bool),
        5 => run(p5 as fn(Fixed<Vec<i32>>) -> TestResult),
        6 => run(p6 as fn(Fixed<i32>, Fixed<Vec<i32>>) -> bool),
        7 => run(p7 as fn(Fixed<i32>, Fixed<Vec<i32>>) -> bool),
        _ => run(p8 as fn(Fixed<i32>) -> TestResult),
    }
}

// --- cases --------------------------------------------------------------------------

fn cases() -> String {
    let mut rng = Rng(0x0c0c_9a1e_c0ff_ee42);
    let mut body = String::new();
    let mut counts = [0usize; 7];

    // Kind 0: integers of every width.
    for _ in 0..1600 {
        counts[0] += 1;
        let width = rng.below(8);
        let v = rng.int();
        digits(&mut body, 0, 1);
        digits(&mut body, width, 1);
        let (input, out) = match width {
            0 => (v as i8).to_string_and(shrinks(&(v as i8), |x| x.to_string())),
            1 => (v as i16).to_string_and(shrinks(&(v as i16), |x| x.to_string())),
            2 => (v as i32).to_string_and(shrinks(&(v as i32), |x| x.to_string())),
            3 => v.to_string_and(shrinks(&v, |x| x.to_string())),
            4 => (v as u8).to_string_and(shrinks(&(v as u8), |x| x.to_string())),
            5 => (v as u16).to_string_and(shrinks(&(v as u16), |x| x.to_string())),
            6 => (v as u32).to_string_and(shrinks(&(v as u32), |x| x.to_string())),
            _ => (v as u64).to_string_and(shrinks(&(v as u64), |x| x.to_string())),
        };
        text(&mut body, &input);
        texts(&mut body, out);
    }

    // Kind 1: floats, built as `mantissa * 2^exp` so that both sides make the
    // same one the same way.
    // The edges where a conversion to an integer saturates, then random ones.
    let edges: Vec<(bool, u64, i64, bool)> = [(1, 63), (1, 64), (3, 61), (1, 62), (7, 60)]
        .iter()
        .map(|&(m, e)| (true, m, e))
        .chain(
            [(1, 31), (1, 32), (3, 29), (1, 30)]
                .iter()
                .map(|&(m, e)| (false, m, e)),
        )
        .flat_map(|(d, m, e)| [(d, m, e, false), (d, m, e, true)])
        .collect();
    for k in 0..800 + edges.len() {
        counts[1] += 1;
        let random = k >= edges.len();
        let double = if random { rng.chance(50) } else { edges[k].0 };
        let special = if random && rng.chance(15) {
            rng.below(4) + 1
        } else {
            0
        };
        let mantissa = if random {
            rng.below(if double { 1 << 53 } else { 1 << 24 })
        } else {
            edges[k].1
        };
        let exp = if random {
            rng.below(if double { 2200 } else { 330 }) as i64 - if double { 1100 } else { 165 }
        } else {
            edges[k].2
        };
        let negative = if random { rng.chance(50) } else { edges[k].3 };
        digits(&mut body, 1, 1);
        digits(&mut body, u64::from(double), 1);
        digits(&mut body, special, 1);
        text(&mut body, &mantissa.to_string());
        text(&mut body, &exp.to_string());
        digits(&mut body, u64::from(negative), 1);
        let out = if double {
            let f = match special {
                1 => f64::NAN,
                2 => f64::INFINITY,
                3 => f64::NEG_INFINITY,
                4 => -0.0,
                _ => {
                    let mut f = mantissa as f64;
                    for _ in 0..exp.max(0) {
                        f *= 2.0;
                    }
                    for _ in 0..(-exp).max(0) {
                        f *= 0.5;
                    }
                    if negative { -f } else { f }
                }
            };
            let mut out = vec![(f as i64).to_string()];
            out.extend(shrinks(&f, |x| (*x as i64).to_string()));
            out
        } else {
            let f = match special {
                1 => f32::NAN,
                2 => f32::INFINITY,
                3 => f32::NEG_INFINITY,
                4 => -0.0,
                _ => {
                    let mut f = mantissa as f32;
                    for _ in 0..exp.max(0) {
                        f *= 2.0;
                    }
                    for _ in 0..(-exp).max(0) {
                        f *= 0.5;
                    }
                    if negative { -f } else { f }
                }
            };
            let mut out = vec![(f as i32).to_string()];
            out.extend(shrinks(&f, |x| (*x as i32).to_string()));
            out
        };
        texts(&mut body, out);
    }

    // Kind 2: characters.
    for _ in 0..600 {
        counts[2] += 1;
        let cp = loop {
            let cp = match rng.below(3) {
                0 => rng.below(0x100) as u32,
                1 => rng.below(0x10000) as u32,
                _ => rng.below(0x110000) as u32,
            };
            if char::from_u32(cp).is_some() {
                break cp;
            }
        };
        let c = char::from_u32(cp).unwrap();
        digits(&mut body, 2, 1);
        text(&mut body, &cp.to_string());
        texts(&mut body, shrinks(&c, |x| (*x as u32).to_string()));
    }

    // Kind 3: vectors of integers.
    for _ in 0..800 {
        counts[3] += 1;
        let mut v = rng.small_vec(12, 50);
        if rng.chance(10) {
            v.push(i32::MIN);
        }
        digits(&mut body, 3, 1);
        text(&mut body, &ints(&v));
        texts(&mut body, shrinks(&v, |x| ints(x)));
    }

    // Kind 4: strings.
    for _ in 0..400 {
        counts[4] += 1;
        let pool = [
            'a',
            'Z',
            ' ',
            '0',
            'é',
            '日',
            '\u{0}',
            '\u{10ffff}',
            '\u{d7ff}',
            '\u{e000}',
        ];
        let s: String = (0..rng.below(7))
            .map(|_| pool[rng.below(10) as usize])
            .collect();
        digits(&mut body, 4, 1);
        text(&mut body, &s);
        texts(&mut body, shrinks(&s, |x| x.clone()));
    }

    // Kind 5: the rest.
    for _ in 0..600 {
        counts[5] += 1;
        let which = rng.below(5);
        let a = rng.int() as i32;
        let b = rng.chance(50);
        let c = rng.below(256) as u8;
        digits(&mut body, 5, 1);
        digits(&mut body, which, 1);
        text(&mut body, &a.to_string());
        digits(&mut body, u64::from(b), 1);
        text(&mut body, &c.to_string());
        let out = match which {
            0 => shrinks(&b, |x| x.to_string()),
            1 => {
                let o = if b { Some(a) } else { None };
                shrinks(&o, |x| match x {
                    Some(n) => format!("some {n}"),
                    None => "none".to_string(),
                })
            }
            2 => {
                let r: Result<i32, bool> = if b { Ok(a) } else { Err(c % 2 == 1) };
                shrinks(&r, |x| match x {
                    Ok(n) => format!("ok {n}"),
                    Err(e) => format!("err {e}"),
                })
            }
            3 => shrinks(&(a, b, c), |(x, y, z)| format!("{x} {y} {z}")),
            _ => {
                let vv: Vec<Vec<u8>> = vec![vec![c, c / 2], vec![], vec![c / 3]];
                shrinks(&vv, |x| {
                    x.iter()
                        .map(|v| v.iter().map(u8::to_string).collect::<Vec<_>>().join(","))
                        .collect::<Vec<_>>()
                        .join(";")
                })
            }
        };
        texts(&mut body, out);
    }

    // Kind 6: the tester, on chosen inputs.
    for _ in 0..600 {
        counts[6] += 1;
        let which = rng.below(9);
        let k = rng.below(60) as i64 - 20;
        let x = rng.below(100) as i32 - 50;
        let mut v = rng.small_vec(10, 30);
        if which == 7 && rng.chance(60) && !v.is_empty() {
            let at = rng.below(v.len() as u64) as usize;
            v[at] = x;
        }
        NEXT_INT.with(|n| *n.borrow_mut() = x);
        NEXT_VEC.with(|n| *n.borrow_mut() = v.clone());
        BOUND.with(|b| *b.borrow_mut() = k);
        digits(&mut body, 6, 1);
        digits(&mut body, which, 1);
        text(&mut body, &k.to_string());
        text(&mut body, &x.to_string());
        text(&mut body, &ints(&v));
        text(&mut body, &tester_case(which));
    }

    let mut out = String::new();
    let _ = writeln!(
        out,
        "-- GENERATED by scripts/generate.sh from quickcheck {UPSTREAM_VERSION}.
-- Do not edit: run the script again instead.
--
-- Inputs, with what the crate makes of them, for `Tests.mw`: the shrinks of
-- {} integers, {} floats, {} characters, {} vectors and {} strings; {} other
-- values; and {} runs of the tester.
--
-- Copyright Andrew Gallant and the quickcheck contributors, and the Meadow
-- port's authors. Dual-licensed under MIT or the Unlicense: see COPYRIGHT.

-- Each case starts with its kind (1 base-64 digit), and its fields follow in
-- the order `Tests.mw` reads them. A string is its length in bytes (3 digits)
-- and then its bytes; a list is a count (3) and its strings. At most {TAKE}
-- shrinks of a value are listed.
@cfg(test)
@pub(pkg) def cases =
  {}",
        counts[0],
        counts[1],
        counts[2],
        counts[3],
        counts[4],
        counts[5],
        counts[6],
        long_literal(&body, 96)
    );
    out
}

trait ShownWith {
    fn to_string_and(&self, shrinks: Vec<String>) -> (String, Vec<String>);
}

impl<T: ToString> ShownWith for T {
    fn to_string_and(&self, shrinks: Vec<String>) -> (String, Vec<String>) {
        (self.to_string(), shrinks)
    }
}
