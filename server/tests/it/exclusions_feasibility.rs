//! exclusions slice: the pure feasibility function (AC25-AC29). No database. The oracles
//! (brute-force permutation search, bipartite matching) are written here, independent of
//! the implementation.

use std::collections::BTreeSet;

use app::domain::feasibility::{Feasibility, Infeasibility, UserId, check_feasibility};

use Feasibility::{Feasible, Infeasible};

// ---------------------------------------------------------------- tiny seeded RNG

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        // splitmix64
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn chance(&mut self, percent: usize) -> bool {
        self.below(100) < percent
    }

    fn shuffle<T>(&mut self, v: &mut [T]) {
        for i in (1..v.len()).rev() {
            let j = self.below(i + 1);
            v.swap(i, j);
        }
    }
}

// ---------------------------------------------------------------- oracles

/// The normalised problem: sorted distinct members and the set of forbidden pairs among them.
struct Problem {
    members: Vec<UserId>,
    forbidden: BTreeSet<(UserId, UserId)>,
}

impl Problem {
    fn new(members: &[UserId], rules: &[(UserId, UserId)]) -> Self {
        let set: BTreeSet<UserId> = members.iter().copied().collect();
        let forbidden = rules
            .iter()
            .copied()
            .filter(|(g, x)| g != x && set.contains(g) && set.contains(x))
            .collect();
        Problem {
            members: set.into_iter().collect(),
            forbidden,
        }
    }

    fn allowed(&self, giver: UserId, receiver: UserId) -> bool {
        giver != receiver && !self.forbidden.contains(&(giver, receiver))
    }

    fn allowed_of(&self, giver: UserId) -> BTreeSet<UserId> {
        self.members
            .iter()
            .copied()
            .filter(|r| self.allowed(giver, *r))
            .collect()
    }

    /// Brute force: is there a permutation without fixed points that respects all rules?
    fn brute_force(&self) -> bool {
        fn go(p: &Problem, i: usize, used: &mut Vec<bool>) -> bool {
            if i == p.members.len() {
                return true;
            }
            for j in 0..p.members.len() {
                if !used[j] && p.allowed(p.members[i], p.members[j]) {
                    used[j] = true;
                    if go(p, i + 1, used) {
                        return true;
                    }
                    used[j] = false;
                }
            }
            false
        }
        go(self, 0, &mut vec![false; self.members.len()])
    }

    /// Kuhn's algorithm: does a perfect matching giver -> allowed receiver exist?
    fn perfect_matching(&self) -> bool {
        let n = self.members.len();
        let adj: Vec<Vec<usize>> = (0..n)
            .map(|i| {
                (0..n)
                    .filter(|j| self.allowed(self.members[i], self.members[*j]))
                    .collect()
            })
            .collect();
        fn try_kuhn(
            v: usize,
            adj: &[Vec<usize>],
            seen: &mut [bool],
            matched: &mut [Option<usize>],
        ) -> bool {
            for &r in &adj[v] {
                if seen[r] {
                    continue;
                }
                seen[r] = true;
                if matched[r].is_none_or(|u| try_kuhn(u, adj, seen, matched)) {
                    matched[r] = Some(v);
                    return true;
                }
            }
            false
        }
        let mut matched = vec![None; n];
        (0..n).all(|v| try_kuhn(v, &adj, &mut vec![false; n], &mut matched))
    }

    /// The reason the spec's priority order prescribes, except that `Blocked`'s witness is
    /// only checked for its properties (`None` = "some Blocked").
    fn expected_reason(&self) -> Option<Infeasibility> {
        if self.members.len() < 2 {
            return Some(Infeasibility::TooFewMembers);
        }
        for &g in &self.members {
            if self.allowed_of(g).is_empty() {
                return Some(Infeasibility::NoReceiverFor { giver: g });
            }
        }
        for &r in &self.members {
            if self.members.iter().all(|g| !self.allowed(*g, r)) {
                return Some(Infeasibility::NoGiverFor { receiver: r });
            }
        }
        None
    }
}

