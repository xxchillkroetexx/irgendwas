//! draw slice: the pure algorithm (AC1-AC13), plus property checks against a brute-force
//! oracle. No database. Only properties and distributions are asserted, never a hard-coded
//! pairing (a `rand` upgrade may change the streams).

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use crate::common::draw::{assert_valid, reciprocal_pairs};
use app::domain::feasibility::{Feasibility, Infeasibility, check_feasibility};
use app::server::draw::{DrawConfig, DrawError, MAX_DRAW_MEMBERS, MIN_DRAW_MEMBERS, draw};
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};

type Pairs = Vec<(i32, i32)>;

fn ids(n: usize) -> Vec<i32> {
    (1..=n as i32).collect()
}

fn run_with(
    members: &[i32],
    rules: &[(i32, i32)],
    cfg: &DrawConfig,
    seed: u64,
) -> Result<Pairs, DrawError> {
    draw(members, rules, cfg, &mut StdRng::seed_from_u64(seed))
}

fn run(members: &[i32], rules: &[(i32, i32)], seed: u64) -> Result<Pairs, DrawError> {
    run_with(members, rules, &DrawConfig::default(), seed)
}

fn fallback_only() -> DrawConfig {
    DrawConfig {
        rejection_attempts: 0,
        ..DrawConfig::default()
    }
}

/// Pearson statistic of `counts` against `expected` per cell.
fn chi_square(counts: &[u64], expected: f64) -> f64 {
    counts
        .iter()
        .map(|&c| {
            let d = c as f64 - expected;
            d * d / expected
        })
        .sum()
}

// ------------------------------------------------------------------------- AC1

#[test]
fn ac1_every_size_2_to_12_gives_a_valid_assignment_for_200_seeds() {
    for n in 2..=12usize {
        let members = ids(n);
        for seed in 0..200 {
            let pairs =
                run(&members, &[], seed).unwrap_or_else(|e| panic!("n={n} seed={seed}: {e:?}"));
            assert_valid(&pairs, &members, &[]);
            assert!(
                pairs.windows(2).all(|w| w[0].0 < w[1].0),
                "result is sorted by giver (n={n}, seed={seed})"
            );
        }
    }
}

#[test]
fn ac1_sizes_up_to_the_cap_are_valid() {
    for n in [20usize, 50, 100, 200] {
        let members = ids(n);
        for seed in 0..5 {
            assert_valid(&run(&members, &[], seed).unwrap(), &members, &[]);
        }
    }
}

// ------------------------------------------------------------------------- AC2

#[test]
fn ac2_input_order_and_duplicates_do_not_matter() {
    for n in [5usize, 12] {
        let members = ids(n);
        let rules: Vec<(i32, i32)> = (1..=n as i32).map(|g| (g, g % n as i32 + 1)).collect();
        for seed in 0..50 {
            let expected = run(&members, &rules, seed).unwrap();
            // Reversed, rotated, duplicated members; reversed, duplicated rules.
            let mut m2: Vec<i32> = members.iter().rev().copied().collect();
            m2.rotate_left(2);
            m2.extend_from_slice(&members[..3]);
            let mut r2: Vec<(i32, i32)> = rules.iter().rev().copied().collect();
            r2.extend_from_slice(&rules[..2]);
            assert_eq!(run(&m2, &r2, seed).unwrap(), expected, "n={n} seed={seed}");
        }
    }
}

#[test]
fn ac2_different_seeds_give_different_results() {
    let members = ids(4);
    let distinct: std::collections::BTreeSet<Pairs> =
        (0..50).map(|s| run(&members, &[], s).unwrap()).collect();
    assert!(distinct.len() >= 2, "50 seeds gave one single result");
}

#[test]
fn g2_the_same_seed_gives_the_same_result() {
    for n in [3usize, 9, 10, 25] {
        let members = ids(n);
        for seed in [0u64, 1, 77] {
            assert_eq!(run(&members, &[], seed), run(&members, &[], seed));
            assert_eq!(
                run_with(&members, &[], &fallback_only(), seed),
                run_with(&members, &[], &fallback_only(), seed)
            );
        }
    }
}

// ------------------------------------------------------------------------- AC3

#[test]
fn ac3_two_members_give_to_each_other_for_every_seed() {
    for seed in 0..100 {
        assert_eq!(run(&[7, 3], &[], seed).unwrap(), vec![(3, 7), (7, 3)]);
    }
}

