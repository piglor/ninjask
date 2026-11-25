//! Deterministic pseudo-random numbers, with the helpers Showdown's battle
//! code uses: `random(n)`, `random_range(a, b)`, `chance(num, den)`, `sample`
//! and partial `shuffle`.
//!
//! Sampled play uses two generators:
//! - xoshiro256++, fast, for self-play and search (fast mode);
//! - Showdown's own `SodiumRNG` (ChaCha20, as libsodium's
//!   `randombytes_buf_deterministic`), so that a battle seeded like Showdown's
//!   draws the same numbers (reference mode).
//!
//! Enumeration uses a third backend, `Precise`, which replays weighted integer
//! decisions without invoking either generator.
//!
//! Both sampled generators produce 32-bit draws that the helpers scale exactly as Showdown's
//! `PRNG.random` does, so a helper's result depends only on the draw.

use crate::EnumerationMode;

#[derive(Clone, Debug)]
pub struct Rng {
    gen: Gen,
    /// When on, every draw is recorded with its call site (for comparing the
    /// order of draws with Showdown's).
    pub trace: Option<Vec<String>>,
    /// The draw log and forced draws (see [`Draws`]); off, it costs one
    /// check per draw.
    draws: Option<Box<Draws>>,
}

/// One call on the generator, as Showdown's battle makes it: the four
/// methods of its `PRNG` through which every random number of a battle is
/// drawn. Showdown writes the calls of a trace as a method name and
/// decimal strings ([`Draw`]); this is the same call in numbers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Call {
    /// `random(n)`: an integer in `[0, n)`.
    Random(u32),
    /// `random(from, to)`: an integer in `[from, to)`.
    RandomRange(u32, u32),
    /// `randomChance(numerator, denominator)`.
    Chance(u32, u32),
    /// `randomChance` with a numerator that is not a whole number (an
    /// accuracy the stages left fractional): the bits of the `f64`.
    ChanceF(u64, u32),
    /// `sample(items)`: an index below the number of items.
    Sample(u32),
    /// `shuffle(items, start, end)`: a Fisher-Yates shuffle of `[start, end)`.
    Shuffle { len: u32, start: u32, end: u32 },
}

/// What a [`Call`] gave.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Int(u32),
    Bool(bool),
    /// A shuffle: the permutation of the indices, `new[i] = old[perm[i]]`.
    Perm(Vec<u32>),
}

/// A call and its result as the differential harness writes them: the
/// method name, the arguments and the result, each number as a decimal
/// string (`randomChance`: `true` or `false`; `sample`: the number of items
/// and the index drawn; `shuffle`: the number of items and the range, then
/// the permutation).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Draw {
    pub method: String,
    pub args: Vec<String>,
    pub result: Vec<String>,
}

impl Call {
    pub fn method(&self) -> &'static str {
        match self {
            Call::Random(_) | Call::RandomRange(..) => "random",
            Call::Chance(..) | Call::ChanceF(..) => "randomChance",
            Call::Sample(_) => "sample",
            Call::Shuffle { .. } => "shuffle",
        }
    }

    fn args(&self) -> Vec<String> {
        match *self {
            Call::Random(n) | Call::Sample(n) => vec![n.to_string()],
            Call::RandomRange(a, b) | Call::Chance(a, b) => vec![a.to_string(), b.to_string()],
            Call::ChanceF(num, den) => vec![f64::from_bits(num).to_string(), den.to_string()],
            Call::Shuffle { len, start, end } => {
                vec![len.to_string(), start.to_string(), end.to_string()]
            }
        }
    }

    /// Whether `outcome` is a possible result of this call.
    fn admits(&self, outcome: &Outcome) -> bool {
        match (self, outcome) {
            (Call::Random(n) | Call::Sample(n), Outcome::Int(r)) => r < n,
            (Call::RandomRange(a, b), Outcome::Int(r)) => (a..b).contains(&r),
            (Call::Chance(..) | Call::ChanceF(..), Outcome::Bool(_)) => true,
            (Call::Shuffle { len, .. }, Outcome::Perm(p)) => {
                // The length first: a recorded list that names a huge `len` must not allocate.
                if p.len() != *len as usize {
                    return false;
                }
                let mut seen = vec![false; p.len()];
                p.iter().all(|&i| {
                    seen.get_mut(i as usize)
                        .is_some_and(|s| !std::mem::replace(s, true))
                })
            }
            _ => false,
        }
    }
}

impl Outcome {
    fn strings(&self) -> Vec<String> {
        match self {
            Outcome::Int(r) => vec![r.to_string()],
            Outcome::Bool(b) => vec![b.to_string()],
            Outcome::Perm(p) => p.iter().map(u32::to_string).collect(),
        }
    }
}

impl Draw {
    pub fn new(call: &Call, outcome: &Outcome) -> Draw {
        Draw {
            method: call.method().to_string(),
            args: call.args(),
            result: outcome.strings(),
        }
    }

