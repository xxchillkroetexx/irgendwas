//! The draw algorithm (spec: draw, "Algorithm"). Pure: no I/O, the RNG is injected
//! (production: a fresh OS-seeded RNG per draw, S6).
//!
//! Steps: normalise the input (0), check the group size (1), ask the exclusions slice's
//! [`check_feasibility`] (2), then draw:
//! - **exact tier** (`n <= exact_max_members`): every valid assignment is enumerated (DFS,
//!   givers and receivers in ascending id order); the result is uniform over those with the
//!   fewest reciprocal pairs (G3). Two passes (count, then pick the k-th),
//!   both over the whole search space, so the run time depends only on the input.
//! - **large tier**: rejection sampling of uniform permutations (Fisher–Yates) until one is
//!   valid and free of reciprocal pairs (G4), else a matching-guided greedy that always
//!   succeeds (4b).
//!
//! Secrecy (S5): nothing here logs. The result and the RNG never leave the caller.

use rand::Rng;
use rand::seq::SliceRandom;

use crate::domain::feasibility::{Feasibility, Infeasibility, UserId, check_feasibility};

pub const MIN_DRAW_MEMBERS: usize = 2;
pub const MAX_DRAW_MEMBERS: usize = 200;

/// The exact tier uses one bit per member.
const EXACT_BITS: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DrawConfig {
    /// Largest group drawn by full enumeration (default 9: at most 133,496 leaves).
    pub exact_max_members: usize,
    /// Permutations tried by rejection sampling before the fallback (default 100,000).
    pub rejection_attempts: u32,
}