// ------------------------------------------------------------------------- AC4

#[test]
fn ac4_four_members_get_one_of_the_six_four_cycles_uniformly() {
    let members = ids(4);
    let mut counts: BTreeMap<Pairs, u64> = BTreeMap::new();
    for seed in 0..60_000 {
        let pairs = run(&members, &[], seed).unwrap();
        assert_valid(&pairs, &members, &[]);
        assert_eq!(
            reciprocal_pairs(&pairs),
            0,
            "two reciprocal pairs: {pairs:?}"
        );
        *counts.entry(pairs).or_default() += 1;
    }
    assert_eq!(counts.len(), 6, "exactly the six four-cycles occur");
    let cells: Vec<u64> = counts.values().copied().collect();
    let stat = chi_square(&cells, 10_000.0);
    assert!(stat < 20.52, "chi-square {stat} (counts {cells:?})");
}

// ------------------------------------------------------------------------- AC5

#[test]
fn ac5_couples_excluded_both_ways_give_the_two_four_cycles_half_each() {
    let (a, b, c, d) = (1, 2, 3, 4);
    let rules = [(a, b), (b, a), (c, d), (d, c)];
    let cycle1: Pairs = vec![(a, c), (b, d), (c, b), (d, a)]; // A->C->B->D->A
    let cycle2: Pairs = vec![(a, d), (b, c), (c, a), (d, b)]; // A->D->B->C->A
    let mut first = 0u32;
    for seed in 0..10_000 {
        let pairs = run(&ids(4), &rules, seed).unwrap();
        if pairs == cycle1 {
            first += 1;
        } else {
            assert_eq!(pairs, cycle2, "seed {seed}");
        }
    }
    let share = f64::from(first) / 10_000.0;
    assert!((0.48..=0.52).contains(&share), "first cycle share {share}");
}

// ------------------------------------------------------------------------- AC6

#[test]
fn ac6_one_forbidden_pair_in_a_trio_leaves_one_assignment() {
    for seed in 0..100 {
        let pairs = run(&ids(3), &[(1, 2)], seed).unwrap();
        assert_eq!(pairs, vec![(1, 3), (2, 1), (3, 2)], "A->C, B->A, C->B");
    }
}

#[test]
fn g3_an_unconstrained_trio_gives_both_three_cycles_evenly() {
    let mut forward = 0u32;
    for seed in 0..4_000 {
        let pairs = run(&ids(3), &[], seed).unwrap();
        assert_eq!(reciprocal_pairs(&pairs), 0);
        if pairs == vec![(1, 2), (2, 3), (3, 1)] {
            forward += 1;
        }
    }
    assert!(
        (1_800..=2_200).contains(&forward),
        "forward cycle {forward}/4000"
    );
}

// ------------------------------------------------------------------------- AC7

#[test]
fn ac7_a_forced_reciprocal_pair_is_the_only_one() {
    let (a, b) = (1, 2);
    let mut rules = vec![];
    for blocked in 3..=6 {
        rules.push((a, blocked));
        rules.push((b, blocked));
    }
    for seed in 0..1_000 {
        let pairs = run(&ids(6), &rules, seed).unwrap();
        assert_valid(&pairs, &ids(6), &rules);
        assert!(pairs.contains(&(a, b)) && pairs.contains(&(b, a)));
        assert_eq!(reciprocal_pairs(&pairs), 1, "seed {seed}: {pairs:?}");
    }
}

// ------------------------------------------------------------------------- AC8

#[test]
fn ac8_ten_members_have_no_reciprocal_pair_and_a_uniform_first_receiver() {
    let members = ids(10);
    let mut first_receiver = [0u64; 11];
    for seed in 0..20_000 {
        let pairs = run(&members, &[], seed).unwrap();
        assert_valid(&pairs, &members, &[]);
        assert_eq!(reciprocal_pairs(&pairs), 0, "seed {seed}: {pairs:?}");
        first_receiver[pairs[0].1 as usize] += 1;
    }
    assert_eq!(
        first_receiver[1], 0,
        "the first member never draws themselves"
    );
    let stat = chi_square(&first_receiver[2..], 20_000.0 / 9.0);
    assert!(stat < 26.12, "chi-square {stat} ({first_receiver:?})");
}