    /// The call and outcome a trace entry names. A method the battle never
    /// calls gives `Ok(None)`: it can never match a call, but keeps its
    /// place in a list. A known method with arguments or a result that are
    /// not what the call takes is an error.
    pub fn parse(&self) -> Result<Option<(Call, Outcome)>, String> {
        let bad = |what: &str| {
            format!(
                "bad draw {} {:?} {:?}: {what}",
                self.method, self.args, self.result
            )
        };
        let nums = |v: &[String]| -> Result<Vec<u32>, String> {
            v.iter()
                .map(|x| x.parse::<u32>().map_err(|_| bad("not a number")))
                .collect()
        };
        let (call, outcome) = match (self.method.as_str(), self.args.len()) {
            ("random", 1) => {
                let a = nums(&self.args)?;
                let r = nums(&self.result)?;
                (
                    Call::Random(a[0]),
                    Outcome::Int(*r.first().ok_or_else(|| bad("no result"))?),
                )
            }
            ("random", 2) => {
                let a = nums(&self.args)?;
                let r = nums(&self.result)?;
                (
                    Call::RandomRange(a[0], a[1]),
                    Outcome::Int(*r.first().ok_or_else(|| bad("no result"))?),
                )
            }
            ("randomChance", 2) => {
                let den = nums(&self.args[1..])?[0];
                let r = match self.result.as_slice() {
                    [b] if b == "true" => true,
                    [b] if b == "false" => false,
                    _ => return Err(bad("not true or false")),
                };
                let call = match self.args[0].parse::<u32>() {
                    Ok(num) => Call::Chance(num, den),
                    Err(_) => match self.args[0].parse::<f64>() {
                        Ok(num) if num.is_finite() => Call::ChanceF(num.to_bits(), den),
                        _ => return Err(bad("not a number")),
                    },
                };
                (call, Outcome::Bool(r))
            }
            ("sample", 1) => {
                let a = nums(&self.args)?;
                let r = nums(&self.result)?;
                (
                    Call::Sample(a[0]),
                    Outcome::Int(*r.first().ok_or_else(|| bad("no result"))?),
                )
            }
            ("shuffle", 3) => {
                let a = nums(&self.args)?;
                (
                    Call::Shuffle {
                        len: a[0],
                        start: a[1],
                        end: a[2],
                    },
                    Outcome::Perm(nums(&self.result)?),
                )
            }
            ("random" | "randomChance" | "sample" | "shuffle", _) => {
                return Err(bad("wrong number of arguments"))
            }
            _ => return Ok(None),
        };
        if !call.admits(&outcome) {
            return Err(bad("the result is not possible for the call"));
        }
        Ok(Some((call, outcome)))
    }
}

/// The draw log and forced draws of a generator, for comparing a battle
/// with Showdown's draw for draw.
///
/// With the log on, every call is recorded with the result used. A list
/// of forced draws answers calls: the call that is the n-th made since the
/// list was loaded takes the result of the n-th entry when its method and
/// arguments are the entry's, and the generator's own otherwise. The
/// generator advances for every call either way, so what it would have
/// drawn is not lost and the battle's state stays its own.
#[derive(Clone, Debug, Default)]
struct Draws {
    /// The forced list; an entry that names no call the battle makes is `None`.
    queue: Vec<Option<(Call, Outcome)>>,
    made: Vec<(Call, Outcome)>,
}

#[derive(Clone, Debug)]
enum Gen {
    Xoshiro([u64; 4]),
    /// Showdown's sodium seed: a 32-byte ChaCha20 key, replaced at each draw.
    Sodium([u8; 32]),
    Precise(PreciseState),
}

const DRAW_COUNT: u64 = 1u64 << 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DecisionKind {
    Integer(u32),
    Chance(u32, u32),
    Damage(u32),
}

#[derive(Clone, Debug)]
struct Decision {
    kind: DecisionKind,
    alternatives: u32,
    selected: u32,
}

/// A depth-first path through conditional chance decisions. Suffixes belong
/// only to their prefix and are discarded when that prefix changes.
#[derive(Clone, Debug)]
pub(crate) struct PreciseState {
    mode: EnumerationMode,
    path: Vec<Decision>,
    cursor: usize,
    pub(crate) probability: f64,
}

impl PreciseState {
    pub(crate) fn new(mode: EnumerationMode) -> Self {
        Self {
            mode,
            path: Vec::new(),
            cursor: 0,
            probability: 1.0,
        }
    }

    fn choose(&mut self, kind: DecisionKind, alternatives: u32) -> (u32, f64) {
        if self.cursor == self.path.len() {
            self.path.push(Decision {
                kind,
                alternatives,
                selected: 0,
            });
        }
        let decision = &self.path[self.cursor];
        assert_eq!(decision.kind, kind, "chance replay diverged");
        assert_eq!(
            decision.alternatives, alternatives,
            "chance replay bounds diverged"
        );
        let k = decision.selected;
        let probability = match kind {
            DecisionKind::Integer(n) => {
                let lo = (u64::from(k) * DRAW_COUNT).div_ceil(u64::from(n));
                let hi = ((u64::from(k) + 1) * DRAW_COUNT).div_ceil(u64::from(n));
                (hi - lo) as f64 / DRAW_COUNT as f64
            }
            DecisionKind::Chance(num, den) => {
                let yes = (u64::from(num.min(den)) * DRAW_COUNT).div_ceil(u64::from(den));
                if alternatives == 1 {
                    1.0
                } else if k == 0 {
                    yes as f64 / DRAW_COUNT as f64
                } else {
                    (DRAW_COUNT - yes) as f64 / DRAW_COUNT as f64
                }
            }
            DecisionKind::Damage(_) => 1.0 / f64::from(alternatives),
        };
        self.cursor += 1;
        self.probability *= probability;
        (k, probability)
    }