/// Runs `check_feasibility` and checks it against both oracles. Returns the result.
fn check(members: &[UserId], rules: &[(UserId, UserId)], what: &str) -> Feasibility {
    let p = Problem::new(members, rules);
    let matching = p.perfect_matching();
    if (2..=8).contains(&p.members.len()) {
        assert_eq!(
            p.brute_force(),
            matching,
            "oracles disagree ({what}): members {members:?} rules {rules:?}"
        );
    }
    let result = check_feasibility(members, rules);
    let feasible_by_oracle = p.members.len() >= 2 && matching;
    match &result {
        Feasible => assert!(
            feasible_by_oracle,
            "{what}: Feasible but no valid assignment exists: {members:?} {rules:?}"
        ),
        Infeasible(reason) => {
            assert!(
                !feasible_by_oracle,
                "{what}: {reason:?} but a valid assignment exists: {members:?} {rules:?}"
            );
            match p.expected_reason() {
                Some(expected) => assert_eq!(
                    reason, &expected,
                    "{what}: wrong reason / priority: {members:?} {rules:?}"
                ),
                None => match reason {
                    Infeasibility::Blocked { givers, receivers } => {
                        assert!(!givers.is_empty(), "{what}: empty witness");
                        assert!(
                            givers.windows(2).all(|w| w[0] < w[1]),
                            "{what}: givers sorted and distinct: {givers:?}"
                        );
                        assert!(
                            receivers.windows(2).all(|w| w[0] < w[1]),
                            "{what}: receivers sorted and distinct: {receivers:?}"
                        );
                        assert!(
                            receivers.len() < givers.len(),
                            "{what}: Hall violation needs fewer receivers: {givers:?} {receivers:?}"
                        );
                        assert!(
                            givers.iter().all(|g| p.members.contains(g)),
                            "{what}: givers are members"
                        );
                        let union: BTreeSet<UserId> =
                            givers.iter().flat_map(|g| p.allowed_of(*g)).collect();
                        let listed: BTreeSet<UserId> = receivers.iter().copied().collect();
                        assert_eq!(
                            listed, union,
                            "{what}: receivers are the union of the givers' allowed receivers"
                        );
                    }
                    other => panic!(
                        "{what}: expected Blocked (no empty row or column), got {other:?}: \
                         {members:?} {rules:?}"
                    ),
                },
            }
        }
    }
    result
}

// ---------------------------------------------------------------- AC25 - AC28

#[test]
fn ac25_fewer_than_two_distinct_members_is_too_few() {
    assert_eq!(
        check_feasibility(&[], &[]),
        Infeasible(Infeasibility::TooFewMembers)
    );
    assert_eq!(
        check_feasibility(&[1], &[]),
        Infeasible(Infeasibility::TooFewMembers)
    );
    assert_eq!(
        check_feasibility(&[1, 1], &[]),
        Infeasible(Infeasibility::TooFewMembers)
    );
    assert_eq!(
        check_feasibility(&[7, 7, 7], &[(7, 7)]),
        Infeasible(Infeasibility::TooFewMembers)
    );
    assert_eq!(check_feasibility(&[1, 2], &[]), Feasible);
    assert_eq!(check_feasibility(&[1, 2, 2], &[]), Feasible);
}

#[test]
fn ac26_no_receiver_no_giver_and_their_priority() {
    assert_eq!(
        check_feasibility(&[1, 2], &[(1, 2)]),
        Infeasible(Infeasibility::NoReceiverFor { giver: 1 })
    );
    assert_eq!(
        check_feasibility(&[1, 2, 3], &[(1, 3), (2, 3)]),
        Infeasible(Infeasibility::NoGiverFor { receiver: 3 })
    );
    // Both apply: NoReceiverFor wins.
    assert_eq!(
        check_feasibility(&[1, 2, 3], &[(1, 2), (1, 3), (2, 3), (1, 2)]),
        Infeasible(Infeasibility::NoReceiverFor { giver: 1 })
    );
    // The smallest id is named when several qualify.
    assert_eq!(
        check_feasibility(&[1, 2, 3], &[(3, 1), (3, 2), (2, 1), (2, 3)]),
        Infeasible(Infeasibility::NoReceiverFor { giver: 2 })
    );
    assert_eq!(
        check_feasibility(
            &[1, 2, 3, 4],
            &[(1, 3), (2, 3), (4, 3), (1, 4), (2, 4), (3, 4)]
        ),
        Infeasible(Infeasibility::NoGiverFor { receiver: 3 })
    );
}

#[test]
fn ac27_blocked_names_the_hall_violator() {
    let rules = [(1, 2), (1, 3), (2, 1), (2, 3), (3, 1), (3, 2)];
    assert_eq!(
        check_feasibility(&[1, 2, 3, 4, 5], &rules),
        Infeasible(Infeasibility::Blocked {
            givers: vec![1, 2, 3],
            receivers: vec![4, 5],
        })
    );
    // n = 3: both 1 and 2 may only draw 3.
    assert_eq!(
        check_feasibility(&[1, 2, 3], &[(1, 2), (2, 1)]),
        Infeasible(Infeasibility::Blocked {
            givers: vec![1, 2],
            receivers: vec![3],
        })
    );
}