#[test]
fn g4_the_large_tier_avoids_reciprocal_pairs_when_it_can() {
    for n in [10usize, 11, 15, 30] {
        let members = ids(n);
        for seed in 0..300 {
            let pairs = run(&members, &[], seed).unwrap();
            assert_eq!(reciprocal_pairs(&pairs), 0, "n={n} seed={seed}");
        }
    }
}

#[test]
fn g3_nine_members_still_use_the_exact_tier() {
    // 9 is the last exact size: no reciprocal pair, uniform first receiver.
    let members = ids(9);
    let mut first_receiver = [0u64; 10];
    for seed in 0..2_000 {
        let pairs = run(&members, &[], seed).unwrap();
        assert_eq!(reciprocal_pairs(&pairs), 0);
        first_receiver[pairs[0].1 as usize] += 1;
    }
    let stat = chi_square(&first_receiver[2..], 2_000.0 / 8.0);
    assert!(stat < 26.12, "chi-square {stat} ({first_receiver:?})");
}

// ------------------------------------------------------------------------- AC9

/// A random group with rules: sparse, dense (each member may draw only 2-3 others), or with
/// a planted blocker.
fn random_rule_set(rng: &mut StdRng) -> (Vec<i32>, Vec<(i32, i32)>) {
    let n = rng.random_range(10..=30usize);
    let members = ids(n);
    let mut rules = vec![];
    let kind = rng.random_range(0..10);
    for &g in &members {
        let others: Vec<i32> = members.iter().copied().filter(|&o| o != g).collect();
        match kind {
            0..=4 | 8 | 9 => {
                let k = rng.random_range(0..=3usize);
                let mut pool = others;
                pool.shuffle(rng);
                rules.extend(pool.into_iter().take(k).map(|o| (g, o)));
            }
            _ => {
                let allowed = rng.random_range(2..=3usize);
                let mut pool = others;
                pool.shuffle(rng);
                rules.extend(pool.into_iter().skip(allowed).map(|o| (g, o)));
            }
        }
    }
    if kind == 8 {
        // Member 1 may draw nobody.
        rules.extend(members.iter().filter(|&&o| o != 1).map(|&o| (1, o)));
    }
    if kind == 9 {
        // Members 1-3 may only draw 4 and 5: a Hall violation.
        for g in 1..=3 {
            rules.retain(|&(giver, _)| giver != g);
            rules.extend(
                members
                    .iter()
                    .filter(|&&o| o != g && o != 4 && o != 5)
                    .map(|&o| (g, o)),
            );
        }
    }
    rules.shuffle(rng);
    (members, rules)
}

#[test]
fn ac9_the_rule_set_generator_yields_feasible_and_infeasible_sets() {
    let mut rng = StdRng::seed_from_u64(9);
    let (mut feasible, mut infeasible) = (0, 0);
    for _ in 0..500 {
        let (members, rules) = random_rule_set(&mut rng);
        match check_feasibility(&members, &rules) {
            Feasibility::Feasible => feasible += 1,
            Feasibility::Infeasible(_) => infeasible += 1,
        }
    }
    assert!(feasible >= 100, "only {feasible} feasible sets");
    assert!(infeasible >= 50, "only {infeasible} infeasible sets");
}

#[test]
fn ac9_the_fallback_is_valid_on_feasible_sets_and_reports_the_reason_on_infeasible_ones() {
    let mut rng = StdRng::seed_from_u64(9);
    for round in 0..500 {
        let (members, rules) = random_rule_set(&mut rng);
        let result = run_with(&members, &rules, &fallback_only(), round);
        match check_feasibility(&members, &rules) {
            Feasibility::Feasible => {
                let pairs = result.unwrap_or_else(|e| panic!("round {round}: {e:?}"));
                assert_valid(&pairs, &members, &rules);
            }
            Feasibility::Infeasible(reason) => {
                assert_eq!(result, Err(DrawError::Infeasible(reason)), "round {round}");
            }
        }
    }
}

#[test]
fn ac9_the_default_config_agrees_with_the_feasibility_check_too() {
    let mut rng = StdRng::seed_from_u64(99);
    for round in 0..200 {
        let (members, rules) = random_rule_set(&mut rng);
        let result = run(&members, &rules, round);
        match check_feasibility(&members, &rules) {
            Feasibility::Feasible => assert_valid(&result.unwrap(), &members, &rules),
            Feasibility::Infeasible(reason) => {
                assert_eq!(result, Err(DrawError::Infeasible(reason)))
            }
        }
    }
}