    pub(crate) fn advance(&mut self) -> bool {
        self.path.truncate(self.cursor);
        while let Some(last) = self.path.last_mut() {
            if last.selected + 1 < last.alternatives {
                last.selected += 1;
                self.cursor = 0;
                self.probability = 1.0;
                return true;
            }
            self.path.pop();
        }
        false
    }
}

/// How a battle's generator is seeded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Seed {
    /// Fast mode: xoshiro256++ from a 64-bit seed ([`Rng::new`]).
    Fast(u64),
    /// Reference mode: Showdown's own generator from a seed like
    /// `sodium,<hex>` ([`Rng::from_showdown_seed`]), so the battle draws the
    /// numbers a Showdown battle with that seed draws.
    Showdown(String),
}

impl From<u64> for Seed {
    fn from(seed: u64) -> Seed {
        Seed::Fast(seed)
    }
}

impl Seed {
    /// A new generator with this seed.
    pub fn rng(&self) -> Result<Rng, String> {
        match self {
            Seed::Fast(seed) => Ok(Rng::new(*seed)),
            Seed::Showdown(seed) => Rng::from_showdown_seed(seed),
        }
    }
}

// Thread-local so the no-PRNG regression test can run alongside sampled tests.
#[cfg(test)]
std::thread_local! {
    static FORBID_PRNG: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
fn assert_prng_allowed() {
    FORBID_PRNG.with(|forbidden| assert!(!forbidden.get(), "enumeration invoked a PRNG"));
}

fn splitmix64(state: &mut u64) -> u64 {
    #[cfg(test)]
    assert_prng_allowed();
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Nonce of libsodium's deterministic random bytes ("LibsodiumDRG").
const SODIUM_NONCE: &[u8; 12] = b"LibsodiumDRG";

fn quarter(s: &mut [u32; 16], a: usize, b: usize, c: usize, d: usize) {
    s[a] = s[a].wrapping_add(s[b]);
    s[d] = (s[d] ^ s[a]).rotate_left(16);
    s[c] = s[c].wrapping_add(s[d]);
    s[b] = (s[b] ^ s[c]).rotate_left(12);
    s[a] = s[a].wrapping_add(s[b]);
    s[d] = (s[d] ^ s[a]).rotate_left(8);
    s[c] = s[c].wrapping_add(s[d]);
    s[b] = (s[b] ^ s[c]).rotate_left(7);
}

/// The first 64-byte ChaCha20 (RFC 8439) keystream block for this key, the
/// sodium nonce and block counter 0.
fn chacha20_block(key: &[u8; 32]) -> [u8; 64] {
    #[cfg(test)]
    assert_prng_allowed();
    let mut init = [0u32; 16];
    init[..4].copy_from_slice(&[0x6170_7865, 0x3320_646e, 0x7962_2d32, 0x6b20_6574]);
    for i in 0..8 {
        init[4 + i] = u32::from_le_bytes(key[4 * i..4 * i + 4].try_into().unwrap());
    }
    init[12] = 0;
    for i in 0..3 {
        init[13 + i] = u32::from_le_bytes(SODIUM_NONCE[4 * i..4 * i + 4].try_into().unwrap());
    }
    let mut s = init;
    for _ in 0..10 {
        quarter(&mut s, 0, 4, 8, 12);
        quarter(&mut s, 1, 5, 9, 13);
        quarter(&mut s, 2, 6, 10, 14);
        quarter(&mut s, 3, 7, 11, 15);
        quarter(&mut s, 0, 5, 10, 15);
        quarter(&mut s, 1, 6, 11, 12);
        quarter(&mut s, 2, 7, 8, 13);
        quarter(&mut s, 3, 4, 9, 14);
    }
    let mut out = [0u8; 64];
    for i in 0..16 {
        out[4 * i..4 * i + 4].copy_from_slice(&s[i].wrapping_add(init[i]).to_le_bytes());
    }
    out
}

impl Rng {
    /// Fast mode: xoshiro256++ from a 64-bit seed.
    /// Replaces the generator with the fast one of `seed` and nothing else:
    /// the draw log and the forced draws stay (a forced draw is still
    /// answered first), as does a trace.
    pub fn reseed(&mut self, seed: u64) {
        self.gen = Rng::new(seed).gen;
    }

    pub fn new(seed: u64) -> Rng {
        let mut st = seed;
        let state = [
            splitmix64(&mut st),
            splitmix64(&mut st),
            splitmix64(&mut st),
            splitmix64(&mut st),
        ];
        Rng {
            gen: Gen::Xoshiro(state),
            trace: None,
            draws: None,
        }
    }

    /// Reference mode: Showdown's PRNG for a seed like "sodium,0123abcd...".
    pub fn from_showdown_seed(seed: &str) -> Result<Rng, String> {
        let hex = seed
            .strip_prefix("sodium,")
            .ok_or_else(|| format!("not a sodium seed: {seed}"))?;
        if hex.len() > 64 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(format!("bad sodium seed: {seed}"));
        }
        // Showdown pads the hex to 32 bytes with zeros.
        let padded = format!("{hex:0<64}");
        let mut key = [0u8; 32];
        for (i, b) in key.iter_mut().enumerate() {
            *b = u8::from_str_radix(&padded[2 * i..2 * i + 2], 16).unwrap();
        }
        Ok(Rng {
            gen: Gen::Sodium(key),
            trace: None,
            draws: None,
        })
    }

    /// The seed as Showdown prints it (`battle.prng.getSeed()`), in reference mode.
    pub fn showdown_seed(&self) -> Option<String> {
        match &self.gen {
            Gen::Sodium(key) => Some(format!(
                "sodium,{}",
                key.iter().map(|b| format!("{b:02x}")).collect::<String>()
            )),
            Gen::Xoshiro(_) | Gen::Precise(_) => None,
        }
    }

    fn next_xoshiro(s: &mut [u64; 4]) -> u64 {
        #[cfg(test)]
        assert_prng_allowed();
        let result = (s[0].wrapping_add(s[3])).rotate_left(23).wrapping_add(s[0]);
        let t = s[1] << 17;
        s[2] ^= s[0];
        s[3] ^= s[1];
        s[1] ^= s[2];
        s[0] ^= s[3];
        s[2] ^= t;
        s[3] = s[3].rotate_left(45);
        result
    }

    /// A 32-bit draw (Showdown's `rng.next()`).
    pub fn next_u32(&mut self) -> u32 {
        match &mut self.gen {
            Gen::Precise(_) => panic!("raw PRNG draw during enumeration"),
            Gen::Xoshiro(s) => (Rng::next_xoshiro(s) >> 32) as u32,
            Gen::Sodium(key) => {
                let block = chacha20_block(key);
                key.copy_from_slice(&block[..32]);
                u32::from_be_bytes(block[32..36].try_into().unwrap())
            }
        }
    }

    /// 64 random bits, for seeding (not used by battle code).
    pub fn next_u64(&mut self) -> u64 {
        match &mut self.gen {
            Gen::Precise(_) => panic!("raw PRNG draw during enumeration"),
            Gen::Xoshiro(s) => Rng::next_xoshiro(s),
            Gen::Sodium(_) => ((self.next_u32() as u64) << 32) | self.next_u32() as u64,
        }
    }

    /// Uniform integer in `[0, n)`: Showdown's `random(n)`, floor(draw * n / 2^32).
    #[track_caller]
    pub fn random(&mut self, n: u32) -> u32 {
        assert!(n > 0);
        if let Gen::Precise(state) = &mut self.gen {
            return state.choose(DecisionKind::Integer(n), n).0;
        }
        let r = self.draw(n);
        if self.draws.is_none() {
            return r;
        }
        self.answer(Call::Random(n), Outcome::Int(r)).int()
    }

    /// The sampled generator's `random(n)`: the draw itself, with its trace
    /// entry, for the methods that are one call of their own.
    #[track_caller]
    fn draw(&mut self, n: u32) -> u32 {
        let r = ((self.next_u32() as u64 * n as u64) >> 32) as u32;
        if let Some(t) = &mut self.trace {
            let at = std::panic::Location::caller();
            t.push(format!("random({n}) = {r} @ {}:{}", at.file(), at.line()));
        }
        r
    }

    /// The result of a call: the forced one when the list has it at this
    /// place, else `natural`. Records the call when logging.
    fn answer(&mut self, call: Call, natural: Outcome) -> Outcome {
        let Some(draws) = self.draws.as_deref_mut() else {
            return natural;
        };
        let outcome = match draws.queue.get(draws.made.len()) {
            Some(Some((forced, outcome))) if *forced == call => outcome.clone(),
            _ => natural,
        };
        draws.made.push((call, outcome.clone()));
        outcome
    }

    /// Start logging every call ([`Rng::take_draws`]).
    pub fn record_draws(&mut self) {
        self.draws.get_or_insert_with(Default::default);
    }

    /// Load a list of forced draws (and start logging): the calls made from
    /// now on are answered from it where they match (see [`Draw::parse`]
    /// for what an entry can be); the log is emptied.
    pub fn force_draws(&mut self, list: &[Draw]) -> Result<(), String> {
        let queue = list
            .iter()
            .map(Draw::parse)
            .collect::<Result<Vec<_>, _>>()?;
        let draws = self.draws.get_or_insert_with(Default::default);
        draws.queue = queue;
        draws.made.clear();
        Ok(())
    }

    /// The calls made since the last take (or since logging started), with
    /// the result used for each, emptying the log.
    pub fn take_draws(&mut self) -> Vec<Draw> {
        match self.draws.as_deref_mut() {
            Some(d) => std::mem::take(&mut d.made)
                .iter()
                .map(|(call, outcome)| Draw::new(call, outcome))
                .collect(),
            None => Vec::new(),
        }
    }

    /// Damage variance: only DamageEndpoints replaces the distribution by endpoints.
    /// Delegation preserves ordinary play's draw count and caller trace.
    #[track_caller]
    pub fn random_damage(&mut self, n: u32) -> u32 {
        assert!(n > 0);
        if let Gen::Precise(state) = &mut self.gen {
            if state.mode == EnumerationMode::DamageEndpoints {
                let (selected, _) =
                    state.choose(DecisionKind::Damage(n), if n == 1 { 1 } else { 2 });
                return if selected == 0 { 0 } else { n - 1 };
            }
        }
        self.random(n)
    }

    pub(crate) fn precise(state: PreciseState) -> Self {
        Self {
            gen: Gen::Precise(state),
            trace: None,
            draws: None,
        }
    }

    pub(crate) fn into_precise(self) -> PreciseState {
        match self.gen {
            Gen::Precise(state) => state,
            _ => unreachable!("enumeration lost its precise generator"),
        }
    }

    /// Uniform integer in `[lo, hi)`, like Showdown's `random(lo, hi)`.
    #[track_caller]
    pub fn random_range(&mut self, lo: u32, hi: u32) -> u32 {
        if self.draws.is_none() || matches!(self.gen, Gen::Precise(_)) {
            return lo + self.random(hi - lo);
        }
        let r = lo + self.sampled(hi - lo);
        self.answer(Call::RandomRange(lo, hi), Outcome::Int(r))
            .int()
    }

    /// `random(n)` on the sampled generator, without a call of its own.
    #[track_caller]
    fn sampled(&mut self, n: u32) -> u32 {
        assert!(n > 0);
        self.draw(n)
    }

    /// True with probability `num / den`, like `randomChance`.
    #[track_caller]
    pub fn chance(&mut self, num: u32, den: u32) -> bool {
        assert!(den > 0);
        if let Gen::Precise(state) = &mut self.gen {
            let count = (u64::from(num.min(den)) * DRAW_COUNT).div_ceil(u64::from(den));
            let alternatives = if count == 0 || count == DRAW_COUNT {
                1
            } else {
                2
            };
            let (selected, _) = state.choose(DecisionKind::Chance(num, den), alternatives);
            return count == DRAW_COUNT || (count != 0 && selected == 0);
        }
        if self.draws.is_none() {
            return self.random(den) < num;
        }
        let natural = self.draw(den) < num;
        self.answer(Call::Chance(num, den), Outcome::Bool(natural))
            .bool()
    }

    /// `randomChance` for a numerator that need not be a whole number (an
    /// accuracy the stages left fractional): true with probability
    /// `num / den`.
    #[track_caller]
    pub fn chance_f(&mut self, num: f64, den: u32) -> bool {
        assert!(den > 0);
        if self.draws.is_none() || matches!(self.gen, Gen::Precise(_)) {
            return (self.random(den) as f64) < num;
        }
        let natural = (self.draw(den) as f64) < num;
        let call = if num.fract() == 0.0 && (0.0..=f64::from(u32::MAX)).contains(&num) {
            Call::Chance(num as u32, den)
        } else {
            Call::ChanceF(num.to_bits(), den)
        };
        self.answer(call, Outcome::Bool(natural)).bool()
    }

    #[track_caller]
    pub fn sample<T: Copy>(&mut self, items: &[T]) -> T {
        if self.draws.is_none() || matches!(self.gen, Gen::Precise(_)) {
            return items[self.random(items.len() as u32) as usize];
        }
        let natural = self.sampled(items.len() as u32);
        let index = self
            .answer(Call::Sample(items.len() as u32), Outcome::Int(natural))
            .int();
        items[index as usize]
    }

    /// Fisher–Yates shuffle of `items[start..end]`, like Showdown's `shuffle`.
    #[track_caller]
    pub fn shuffle<T>(&mut self, items: &mut [T], start: usize, end: usize) {
        if self.draws.is_none() || matches!(self.gen, Gen::Precise(_)) {
            let mut start = start;
            while start < end - 1 {
                let next = self.random_range(start as u32, end as u32) as usize;
                if start != next {
                    items.swap(start, next);
                }
                start += 1;
            }
            return;
        }
        // Shuffle the indices to get the permutation as a call result, then
        // move the items by whichever permutation the call gave.
        let mut order: Vec<u32> = (0..items.len() as u32).collect();
        let mut at = start;
        while at + 1 < end {
            let next = at + self.sampled((end - at) as u32) as usize;
            if at != next {
                order.swap(at, next);
            }
            at += 1;
        }
        let call = Call::Shuffle {
            len: items.len() as u32,
            start: start as u32,
            end: end as u32,
        };
        let Outcome::Perm(order) = self.answer(call, Outcome::Perm(order)) else {
            unreachable!("a shuffle gives a permutation")
        };
        // new[i] = old[order[i]], in place.
        for i in 0..order.len() {
            let mut j = order[i] as usize;
            while j < i {
                j = order[j] as usize;
            }
            items.swap(i, j);
        }
    }
}

impl Outcome {
    fn int(self) -> u32 {
        match self {
            Outcome::Int(r) => r,
            _ => unreachable!("not an integer"),
        }
    }

    fn bool(self) -> bool {
        match self {
            Outcome::Bool(b) => b,
            _ => unreachable!("not a boolean"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chacha20_matches_rfc8439_keystream() {
        // RFC 8439 2.4.2 uses another nonce; check the block function on the
        // all-zero key and nonce (test vector 1 of A.1) through a local copy.
        let key = [0u8; 32];
        let mut init = [0u32; 16];
        init[..4].copy_from_slice(&[0x6170_7865, 0x3320_646e, 0x7962_2d32, 0x6b20_6574]);
        let mut s = init;
        for _ in 0..10 {
            quarter(&mut s, 0, 4, 8, 12);
            quarter(&mut s, 1, 5, 9, 13);
            quarter(&mut s, 2, 6, 10, 14);
            quarter(&mut s, 3, 7, 11, 15);
            quarter(&mut s, 0, 5, 10, 15);
            quarter(&mut s, 1, 6, 11, 12);
            quarter(&mut s, 2, 7, 8, 13);
            quarter(&mut s, 3, 4, 9, 14);
        }
        let first = s[0].wrapping_add(init[0]).to_le_bytes();
        assert_eq!(first, [0x76, 0xb8, 0xe0, 0xad]);
        let _ = key;
    }

    #[test]
    fn sodium_matches_showdown() {
        // Showdown: new PRNG('sodium,0123456789abcdef0123456789abcdef').rng.next() x 5.
        let mut r = Rng::from_showdown_seed("sodium,0123456789abcdef0123456789abcdef").unwrap();
        let draws: Vec<u32> = (0..5).map(|_| r.next_u32()).collect();
        assert_eq!(
            draws,
            [1564598223, 2628916691, 3021397871, 1916167142, 4157841405]
        );
        assert_eq!(
            r.showdown_seed().unwrap(),
            "sodium,51b3d614ec21851a6419bc898430b29563817b1ec4fa00ea8c2cf63b160efc26"
        );
        // random(16), random(100), random(2, 5), randomChance(1, 3), random(6).
        let mut q = Rng::from_showdown_seed("sodium,0123456789abcdef0123456789abcdef").unwrap();
        assert_eq!(
            (
                q.random(16),
                q.random(100),
                q.random_range(2, 5),
                q.chance(1, 3),
                q.random(6)
            ),
            (5, 61, 4, false, 5)
        );
    }

    fn draw(method: &str, args: &[&str], result: &[&str]) -> Draw {
        Draw {
            method: method.into(),
            args: args.iter().map(|s| s.to_string()).collect(),
            result: result.iter().map(|s| s.to_string()).collect(),
        }
    }

    /// One of each call, in a fixed order, from a generator.
    fn calls(r: &mut Rng) -> (u32, u32, bool, &'static str, Vec<u32>) {
        let mut items = vec![0u32, 1, 2, 3, 4];
        r.shuffle(&mut items, 1, 4);
        (
            r.random(16),
            r.random_range(2, 6),
            r.chance(9, 10),
            r.sample(&["a", "b", "c"]),
            items,
        )
    }

    #[test]
    fn recording_the_draws_changes_nothing_and_logs_showdowns_calls() {
        let seed = "sodium,0123456789abcdef0123456789abcdef";
        let plain = calls(&mut Rng::from_showdown_seed(seed).unwrap());
        let mut logged = Rng::from_showdown_seed(seed).unwrap();
        logged.record_draws();
        assert_eq!(calls(&mut logged), plain);
        let draws = logged.take_draws();
        let methods: Vec<&str> = draws.iter().map(|d| d.method.as_str()).collect();
        assert_eq!(
            methods,
            ["shuffle", "random", "random", "randomChance", "sample"]
        );
        assert_eq!(draws[0].args, ["5", "1", "4"]);
        assert_eq!(draws[0].result.len(), 5);
        assert_eq!(draws[1].args, ["16"]);
        assert_eq!(draws[2].args, ["2", "6"]);
        assert_eq!(draws[3].args, ["9", "10"]);
        assert_eq!(draws[4].args, ["3"]);
        // the shuffle's permutation is what the items went through
        let order: Vec<u32> = draws[0].result.iter().map(|x| x.parse().unwrap()).collect();
        assert_eq!(order, plain.4);
        assert_eq!(plain.4[0], 0);
        assert_eq!(plain.4[4], 4);
        // taking empties the log
        assert!(logged.take_draws().is_empty());
    }

    #[test]
    fn the_draw_log_matches_the_generators_trace_and_leaves_the_fast_generator_alone() {
        let mut fast = Rng::new(5);
        let plain = calls(&mut fast);
        let mut logged = Rng::new(5);
        logged.record_draws();
        assert_eq!(calls(&mut logged), plain);
        assert_eq!(logged.take_draws().len(), 5);
    }

    #[test]
    fn forced_draws_answer_matching_calls_and_the_generator_the_rest() {
        let seed = "sodium,0123456789abcdef0123456789abcdef";
        let mut natural = Rng::from_showdown_seed(seed).unwrap();
        natural.record_draws();
        let want = calls(&mut natural);
        let draws = natural.take_draws();
        // The same list forced onto another generator gives the same results.
        let mut other = Rng::new(77);
        other.force_draws(&draws).unwrap();
        assert_eq!(calls(&mut other), want);
        assert_eq!(other.take_draws(), draws);
        // A call that does not match its place in the list takes the generator's own answer.
        let mut skewed = Rng::new(77);
        let mut list = draws.clone();
        list[1].result = vec!["99".into()];
        skewed.force_draws(&list).unwrap_err();
        list[1] = draw("random", &["15"], &["3"]);
        skewed.force_draws(&list).unwrap();
        let got = calls(&mut skewed);
        assert_eq!(got.4, want.4);
        let made = skewed.take_draws();
        assert_eq!(made[0], draws[0]);
        assert_eq!(made[1].args, ["16"]); // the call, not the entry
        assert_ne!(made[1], list[1]);
        // loading empties the log and a list shorter than the calls forces only its places
        skewed.force_draws(&[]).unwrap();
        calls(&mut skewed);
        assert_eq!(skewed.take_draws().len(), 5);
    }

    #[test]
    fn a_forced_chance_with_a_fractional_numerator_matches_the_accuracy_call() {
        let mut r = Rng::new(3);
        r.force_draws(&[
            draw("randomChance", &["67.5", "100"], &["true"]),
            draw("randomChance", &["90", "100"], &["false"]),
        ])
        .unwrap();
        assert!(r.chance_f(67.5, 100));
        assert!(!r.chance_f(90.0, 100));
        let made = r.take_draws();
        assert_eq!(made[0], draw("randomChance", &["67.5", "100"], &["true"]));
        assert_eq!(made[1], draw("randomChance", &["90", "100"], &["false"]));
        // an unforced accuracy check logs a whole numerator as `chance` does
        let mut q = Rng::new(3);
        q.record_draws();
        let a = q.chance_f(85.0, 100);
        let mut p = Rng::new(3);
        p.record_draws();
        assert_eq!(p.chance(85, 100), a);
        assert_eq!(q.take_draws(), p.take_draws());
    }

    #[test]
    fn draws_parse_to_calls_and_refuse_what_a_call_cannot_have() {
        assert_eq!(
            draw("random", &["16"], &["3"]).parse().unwrap(),
            Some((Call::Random(16), Outcome::Int(3)))
        );
        assert_eq!(
            draw("random", &["2", "6"], &["5"]).parse().unwrap(),
            Some((Call::RandomRange(2, 6), Outcome::Int(5)))
        );
        assert_eq!(
            draw("sample", &["3"], &["2"]).parse().unwrap(),
            Some((Call::Sample(3), Outcome::Int(2)))
        );
        assert_eq!(
            draw("shuffle", &["3", "1", "3"], &["0", "2", "1"])
                .parse()
                .unwrap(),
            Some((
                Call::Shuffle {
                    len: 3,
                    start: 1,
                    end: 3
                },
                Outcome::Perm(vec![0, 2, 1])
            ))
        );
        // a method the battle never calls keeps its place and matches nothing
        assert_eq!(draw("next", &[], &["7"]).parse().unwrap(), None);
        assert!(draw("random", &[], &["0.5"])
            .parse()
            .unwrap_err()
            .contains("arguments"));
        for bad in [
            draw("random", &["x"], &["0"]),
            draw("random", &["16"], &["16"]),
            draw("random", &["16"], &[]),
            draw("random", &["2", "6"], &["6"]),
            draw("randomChance", &["9", "10"], &["yes"]),
            draw("randomChance", &["nan", "10"], &["true"]),
            draw("randomChance", &["9", "x"], &["true"]),
            draw("sample", &["3"], &["3"]),
            draw("shuffle", &["3", "0", "3"], &["0", "1"]),
            draw("shuffle", &["3", "0", "3"], &["0", "1", "1"]),
            draw("shuffle", &["3", "0", "3"], &["0", "1", "3"]),
        ] {
            assert!(bad.parse().is_err(), "{bad:?}");
        }
        // what a call logs parses back to itself
        for (call, outcome) in [
            (Call::Chance(1, 24), Outcome::Bool(false)),
            (Call::ChanceF(67.5f64.to_bits(), 100), Outcome::Bool(true)),
            (
                Call::Shuffle {
                    len: 2,
                    start: 0,
                    end: 2,
                },
                Outcome::Perm(vec![1, 0]),
            ),
        ] {
            assert_eq!(
                Draw::new(&call, &outcome).parse().unwrap(),
                Some((call, outcome))
            );
        }
    }

    #[test]
    fn a_forced_permutation_moves_the_items_by_it() {
        let mut r = Rng::new(1);
        r.force_draws(&[draw("shuffle", &["4", "0", "4"], &["2", "0", "3", "1"])])
            .unwrap();
        let mut items = ["a", "b", "c", "d"];
        r.shuffle(&mut items, 0, 4);
        assert_eq!(items, ["c", "a", "d", "b"]);
    }

    #[test]
    fn seeds_make_the_generator_they_name() {
        assert!(Seed::from(5).rng().unwrap().showdown_seed().is_none());
        assert!(Seed::Showdown("sodium,ab".into())
            .rng()
            .unwrap()
            .showdown_seed()
            .is_some());
        assert!(Seed::Showdown("ab".into()).rng().is_err());
    }

    #[test]
    fn showdown_seed_round_trips() {
        let r = Rng::from_showdown_seed("sodium,0123456789abcdef0123456789abcdef").unwrap();
        assert_eq!(
            r.showdown_seed().unwrap(),
            "sodium,0123456789abcdef0123456789abcdef00000000000000000000000000000000"
        );
    }
}

#[cfg(test)]
mod enumeration_tests {
    use super::*;
    fn outcomes<T>(mode: EnumerationMode, f: impl Fn(&mut Rng) -> T) -> Vec<(T, f64)> {
        let mut path = PreciseState::new(mode);
        let mut out = Vec::new();
        loop {
            let mut rng = Rng::precise(path);
            let value = f(&mut rng);
            path = rng.into_precise();
            out.push((value, path.probability));
            if !path.advance() {
                return out;
            }
        }
    }
    struct NoPrng;
    impl NoPrng {
        fn enter() -> Self {
            FORBID_PRNG.with(|flag| flag.set(true));
            Self
        }
    }
    impl Drop for NoPrng {
        fn drop(&mut self) {
            FORBID_PRNG.with(|flag| flag.set(false));
        }
    }

    #[test]
    fn damage_endpoints_never_calls_a_prng() {
        use crate::{Battle, Dex};
        use std::sync::Arc;
        let dex = Arc::new(
            Dex::from_json_unpinned(include_str!("../tests/fixtures/enumeration.json")).unwrap(),
        );
        let team = "Fast|||battlearmor|Tackle,Double Hit|Hardy|||||50|";
        let mut battle = Battle::new(
            dex,
            [team, "Slow|||battlearmor|Splash|Hardy|||||50|"],
            ["a", "b"],
            1,
        )
        .unwrap();
        battle.choose_team(0, vec![0]);
        battle.choose_team(1, vec![0]);
        let sampled = [
            Rng::new(19),
            Rng::from_showdown_seed("sodium,1234").unwrap(),
        ];
        // Tripwires are in the actual xoshiro, ChaCha and seed-expansion code,
        // so even accidentally using a fresh or separate RNG fails this test.
        let _guard = NoPrng::enter();
        let helper_paths = outcomes(EnumerationMode::DamageEndpoints, |r| {
            let mut shuffled = [0, 1, 2];
            r.shuffle(&mut shuffled, 0, 3);
            (
                r.random(3),
                r.random_range(2, 4),
                r.chance(3, 10),
                r.sample(&[10, 20]),
                r.random_damage(16),
                shuffled,
            )
        });
        assert!((helper_paths.iter().map(|x| x.1).sum::<f64>() - 1.0).abs() < 1e-12);
        for rng in sampled {
            battle.rng = rng;
            for choice in ["move 1", "move 2"] {
                let result = battle
                    .enumerate_outcomes_text(choice, "move 1", EnumerationMode::DamageEndpoints)
                    .unwrap();
                assert!(!result.is_empty());
                assert!((result.iter().map(|o| o.probability).sum::<f64>() - 1.0).abs() < 1e-12);
            }
        }
    }

    #[test]
    fn no_prng_tripwire_detects_both_generators() {
        let mut generators = [
            Rng::new(19),
            Rng::from_showdown_seed("sodium,1234").unwrap(),
        ];
        let _guard = NoPrng::enter();
        for rng in &mut generators {
            assert!(
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| rng.next_u32())).is_err()
            );
        }
    }

    #[test]
    fn conditional_paths_have_their_own_bounds_and_lengths() {
        let out = outcomes(EnumerationMode::Precise, |r| {
            let a = r.random(3);
            (a, if a == 0 { None } else { Some(r.random(a + 1)) })
        });
        assert_eq!(out.len(), 6);
        assert_eq!(
            out.iter()
                .map(|x| x.0)
                .collect::<std::collections::HashSet<_>>()
                .len(),
            6
        );
        assert!((out.iter().map(|x| x.1).sum::<f64>() - 1.0).abs() < 1e-14);
    }
    #[test]
    fn scaled_integer_masses_and_chance_aggregation_match() {
        let out = outcomes(EnumerationMode::Precise, |r| r.random(3));
        assert_eq!(
            out.iter()
                .map(|x| x.1 * DRAW_COUNT as f64)
                .collect::<Vec<_>>(),
            vec![1431655766.0, 1431655765.0, 1431655765.0]
        );
        let chance = outcomes(EnumerationMode::Precise, |r| r.chance(1, 3));
        assert_eq!(chance, vec![(true, out[0].1), (false, out[1].1 + out[2].1)]);
        assert_eq!(
            outcomes(EnumerationMode::Precise, |r| r.chance(0, 3)),
            vec![(false, 1.0)]
        );
        assert_eq!(
            outcomes(EnumerationMode::Precise, |r| r.chance(4, 3)),
            vec![(true, 1.0)]
        );
    }
    #[test]
    fn only_damage_is_approximated() {
        let precise = outcomes(EnumerationMode::Precise, |r| r.random_damage(16));
        assert_eq!(
            precise,
            (0..16).map(|n| (n, 1.0 / 16.0)).collect::<Vec<_>>()
        );
        let mode = EnumerationMode::DamageEndpoints;
        assert_eq!(
            outcomes(mode, |r| r.random_damage(16)),
            vec![(0, 0.5), (15, 0.5)]
        );
        assert_eq!(outcomes(mode, |r| r.random_damage(1)), vec![(0, 1.0)]);
        assert_eq!(outcomes(mode, |r| r.random(16)), precise);
        assert_eq!(outcomes(mode, |r| r.random_range(4, 20)).len(), 16);
        assert_eq!(outcomes(mode, |r| r.sample(&[0; 16])).len(), 16);
        assert_eq!(
            outcomes(mode, |r| r.chance(3, 100)),
            outcomes(EnumerationMode::Precise, |r| r.chance(3, 100))
        );
        assert_eq!(
            outcomes(mode, |r| (r.random_damage(16), r.random_damage(16))).len(),
            4
        );
    }
    #[test]
    fn shuffle_enumerates_all_permutations() {
        let out = outcomes(EnumerationMode::DamageEndpoints, |r| {
            let mut a = [0, 1, 2];
            r.shuffle(&mut a, 0, 3);
            a
        });
        assert_eq!(out.len(), 6);
        assert_eq!(
            out.iter()
                .map(|x| x.0)
                .collect::<std::collections::HashSet<_>>()
                .len(),
            6
        );
        assert!((out.iter().map(|x| x.1).sum::<f64>() - 1.0).abs() < 1e-14);
    }
    #[test]
    fn damage_helper_preserves_sampled_draws() {
        for mut a in [
            Rng::new(13),
            Rng::from_showdown_seed("sodium,1234").unwrap(),
        ] {
            let mut b = a.clone();
            for _ in 0..100 {
                assert_eq!(a.random_damage(16), b.random(16));
            }
            assert_eq!(a.next_u32(), b.next_u32());
        }
    }
    #[test]
    #[should_panic(expected = "raw PRNG draw during enumeration")]
    fn raw_draws_are_forbidden() {
        Rng::precise(PreciseState::new(EnumerationMode::Precise)).next_u32();
    }
}
