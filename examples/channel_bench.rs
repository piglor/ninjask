//! Throughput of the channel path of `ninjask::batch::Batch` without Python:
//! N battles, a random candidate chosen for every pending decision, every
//! request point read (the new channel lines of each player), finished
//! battles restarted until GAMES games have finished. The cost of what a
//! harness that builds its own player view pays: candidates, channels and
//! requests, with no observation.
//!
//! `cargo run --release --example channel_bench -- <dex.json> <teams.txt> [N] [GAMES] [fast|showdown]`
//! `fast` seeds the battles with integers, `showdown` with `sodium,<hex>`
//! seeds (Showdown's generator); the dex needs `format.banner`. Set
//! RAYON_NUM_THREADS to vary the thread count.

use std::sync::Arc;
use std::time::Instant;

use ninjask::batch::{Batch, BatchOptions};
use ninjask::rng::{Rng, Seed};
use ninjask::Dex;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dex = Arc::new(Dex::from_json(&std::fs::read_to_string(&args[1]).unwrap()).unwrap());
    let pool: Vec<String> = std::fs::read_to_string(&args[2])
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(String::from)
        .collect();
    let n: usize = args.get(3).and_then(|x| x.parse().ok()).unwrap_or(256);
    let games: usize = args.get(4).and_then(|x| x.parse().ok()).unwrap_or(2_000);
    let showdown = args.get(5).is_some_and(|m| m == "showdown");
    let options = BatchOptions {
        names: ["p1".into(), "p2".into()],
        channels: true,
        record_draws: false,
    };
    let mut batch = Batch::with_options(dex, n, options).unwrap();
    let mut started = 0usize;
    let seed_of = |k: usize| {
        if showdown {
            Seed::Showdown(format!(
                "sodium,{:032x}",
                (k as u128 + 1) * 0x9e37_79b9_7f4a_7c15
            ))
        } else {
            Seed::Fast(k as u64)
        }
    };
    let start = |b: &mut Batch, i: usize, started: &mut usize| {
        let (a, c) = (
            &pool[(7 * *started) % pool.len()],
            &pool[(7 * *started + 3) % pool.len()],
        );
        b.reset_seeded(i, [a, c], &seed_of(*started), None).unwrap();
        *started += 1;
    };
    for i in 0..n {
        start(&mut batch, i, &mut started);
    }
    let mut policy = Rng::new(1);
    let mut seen = vec![[0usize; 3]; n];
    let (mut finished, mut decisions, mut lines) = (0usize, 0usize, 0usize);
    let t0 = Instant::now();
    while finished < games {
        let pending = batch.pending();
        decisions += pending.len();
        let picks: Vec<(usize, usize, usize)> = pending
            .iter()
            .map(|&(i, s)| {
                let count = batch.candidates(i, s).unwrap().len() as u32;
                (i, s, policy.random(count) as usize)
            })
            .collect();
        batch.choose_candidates(&picks).unwrap();
        for &(i, s, _) in &picks {
            let new = batch.channel(i, s, seen[i][s]).unwrap();
            seen[i][s] += new.len();
            lines += new.len();
        }
        for (i, read) in seen.iter_mut().enumerate() {
            if batch.done(i) {
                finished += 1;
                *read = [0; 3];
                start(&mut batch, i, &mut started);
            }
        }
    }
    let secs = t0.elapsed().as_secs_f64();
    println!(
        "threads {} {}: {finished} games, {decisions} decisions, {lines} lines in {secs:.2}s: {:.0} games/s, {:.0} decisions/s",
        rayon::current_num_threads(),
        if showdown { "showdown seeds" } else { "integer seeds" },
        finished as f64 / secs,
        decisions as f64 / secs,
    );
}