#[test]
fn ac28_stray_rules_are_ignored() {
    assert_eq!(
        check_feasibility(&[1, 2], &[(1, 1), (1, 9), (9, 2)]),
        Feasible
    );
    assert_eq!(
        check_feasibility(&[1, 2], &[(9, 9), (2, 2), (1, 2), (1, 2)]),
        Infeasible(Infeasibility::NoReceiverFor { giver: 1 })
    );
    // Stray rules never change a verdict.
    let base = [(1, 3), (2, 3)];
    let noisy = [(1, 3), (2, 3), (3, 3), (4, 1), (1, 40), (2, 3), (1, 3)];
    assert_eq!(
        check_feasibility(&[1, 2, 3], &base),
        check_feasibility(&[1, 2, 3], &noisy)
    );
}

#[test]
fn ac28_permuting_members_or_rules_never_changes_the_result() {
    let mut rng = Rng(0xA28);
    for case in 0..600 {
        let n = 2 + rng.below(6);
        let mut members: Vec<UserId> = (1..=n as i32).collect();
        let mut rules = vec![];
        for g in 1..=n as i32 {
            for x in 1..=n as i32 {
                if g != x && rng.chance(35) {
                    rules.push((g, x));
                }
            }
        }
        let reference = check_feasibility(&members, &rules);
        for _ in 0..4 {
            rng.shuffle(&mut members);
            rng.shuffle(&mut rules);
            assert_eq!(
                check_feasibility(&members, &rules),
                reference,
                "case {case}: {members:?} {rules:?}"
            );
        }
        // Duplicated entries do not matter either.
        let mut doubled_members = members.clone();
        doubled_members.extend(members.clone());
        let mut doubled_rules = rules.clone();
        doubled_rules.extend(rules.clone());
        assert_eq!(
            check_feasibility(&doubled_members, &doubled_rules),
            reference,
            "case {case}: duplicates"
        );
    }
}

// ---------------------------------------------------------------- edge cases

#[test]
fn small_groups_and_extreme_rule_sets() {
    // n = 2: the only assignment is the swap.
    assert_eq!(check(&[1, 2], &[], "n=2"), Feasible);
    assert_eq!(
        check(&[1, 2], &[(2, 1)], "n=2 other direction"),
        Infeasible(Infeasibility::NoReceiverFor { giver: 2 })
    );
    // n = 3: one rule leaves exactly one cycle, two rules in a cycle leave the other one.
    assert_eq!(check(&[1, 2, 3], &[(1, 2)], "n=3 one rule"), Feasible);
    assert_eq!(
        check(
            &[1, 2, 3],
            &[(1, 2), (2, 3), (3, 1)],
            "n=3 a cycle excluded"
        ),
        Feasible
    );
    assert_eq!(
        check(
            &[1, 2, 3],
            &[(1, 2), (2, 1)],
            "n=3 two givers share one receiver"
        ),
        Infeasible(Infeasibility::Blocked {
            givers: vec![1, 2],
            receivers: vec![3]
        })
    );
    // Everyone excludes everyone.
    for n in 2..=6 {
        let members: Vec<UserId> = (1..=n).collect();
        let rules: Vec<(UserId, UserId)> = members
            .iter()
            .flat_map(|g| members.iter().map(move |x| (*g, *x)))
            .collect();
        assert_eq!(
            check(&members, &rules, "everyone excludes everyone"),
            Infeasible(Infeasibility::NoReceiverFor { giver: 1 })
        );
    }
    // Rules that only name non-members.
    assert_eq!(
        check(&[1, 2, 3], &[(4, 1), (1, 4), (5, 6)], "non-members"),
        Feasible
    );
    // Sparse, non-consecutive and unordered ids.
    assert_eq!(check(&[90, 3, 41], &[(3, 41)], "odd ids"), Feasible);
}

// ---------------------------------------------------------------- AC29

fn random_instance(rng: &mut Rng, n: usize, id_range: i32) -> (Vec<UserId>, Vec<(UserId, UserId)>) {
    let mut pool: Vec<UserId> = (1..=id_range).collect();
    rng.shuffle(&mut pool);
    let mut members: Vec<UserId> = pool[..n].to_vec();
    let outsiders: Vec<UserId> = pool[n..].to_vec();
    // Density between "no rules" and "almost everything".
    let density = [0, 5, 15, 30, 50, 70, 90][rng.below(7)];
    let mut rules = vec![];
    for &g in &members {
        for &x in &members {
            if g != x && rng.chance(density) {
                rules.push((g, x));
            }
        }
    }
    // Noise: self rules, rules naming non-members, duplicates.
    for _ in 0..rng.below(4) {
        if let Some(&m) = members.first() {
            rules.push((m, m));
        }
        if let (Some(&m), Some(&o)) = (members.get(rng.below(n.max(1))), outsiders.first()) {
            rules.push((m, o));
            rules.push((o, m));
        }
    }
    if !rules.is_empty() && rng.chance(50) {
        let again = rules[rng.below(rules.len())];
        rules.push(again);
    }
    if !members.is_empty() && rng.chance(20) {
        members.push(members[0]);
    }
    rng.shuffle(&mut members);
    rng.shuffle(&mut rules);
    (members, rules)
}