// ------------------------------------------------------------------------ AC10

#[test]
fn ac10_too_few_and_too_many_members() {
    assert_eq!(MIN_DRAW_MEMBERS, 2);
    assert_eq!(MAX_DRAW_MEMBERS, 200);
    assert_eq!(
        run(&[1], &[], 0),
        Err(DrawError::TooFewMembers { count: 1, min: 2 })
    );
    assert_eq!(
        run(&[], &[], 0),
        Err(DrawError::TooFewMembers { count: 0, min: 2 })
    );
    // Duplicates do not count.
    assert_eq!(
        run(&[4, 4], &[], 0),
        Err(DrawError::TooFewMembers { count: 1, min: 2 })
    );
    assert_eq!(
        run(&ids(201), &[], 0),
        Err(DrawError::TooManyMembers {
            count: 201,
            max: 200
        })
    );
    assert!(run(&ids(200), &[], 0).is_ok());
}

// ------------------------------------------------------------------------ AC11

#[test]
fn ac11_the_reason_is_exactly_the_one_the_feasibility_check_gives() {
    let (alice, bob, carol) = (1, 2, 3);
    assert_eq!(
        run(&[alice, bob, carol], &[(bob, alice), (bob, carol)], 0),
        Err(DrawError::Infeasible(Infeasibility::NoReceiverFor {
            giver: bob
        }))
    );
    assert_eq!(
        run(&[alice, bob, carol], &[(alice, bob), (carol, bob)], 0),
        Err(DrawError::Infeasible(Infeasibility::NoGiverFor {
            receiver: bob
        }))
    );
    // A Hall violation: A and B may only draw C.
    assert!(matches!(
        run(&ids(4), &[(1, 2), (1, 4), (2, 1), (2, 4)], 0),
        Err(DrawError::Infeasible(Infeasibility::Blocked { .. }))
    ));
}

#[test]
fn ac11_too_many_members_is_reported_before_feasibility() {
    let mut rules = vec![];
    for blocked in 2..=201 {
        rules.push((1, blocked));
    }
    assert_eq!(
        run(&ids(201), &rules, 0),
        Err(DrawError::TooManyMembers {
            count: 201,
            max: 200
        })
    );
}

// ------------------------------------------------------------------------ AC12

#[test]
fn ac12_rules_naming_non_members_or_self_are_ignored() {
    for n in [5usize, 12] {
        let members = ids(n);
        let clean = [(1, 2), (3, 4)];
        let noisy = [(1, 2), (3, 4), (1, 99), (99, 1), (2, 2), (77, 88), (5, 5)];
        for seed in 0..50 {
            assert_eq!(
                run(&members, &noisy, seed),
                run(&members, &clean, seed),
                "n={n} seed={seed}"
            );
        }
    }
}

// ------------------------------------------------------------------------ AC13

fn dense_rules_200(seed: u64) -> (Vec<i32>, Vec<(i32, i32)>) {
    let mut rng = StdRng::seed_from_u64(seed);
    let members = ids(200);
    let mut rules = vec![];
    for &g in &members {
        let mut pool: Vec<i32> = members.iter().copied().filter(|&o| o != g).collect();
        pool.shuffle(&mut rng);
        rules.extend(pool.into_iter().take(10).map(|o| (g, o)));
    }
    (members, rules)
}

#[test]
fn ac13_200_members_with_10_rules_each_draw_in_under_5_seconds() {
    let (members, rules) = dense_rules_200(13);
    assert_eq!(check_feasibility(&members, &rules), Feasibility::Feasible);
    for (what, cfg) in [
        ("default", DrawConfig::default()),
        ("fallback", fallback_only()),
    ] {
        let started = Instant::now();
        let pairs = run_with(&members, &rules, &cfg, 1).unwrap();
        let took = started.elapsed();
        assert!(took < Duration::from_secs(5), "{what}: {took:?}");
        assert_valid(&pairs, &members, &rules);
    }
}

// ------------------------------------------------- oracle: G1 + G3 by brute force

