//! Draw feasibility (spec: exclusions, "Feasibility function"). Pure: no DB, no I/O, no
//! feature gates, so the draw slice and the tests can call it anywhere.
//!
//! The question: given the group's members and the exclusion rules `(giver, excluded)`, is
//! there a bijection `assign: members → members` with `assign(m) != m` for every member and
//! `assign(g) != x` for every rule `(g, x)`? Cycles of length 2 are allowed (reciprocal
//! avoidance is a soft preference of the draw slice, not a hard constraint).
//!
//! That is exactly a perfect matching in the bipartite graph giver → allowed receiver, which
//! this module decides with Kuhn's augmenting-path algorithm (O(n · E)). When none exists,
//! Hall's theorem gives a witness: a set of givers whose allowed receivers are fewer than
//! they are ([`Infeasibility::Blocked`]).
//!
//! **Disclosure rule:** the ids in an [`Infeasibility`] are for logic and tests only. No UI
//! may render the ids, names or rules they stand for; members only ever see the one-bit
//! notice of the exclusions page, and logs record at most the variant kind.

pub type UserId = i32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Feasibility {
    Feasible,
    Infeasible(Infeasibility),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Infeasibility {
    /// Fewer than 2 distinct members.
    TooFewMembers,
    /// This giver may draw nobody (excluded every other member).
    NoReceiverFor { giver: UserId },
    /// Nobody may draw this member (every other member excluded them).
    NoGiverFor { receiver: UserId },
    /// Hall violation: these givers together may only draw these receivers, and there are
    /// fewer receivers than givers. Both sorted ascending.
    Blocked {
        givers: Vec<UserId>,
        receivers: Vec<UserId>,
    },
}

/// Decides whether a valid draw exists (see the module docs).
///
/// - `members`: the current group members; duplicates are ignored.
/// - `exclusions`: `(giver, excluded)` pairs. Self-rules, duplicates and pairs naming
///   anyone outside `members` are ignored.
///
/// The result depends only on the two sets, never on the input order. When several reasons
/// apply, the first of this list is returned: [`Infeasibility::TooFewMembers`], then
/// [`Infeasibility::NoReceiverFor`] with the smallest giver id, then
/// [`Infeasibility::NoGiverFor`] with the smallest receiver id, then
/// [`Infeasibility::Blocked`] (from a maximum matching built in ascending id order: the
/// first unmatched giver and everything reachable from it by alternating paths).
///
/// Complexity O(n · E) time (E = allowed pairs, at most n²) and O(n + rules) memory.
pub fn check_feasibility(members: &[UserId], exclusions: &[(UserId, UserId)]) -> Feasibility {
    let mut ids = members.to_vec();
    ids.sort_unstable();
    ids.dedup();
    let n = ids.len();
    if n < 2 {
        return Feasibility::Infeasible(Infeasibility::TooFewMembers);
    }
    let graph = Graph::new(&ids, exclusions);

    // A member nobody may draw for / a member nobody may draw: the cheap, specific reasons.
    if let Some(giver) = (0..n).find(|&g| graph.out_degree(g) == 0) {
        return Feasibility::Infeasible(Infeasibility::NoReceiverFor { giver: ids[giver] });
    }
    let mut in_degree = vec![n - 1; n];
    for forbidden in &graph.forbidden {
        for &receiver in forbidden {
            in_degree[receiver] -= 1;
        }
    }
    if let Some(receiver) = (0..n).find(|&r| in_degree[r] == 0) {
        return Feasibility::Infeasible(Infeasibility::NoGiverFor {
            receiver: ids[receiver],
        });
    }

    let matching = graph.maximum_matching();
    match matching.first_unmatched_giver() {
        None => Feasibility::Feasible,
        Some(free_giver) => {
            let (givers, receivers) = graph.hall_witness(&matching, free_giver);
            Feasibility::Infeasible(Infeasibility::Blocked {
                givers: givers.into_iter().map(|g| ids[g]).collect(),
                receivers: receivers.into_iter().map(|r| ids[r]).collect(),
            })
        }
    }
}

/// The bipartite graph giver → allowed receiver over member indexes `0..n` (index order =
/// ascending id). Allowed pairs are implicit: every other member except the forbidden ones.
struct Graph {
    n: usize,
    /// Per giver: the forbidden receivers (sorted, distinct, never the giver itself).
    forbidden: Vec<Vec<usize>>,
}

/// A matching: `receiver_match[r]` is the giver assigned to receiver `r`.
struct Matching {
    receiver_match: Vec<Option<usize>>,
    giver_matched: Vec<bool>,
}

impl Matching {
    /// The smallest unmatched giver, if any.
    fn first_unmatched_giver(&self) -> Option<usize> {
        self.giver_matched.iter().position(|matched| !matched)
    }
}

impl Graph {
    /// `ids` sorted and distinct.
    fn new(ids: &[UserId], exclusions: &[(UserId, UserId)]) -> Self {
        let index = |id: UserId| ids.binary_search(&id).ok();
        let mut forbidden = vec![Vec::new(); ids.len()];
        for &(giver, excluded) in exclusions {
            if let (Some(g), Some(x)) = (index(giver), index(excluded))
                && g != x
            {
                forbidden[g].push(x);
            }
        }
        for list in &mut forbidden {
            list.sort_unstable();
            list.dedup();
        }
        Graph {
            n: ids.len(),
            forbidden,
        }
    }

    fn allowed(&self, giver: usize, receiver: usize) -> bool {
        giver != receiver && self.forbidden[giver].binary_search(&receiver).is_err()
    }

    fn out_degree(&self, giver: usize) -> usize {
        self.n - 1 - self.forbidden[giver].len()
    }

    /// Kuhn's algorithm: try to augment from every giver in ascending order. A giver that
    /// finds no augmenting path when it is processed never gets one later, so the result is
    /// a maximum matching and no unmatched giver has an augmenting path.
    fn maximum_matching(&self) -> Matching {
        let mut matching = Matching {
            receiver_match: vec![None; self.n],
            giver_matched: vec![false; self.n],
        };
        let mut seen = vec![false; self.n];
        for giver in 0..self.n {
            seen.fill(false);
            if self.augment(giver, &mut matching, &mut seen) {
                matching.giver_matched[giver] = true;
            }
        }
        matching
    }

    /// Depth-first search for an augmenting path from the unmatched `start` (iterative, so
    /// large groups cannot overflow the stack). On success the path is flipped.
    fn augment(&self, start: usize, matching: &mut Matching, seen: &mut [bool]) -> bool {
        // `path[k]` = (giver, next receiver to try); `via[k]` = the receiver that leads from
        // `path[k]` to `path[k + 1]` (it is matched to `path[k + 1]`).
        let mut path: Vec<(usize, usize)> = vec![(start, 0)];
        let mut via: Vec<usize> = Vec::new();
        while let Some(top) = path.last_mut() {
            let (giver, next) = *top;
            let Some(receiver) = (next..self.n).find(|&r| !seen[r] && self.allowed(giver, r))
            else {
                path.pop();
                via.pop();
                continue;
            };
            top.1 = receiver + 1;
            seen[receiver] = true;
            via.push(receiver);
            match matching.receiver_match[receiver] {
                Some(holder) => path.push((holder, 0)),
                None => {
                    for (k, (giver, _)) in path.iter().enumerate() {
                        matching.receiver_match[via[k]] = Some(*giver);
                    }
                    return true;
                }
            }
        }
        false
    }

    /// The givers reachable from the unmatched `free_giver` by alternating paths (allowed
    /// edge to a receiver, matched edge back to its giver) and the receivers they reach.
    /// Every reached receiver is matched (else there would be an augmenting path), so there
    /// is exactly one receiver fewer than givers, and the receivers are all the givers'
    /// allowed receivers. Both sorted ascending.
    fn hall_witness(&self, matching: &Matching, free_giver: usize) -> (Vec<usize>, Vec<usize>) {
        let mut giver_seen = vec![false; self.n];
        let mut receiver_seen = vec![false; self.n];
        let mut queue = vec![free_giver];
        giver_seen[free_giver] = true;
        while let Some(giver) = queue.pop() {
            for receiver in (0..self.n).filter(|&r| self.allowed(giver, r)) {
                if receiver_seen[receiver] {
                    continue;
                }
                receiver_seen[receiver] = true;
                if let Some(holder) = matching.receiver_match[receiver]
                    && !giver_seen[holder]
                {
                    giver_seen[holder] = true;
                    queue.push(holder);
                }
            }
        }
        let collect = |seen: &[bool]| (0..self.n).filter(|&i| seen[i]).collect::<Vec<_>>();
        (collect(&giver_seen), collect(&receiver_seen))
    }
}