#[test]
fn ac29_property_feasible_iff_a_valid_permutation_exists_n_0_to_8() {
    let mut rng = Rng(0x29_0001);
    let mut feasible = 0;
    let mut infeasible = 0;
    let mut blocked = 0;
    for n in 0..=8usize {
        for case in 0..1500 {
            let (members, rules) = random_instance(&mut rng, n, 14);
            match check(&members, &rules, &format!("n={n} case={case}")) {
                Feasible => feasible += 1,
                Infeasible(Infeasibility::Blocked { .. }) => {
                    blocked += 1;
                    infeasible += 1;
                }
                Infeasible(_) => infeasible += 1,
            }
        }
    }
    // The generator must exercise all outcomes, or the property test proves little.
    assert!(feasible > 2000, "feasible cases: {feasible}");
    assert!(infeasible > 2000, "infeasible cases: {infeasible}");
    assert!(blocked > 100, "Blocked cases: {blocked}");
}

#[test]
fn ac29_exhaustive_for_three_members() {
    // Every subset of the six possible rules among {1,2,3}.
    let pairs = [(1, 2), (1, 3), (2, 1), (2, 3), (3, 1), (3, 2)];
    for mask in 0..64u32 {
        let rules: Vec<(UserId, UserId)> = pairs
            .iter()
            .enumerate()
            .filter(|(i, _)| mask & (1 << i) != 0)
            .map(|(_, p)| *p)
            .collect();
        check(&[1, 2, 3], &rules, &format!("mask {mask}"));
    }
}

#[test]
fn ac29_exhaustive_for_four_members() {
    let mut pairs = vec![];
    for g in 1..=4 {
        for x in 1..=4 {
            if g != x {
                pairs.push((g, x));
            }
        }
    }
    for mask in 0..(1u32 << pairs.len()) {
        let rules: Vec<(UserId, UserId)> = pairs
            .iter()
            .enumerate()
            .filter(|(i, _)| mask & (1 << i) != 0)
            .map(|(_, p)| *p)
            .collect();
        check(&[1, 2, 3, 4], &rules, &format!("mask {mask}"));
    }
}

#[test]
fn ac29_larger_groups_agree_with_a_matching_oracle() {
    let mut rng = Rng(0x29_0002);
    let mut feasible = 0;
    let mut blocked = 0;
    for n in [9usize, 10, 12, 20, 40, 100, 200] {
        let cases = if n <= 20 { 150 } else { 25 };
        for case in 0..cases {
            let (members, rules) = if rng.chance(50) {
                random_instance(&mut rng, n, 300)
            } else {
                // Dense: every giver may draw only a few receivers.
                let mut ids: Vec<UserId> = (1..=n as i32).collect();
                rng.shuffle(&mut ids);
                let keep = 1 + rng.below(3);
                let mut rules = vec![];
                for &g in &ids {
                    let mut others: Vec<UserId> = ids.iter().copied().filter(|x| *x != g).collect();
                    rng.shuffle(&mut others);
                    rules.extend(others.into_iter().skip(keep).map(|x| (g, x)));
                }
                (ids, rules)
            };
            match check(&members, &rules, &format!("n={n} case={case}")) {
                Feasible => feasible += 1,
                Infeasible(Infeasibility::Blocked { .. }) => blocked += 1,
                Infeasible(_) => {}
            }
        }
    }
    assert!(feasible > 20, "feasible large cases: {feasible}");
    assert!(blocked > 5, "Blocked large cases: {blocked}");
}

#[test]
fn ac29_large_constructed_cases() {
    let members: Vec<UserId> = (1..=200).collect();
    assert_eq!(check(&members, &[], "200 without rules"), Feasible);
    // Everyone excludes the next member (a ring): still feasible.
    let ring: Vec<(UserId, UserId)> = (1..=200).map(|i| (i, i % 200 + 1)).collect();
    assert_eq!(check(&members, &ring, "ring"), Feasible);
    // The first 100 members may only draw member 101.
    let mut rules = vec![];
    for g in 1..=100 {
        for x in (1..=200).filter(|x| *x != 101) {
            rules.push((g, x));
        }
    }
    match check(&members, &rules, "100 givers, one receiver") {
        Infeasible(Infeasibility::Blocked { givers, receivers }) => {
            assert!(givers.len() >= 2);
            assert_eq!(receivers, vec![101]);
        }
        other => panic!("expected Blocked, got {other:?}"),
    }
    // One member nobody may draw.
    let rules: Vec<(UserId, UserId)> = (1..=200).filter(|g| *g != 77).map(|g| (g, 77)).collect();
    assert_eq!(
        check(&members, &rules, "nobody may draw 77"),
        Infeasible(Infeasibility::NoGiverFor { receiver: 77 })
    );
}