fn permutations(n: usize) -> Vec<Vec<usize>> {
    fn go(prefix: &mut Vec<usize>, used: &mut Vec<bool>, out: &mut Vec<Vec<usize>>) {
        let n = used.len();
        if prefix.len() == n {
            out.push(prefix.clone());
            return;
        }
        for i in 0..n {
            if !used[i] {
                used[i] = true;
                prefix.push(i);
                go(prefix, used, out);
                prefix.pop();
                used[i] = false;
            }
        }
    }
    let mut out = vec![];
    go(&mut vec![], &mut vec![false; n], &mut out);
    out
}

/// All valid assignments with the fewest reciprocal pairs (empty: none valid).
fn oracle(members: &[i32], rules: &[(i32, i32)]) -> Vec<Pairs> {
    let valid: Vec<Pairs> = permutations(members.len())
        .into_iter()
        .map(|p| {
            p.iter()
                .enumerate()
                .map(|(i, &j)| (members[i], members[j]))
                .collect::<Pairs>()
        })
        .filter(|pairs| {
            pairs
                .iter()
                .all(|(g, r)| g != r && !rules.contains(&(*g, *r)))
        })
        .collect();
    let best = valid.iter().map(|p| reciprocal_pairs(p)).min();
    valid
        .into_iter()
        .filter(|p| Some(reciprocal_pairs(p)) == best)
        .collect()
}

#[test]
fn g1_g3_small_groups_match_a_brute_force_oracle() {
    let mut rng = StdRng::seed_from_u64(31);
    let (mut some_valid, mut none_valid) = (0, 0);
    for round in 0..400u64 {
        let n = rng.random_range(2..=6usize);
        let members = ids(n);
        let mut rules = vec![];
        for _ in 0..rng.random_range(0..=2 * n) {
            rules.push((
                rng.random_range(1..=n as i32),
                rng.random_range(1..=n as i32),
            ));
        }
        let best = oracle(&members, &rules);
        for seed in 0..3 {
            let result = run(&members, &rules, round * 10 + seed);
            if best.is_empty() {
                assert!(
                    matches!(result, Err(DrawError::Infeasible(_))),
                    "round {round}: expected infeasible, got {result:?}"
                );
            } else {
                let pairs = result.unwrap_or_else(|e| panic!("round {round}: {e:?}"));
                assert!(
                    best.contains(&pairs),
                    "round {round}: {pairs:?} is not a valid assignment with the fewest \
                     reciprocal pairs (rules {rules:?})"
                );
            }
        }
        if best.is_empty() {
            none_valid += 1;
        } else {
            some_valid += 1;
        }
    }
    assert!(
        some_valid > 100 && none_valid > 10,
        "{some_valid}/{none_valid}"
    );
}

#[test]
fn g3_a_constrained_group_is_exactly_uniform_over_its_best_assignments() {
    // 5 members, a few rules: the oracle's set must be hit evenly.
    let members = ids(5);
    let rules = [(1, 2), (2, 3), (4, 5), (5, 1)];
    let best = oracle(&members, &rules);
    assert!(
        best.len() >= 3,
        "fixture has {} best assignments",
        best.len()
    );
    let mut counts: BTreeMap<Pairs, u64> = BTreeMap::new();
    let runs = 3_000 * best.len() as u64;
    for seed in 0..runs {
        let pairs = run(&members, &rules, seed).unwrap();
        assert!(best.contains(&pairs));
        *counts.entry(pairs).or_default() += 1;
    }
    let cells: Vec<u64> = best
        .iter()
        .map(|p| counts.get(p).copied().unwrap_or(0))
        .collect();
    let df = (best.len() - 1) as f64;
    let stat = chi_square(&cells, runs as f64 / best.len() as f64);
    assert!(
        stat < df + 8.0 * (2.0 * df).sqrt(),
        "chi-square {stat}, counts {cells:?}"
    );
}

#[test]
fn g1_larger_groups_with_random_rules_honour_every_rule() {
    let mut rng = StdRng::seed_from_u64(5);
    let mut checked = 0;
    for round in 0..150u64 {
        let (members, rules) = random_rule_set(&mut rng);
        if check_feasibility(&members, &rules) == Feasibility::Feasible {
            for seed in 0..3 {
                let pairs = run(&members, &rules, round + seed).unwrap();
                assert_valid(&pairs, &members, &rules);
                checked += 1;
            }
        }
    }
    assert!(checked > 100);
}