impl Default for DrawConfig {
    fn default() -> Self {
        DrawConfig {
            exact_max_members: 9,
            rejection_attempts: 100_000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DrawError {
    TooFewMembers {
        count: usize,
        min: usize,
    },
    TooManyMembers {
        count: usize,
        max: usize,
    },
    /// Exactly the reason [`check_feasibility`] returned. Its ids are never shown or logged.
    Infeasible(Infeasibility),
}

/// Step 0: sorted, deduplicated members and the exclusions between two distinct members,
/// as `(giver index, receiver index)` into the members, deduplicated.
struct Normalised {
    members: Vec<UserId>,
    /// `forbidden[g * n + r]`: `g` must not give to `r` (self pairs included).
    forbidden: Vec<bool>,
    exclusions: Vec<(UserId, UserId)>,
}

impl Normalised {
    fn new(members: &[UserId], exclusions: &[(UserId, UserId)]) -> Self {
        let mut ids = members.to_vec();
        ids.sort_unstable();
        ids.dedup();
        let n = ids.len();
        let mut forbidden = vec![false; n * n];
        for i in 0..n {
            forbidden[i * n + i] = true;
        }
        let mut kept = Vec::new();
        for &(giver, receiver) in exclusions {
            if giver == receiver {
                continue;
            }
            if let (Ok(g), Ok(r)) = (ids.binary_search(&giver), ids.binary_search(&receiver))
                && !forbidden[g * n + r]
            {
                forbidden[g * n + r] = true;
                kept.push((giver, receiver));
            }
        }
        kept.sort_unstable();
        Normalised {
            members: ids,
            forbidden,
            exclusions: kept,
        }
    }

    fn n(&self) -> usize {
        self.members.len()
    }

    fn allowed(&self, giver: usize, receiver: usize) -> bool {
        !self.forbidden[giver * self.n() + receiver]
    }

    /// `(giver, receiver)` ids from `receiver_of[giver index]`, sorted by giver.
    fn pairs(&self, receiver_of: &[usize]) -> Vec<(UserId, UserId)> {
        receiver_of
            .iter()
            .enumerate()
            .map(|(g, &r)| (self.members[g], self.members[r]))
            .collect()
    }
}

/// Steps 0–2 without drawing: whether a draw of these members under these rules can
/// happen, and if not, why. The admin status uses it for the blocker; [`draw`] runs it too.
pub fn precheck(members: &[UserId], exclusions: &[(UserId, UserId)]) -> Result<(), DrawError> {
    let norm = Normalised::new(members, exclusions);
    precheck_normalised(&norm)
}

fn precheck_normalised(norm: &Normalised) -> Result<(), DrawError> {
    let n = norm.n();
    if n < MIN_DRAW_MEMBERS {
        return Err(DrawError::TooFewMembers {
            count: n,
            min: MIN_DRAW_MEMBERS,
        });
    }
    if n > MAX_DRAW_MEMBERS {
        return Err(DrawError::TooManyMembers {
            count: n,
            max: MAX_DRAW_MEMBERS,
        });
    }
    match check_feasibility(&norm.members, &norm.exclusions) {
        Feasibility::Feasible => Ok(()),
        Feasibility::Infeasible(reason) => Err(DrawError::Infeasible(reason)),
    }
}

/// Draws a Secret Santa assignment: `(giver, receiver)` pairs sorted by giver.
///
/// `members` in any order (duplicates ignored); `exclusions` are `(giver, forbidden
/// receiver)` pairs in any order (duplicates, self pairs and pairs naming non-members
/// ignored). Same normalised input, config and seed: same output (G2).
pub fn draw<R: Rng + ?Sized>(
    members: &[UserId],
    exclusions: &[(UserId, UserId)],
    cfg: &DrawConfig,
    rng: &mut R,
) -> Result<Vec<(UserId, UserId)>, DrawError> {
    let norm = Normalised::new(members, exclusions);
    precheck_normalised(&norm)?;
    let n = norm.n();
    let receiver_of = if n <= cfg.exact_max_members && n <= EXACT_BITS {
        exact(&norm, rng)
    } else {
        rejection(&norm, cfg.rejection_attempts, rng).unwrap_or_else(|| fallback(&norm, rng))
    };
    Ok(norm.pairs(&receiver_of))
}

// ------------------------------------------------------------------ exact tier (step 3)

/// Depth-first enumeration of the valid assignments, givers and receivers ascending.
struct Exact {
    n: usize,
    /// Bit `r` of `allowed[g]`: `g` may give to `r`.
    allowed: Vec<u64>,
    receiver_of: Vec<usize>,
    mode: Mode,
}

enum Mode {
    /// Pass 1: the fewest reciprocal pairs over all leaves, and how many leaves have it.
    Count { best: u32, leaves: u64 },
    /// Pass 2: the `target`-th leaf (DFS order) with exactly `best` reciprocal pairs.
    Pick {
        best: u32,
        target: u64,
        seen: u64,
        found: Option<Vec<usize>>,
    },
}

impl Exact {
    fn bound(&self) -> u32 {
        match self.mode {
            Mode::Count { best, .. } | Mode::Pick { best, .. } => best,
        }
    }

    fn leaf(&mut self, reciprocal: u32) {
        match &mut self.mode {
            Mode::Count { best, leaves } => {
                if reciprocal < *best {
                    *best = reciprocal;
                    *leaves = 1;
                } else if reciprocal == *best {
                    *leaves += 1;
                }
            }
            Mode::Pick {
                best,
                target,
                seen,
                found,
            } => {
                if reciprocal == *best {
                    if *seen == *target && found.is_none() {
                        *found = Some(self.receiver_of.clone());
                    }
                    *seen += 1;
                }
            }
        }
    }

    /// Assigns giver `giver` (all smaller givers are assigned; `used` = their receivers).
    /// A reciprocal pair is counted when its second giver is assigned: `giver -> r` with
    /// `r < giver` and `r -> giver`.
    fn walk(&mut self, giver: usize, used: u64, reciprocal: u32) {
        if giver == self.n {
            self.leaf(reciprocal);
            return;
        }
        let mut candidates = self.allowed[giver] & !used;
        while candidates != 0 {
            let r = candidates.trailing_zeros() as usize;
            candidates &= candidates - 1;
            let count = reciprocal + u32::from(r < giver && self.receiver_of[r] == giver);
            if count > self.bound() {
                continue;
            }
            self.receiver_of[giver] = r;
            // No early exit in pass 2: it walks the same space whichever leaf is picked, so
            // the time a draw takes says nothing about the result (S2, D10).
            self.walk(giver + 1, used | (1u64 << r), count);
        }
    }
}

/// Step 3: uniform over the valid assignments with the minimum number of reciprocal pairs.
/// The caller guarantees that a valid assignment exists (step 2).
fn exact<R: Rng + ?Sized>(norm: &Normalised, rng: &mut R) -> Vec<usize> {
    let n = norm.n();
    let allowed: Vec<u64> = (0..n)
        .map(|g| {
            (0..n)
                .filter(|&r| norm.allowed(g, r))
                .fold(0u64, |mask, r| mask | (1u64 << r))
        })
        .collect();
    let mut search = Exact {
        n,
        allowed,
        receiver_of: vec![0; n],
        mode: Mode::Count {
            best: u32::MAX,
            leaves: 0,
        },
    };
    search.walk(0, 0, 0);
    let Mode::Count { best, leaves } = search.mode else {
        unreachable!("pass 1 counts")
    };
    assert!(leaves > 0, "step 2 guarantees a valid assignment");
    let target = rng.random_range(0..leaves);
    search.mode = Mode::Pick {
        best,
        target,
        seen: 0,
        found: None,
    };
    search.walk(0, 0, 0);
    match search.mode {
        Mode::Pick {
            found: Some(receiver_of),
            ..
        } => receiver_of,
        _ => unreachable!("pass 2 finds the target leaf of pass 1"),
    }
}

// ----------------------------------------------------------- large tier (step 4a)

/// Step 4a: up to `attempts` uniform permutations (Fisher–Yates, filled from the last
/// position down); the first that is valid and has no reciprocal pair. An attempt stops as
/// soon as a fixed position breaks a rule, which does not change which permutations are
/// accepted, so the result is uniform over the accepted set.
fn rejection<R: Rng + ?Sized>(norm: &Normalised, attempts: u32, rng: &mut R) -> Option<Vec<usize>> {
    let n = norm.n();
    let mut perm: Vec<usize> = Vec::with_capacity(n);
    // Position `i` is final once it is filled; a reciprocal pair {i, v} is seen when the
    // lower of the two positions is filled (the higher one already is).
    let breaks = |perm: &[usize], i: usize| {
        let v = perm[i];
        !norm.allowed(i, v) || (v > i && perm[v] == i)
    };
    'attempt: for _ in 0..attempts {
        perm.clear();
        perm.extend(0..n);
        for i in (1..n).rev() {
            let j = rng.random_range(0..=i);
            perm.swap(i, j);
            if breaks(&perm, i) {
                continue 'attempt;
            }
        }
        if breaks(&perm, 0) {
            continue;
        }
        return Some(perm);
    }
    None
}

// ----------------------------------------------------------- large tier (step 4b)

/// Step 4b: matching-guided greedy. Keeps a perfect matching `M` of the allowed edges in
/// which every fixed pair is matched; each giver (random order) takes the first random
/// candidate (non-reciprocal ones first) that `M` can be re-routed to. `M(g)` always
/// passes, so every giver is fixed. Always succeeds when step 2 did.
fn fallback<R: Rng + ?Sized>(norm: &Normalised, rng: &mut R) -> Vec<usize> {
    let n = norm.n();
    let adjacency: Vec<Vec<usize>> = (0..n)
        .map(|g| (0..n).filter(|&r| norm.allowed(g, r)).collect())
        .collect();
    let (mut match_g, mut match_r) = perfect_matching(&adjacency);

    let mut fixed_g = vec![false; n];
    let mut fixed_r = vec![false; n];
    let mut givers: Vec<usize> = (0..n).collect();
    givers.shuffle(rng);
    for &g in &givers {
        let mut candidates: Vec<usize> = adjacency[g]
            .iter()
            .copied()
            .filter(|&r| !fixed_r[r])
            .collect();
        candidates.shuffle(rng);
        // Stable partition: receivers that do not already give to `g` first.
        let reciprocal = |r: usize| fixed_g[r] && match_g[r] == g;
        let (mut ordered, back): (Vec<usize>, Vec<usize>) =
            candidates.into_iter().partition(|&r| !reciprocal(r));
        ordered.extend(back);
        let chosen = ordered
            .into_iter()
            .find(|&r| {
                r == match_g[g] || reroute(&adjacency, &fixed_r, &mut match_g, &mut match_r, g, r)
            })
            .expect("M(g) always passes");
        debug_assert_eq!(match_g[g], chosen);
        fixed_g[g] = true;
        fixed_r[chosen] = true;
    }
    match_g
}

/// A perfect matching of `adjacency` (Kuhn's augmenting paths, ascending order). Step 2
/// guarantees one exists. Returns (receiver of each giver, giver of each receiver).
fn perfect_matching(adjacency: &[Vec<usize>]) -> (Vec<usize>, Vec<usize>) {
    const NONE: usize = usize::MAX;
    let n = adjacency.len();
    let mut match_g = vec![NONE; n];
    let mut match_r = vec![NONE; n];
    for start in 0..n {
        // Iterative DFS over alternating paths from the free giver `start`.
        let mut seen_r = vec![false; n];
        let mut parent_r = vec![NONE; n]; // giver from which a receiver was reached
        let mut stack = vec![start];
        let mut end = None;
        'search: while let Some(g) = stack.pop() {
            for &r in &adjacency[g] {
                if seen_r[r] {
                    continue;
                }
                seen_r[r] = true;
                parent_r[r] = g;
                if match_r[r] == NONE {
                    end = Some(r);
                    break 'search;
                }
                stack.push(match_r[r]);
            }
        }
        let mut r = end.expect("step 2 guarantees a perfect matching");
        loop {
            let g = parent_r[r];
            let previous = match_g[g];
            match_g[g] = r;
            match_r[r] = g;
            if g == start {
                break;
            }
            r = previous;
        }
    }
    (match_g, match_r)
}

/// Re-routes `M` so that `g -> r` holds and the unfixed rest stays perfectly matched: one
/// alternating-path search (BFS) in the unfixed subgraph without `g` and `r`, from `M⁻¹(r)`
/// to `M(g)`. Updates `M` and returns `true` if such a path exists.
fn reroute(
    adjacency: &[Vec<usize>],
    fixed_r: &[bool],
    match_g: &mut [usize],
    match_r: &mut [usize],
    g: usize,
    r: usize,
) -> bool {
    const NONE: usize = usize::MAX;
    let n = adjacency.len();
    let start = match_r[r]; // unfixed: its receiver `r` is not fixed
    let target = match_g[g];
    let mut parent_r = vec![NONE; n];
    let mut queue = std::collections::VecDeque::from([start]);
    let mut found = false;
    'search: while let Some(x) = queue.pop_front() {
        for &y in &adjacency[x] {
            if fixed_r[y] || y == r || parent_r[y] != NONE {
                continue;
            }
            parent_r[y] = x;
            if y == target {
                found = true;
                break 'search;
            }
            // `y` is matched to an unfixed giver other than `g` (g's receiver is `target`).
            queue.push_back(match_r[y]);
        }
    }
    if !found {
        return false;
    }
    let mut y = target;
    loop {
        let x = parent_r[y];
        let previous = match_g[x];
        match_g[x] = y;
        match_r[y] = x;
        if x == start {
            break;
        }
        y = previous;
    }
    match_g[g] = r;
    match_r[r] = g;
    true
}

/// The kind of an [`Infeasibility`] for logs (S5: never its ids).
pub fn infeasibility_kind(reason: &Infeasibility) -> &'static str {
    match reason {
        Infeasibility::TooFewMembers => "too_few_members",
        Infeasibility::NoReceiverFor { .. } => "no_receiver_for",
        Infeasibility::NoGiverFor { .. } => "no_giver_for",
        Infeasibility::Blocked { .. } => "blocked",
    }
}
