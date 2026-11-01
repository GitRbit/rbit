//! Manual route lookup comparison using only public registry APIs.
//! Excludes registration, admission, callback execution, HTTP and database work.

use std::{
    hint::black_box,
    time::{Duration, Instant},
};

use askama::Template;
use rbit_plugin_api::{Generation, PluginId};
use rbit_plugin_runtime::PluginScope;
use rbit_plugin_web::{
    CompiledPage, FormSchema, MutationAudience, MutationDescriptor, MutationOutcome, MutationRegistry, PageAudience, PageDescriptor,
    PageRegistry, PageScope, compiled_mutation, compiled_page,
};

const WARMUP_BATCHES: usize = 32;
const SAMPLES: usize = 256;
const BATCH_OPERATIONS: u32 = 1_000;
const HIT_PATH: &str = "overview";
const MISS_PATH: &str = "missing";

#[derive(Template)]
#[template(source = "<p>Lookup fixture</p>", ext = "html")]
struct Fragment;

fn measure<T>(mut operation: impl FnMut() -> T) -> (f64, f64) {
    let mut samples = Vec::with_capacity(SAMPLES);
    for sample in 0..WARMUP_BATCHES + SAMPLES {
        let started = Instant::now();
        for _ in 0..BATCH_OPERATIONS {
            // Include destruction of returned handles and their snapshot references.
            drop(black_box(operation()));
        }
        let elapsed = started.elapsed();
        if sample >= WARMUP_BATCHES {
            samples.push(elapsed.as_secs_f64() * 1_000_000_000.0 / f64::from(BATCH_OPERATIONS));
        }
    }
    samples.sort_unstable_by(f64::total_cmp);
    (samples[SAMPLES / 2], samples[SAMPLES * 95 / 100])
}

fn report<T>(registry: &str, unrelated: usize, mode: &str, result: &str, operation: impl FnMut() -> T) {
    let (p50, p95) = measure(operation);
    println!("{registry} unrelated_owners={unrelated} {mode} {result}: p50={p50:.2}ns/op p95={p95:.2}ns/op");
}

/// Run with `cargo test --release --locked -p rbit-plugin-web --test lookup_benchmark -- --ignored --nocapture`.
#[test]
#[ignore = "manual release benchmark; no hardware-dependent CI threshold"]
fn route_lookup_cost_by_unrelated_owner_count() {
    assert!(!black_box(cfg!(debug_assertions)), "run this benchmark with --release");
    println!(
        "{SAMPLES} warmed batch samples, {BATCH_OPERATIONS} operations/batch, {WARMUP_BATCHES} warmup batches; percentiles of batch-average ns/op; one route per owner; target owner lexically last; miss uses an absent path on the target owner"
    );
    let target: PluginId = "example.zzz-target".parse().unwrap();
    for unrelated in [0, 10, 100] {
        let pages = PageRegistry::new();
        let mutations = MutationRegistry::new();
        let mut scopes = Vec::with_capacity(unrelated + 1);
        // Keep registrations as well as scopes alive throughout all measurements.
        let mut page_registrations = Vec::with_capacity(unrelated + 1);
        let mut mutation_registrations = Vec::with_capacity(unrelated + 1);
        for index in 0..=unrelated {
            let owner = if index == unrelated { target.clone() } else { format!("example.owner-{index:03}").parse().unwrap() };
            let scope = PluginScope::new(Generation::INITIAL);
            let page = compiled_page(
                PageDescriptor::new("overview", PageScope::Global, HIT_PATH, PageAudience::Public, Duration::from_secs(1)).unwrap(),
                |_| async { CompiledPage::new("Lookup", Fragment) },
            );
            let mutation = compiled_mutation(
                MutationDescriptor::new(
                    "overview",
                    PageScope::Global,
                    HIT_PATH,
                    MutationAudience::SignedIn,
                    Duration::from_secs(1),
                    FormSchema::new(vec![]).unwrap(),
                )
                .unwrap(),
                |_, _| async { Ok(MutationOutcome::Invalid(CompiledPage::new("Lookup", Fragment)?)) },
            );
            let page_registration = pages.stage(owner.clone(), &scope, scope.generation(), vec![page]).unwrap();
            page_registration.publish().unwrap();
            let mutation_registration = mutations.stage(owner, &scope, scope.generation(), vec![mutation]).unwrap();
            mutation_registration.publish().unwrap();
            scopes.push(scope);
            page_registrations.push(page_registration);
            mutation_registrations.push(mutation_registration);
        }
        let page_snapshot = pages.snapshot();
        let mutation_snapshot = mutations.snapshot();
        assert_eq!(page_snapshot.len(), unrelated + 1);
        assert_eq!(mutation_snapshot.len(), unrelated + 1);
        for (result, path) in [("hit", HIT_PATH), ("miss", MISS_PATH)] {
            assert_eq!(page_snapshot.find(&target, PageScope::Global, path).is_some(), result == "hit");
            assert_eq!(mutation_snapshot.find(&target, PageScope::Global, path).is_some(), result == "hit");
            report("pages", unrelated, "snapshot+lookup", result, || {
                pages.snapshot().find(black_box(&target), PageScope::Global, black_box(path))
            });
            report("pages", unrelated, "retained-snapshot lookup", result, || {
                page_snapshot.find(black_box(&target), PageScope::Global, black_box(path))
            });
            report("mutations", unrelated, "snapshot+lookup", result, || {
                mutations.snapshot().find(black_box(&target), PageScope::Global, black_box(path))
            });
            report("mutations", unrelated, "retained-snapshot lookup", result, || {
                mutation_snapshot.find(black_box(&target), PageScope::Global, black_box(path))
            });
        }
        assert!(scopes.iter().all(|scope| scope.in_flight() == 0));
    }
}
