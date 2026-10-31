//! Manual comparison of compiled rendering and generation-scoped page dispatch.
//! This excludes HTTP middleware, permissions, database work and network latency.

use std::{
    hint::black_box,
    time::{Duration, Instant},
};

use askama::Template;
use rbit_messages::LocaleSnapshot;
use rbit_plugin_api::{Generation, PluginId};
use rbit_plugin_runtime::PluginScope;
use rbit_plugin_web::{CompiledPage, PageAudience, PageContext, PageDescriptor, PageRegistry, PageScope, compiled_page};

#[derive(Template)]
#[template(source = "<section><h2>Overview</h2><p>{{ message }}</p></section>", ext = "html")]
struct Fragment {
    message: &'static str,
}

const MESSAGE: &str = "Compiled Askama <escapes> dynamic values & retains the host shell.";

fn percentiles(samples: &mut [u128]) -> (u128, u128) {
    samples.sort_unstable();
    (samples[samples.len() / 2], samples[samples.len() * 95 / 100])
}

/// Run with `cargo test --release --locked -p rbit-plugin-web --test routes_benchmark -- --ignored --nocapture`.
#[tokio::test]
#[ignore = "manual rendering benchmark; no hardware-dependent CI threshold"]
async fn compiled_page_dispatch_cost() {
    let registry = PageRegistry::new();
    let scope = PluginScope::new(Generation::INITIAL);
    let owner: PluginId = "example.performance".parse().unwrap();
    let pages = (0..16)
        .map(|index| {
            let id = format!("page-{index}");
            let descriptor = PageDescriptor::new(&id, PageScope::Global, &id, PageAudience::Public, Duration::from_secs(1)).unwrap();
            compiled_page(descriptor, |_| async { CompiledPage::new("Overview", Fragment { message: MESSAGE }) })
        })
        .collect();
    let registration = registry.stage(owner.clone(), &scope, scope.generation(), pages).unwrap();
    registration.publish().unwrap();
    let context = PageContext { actor: None, repository: None, locale: LocaleSnapshot::english(0) };
    let expected = Fragment { message: MESSAGE }.render().unwrap();
    let mut direct = Vec::with_capacity(1000);
    let mut dispatch = Vec::with_capacity(1000);
    let mut empty = Vec::with_capacity(1000);
    let empty_registry = PageRegistry::new();
    for sample in 0..1100 {
        let started = Instant::now();
        black_box(Fragment { message: black_box(MESSAGE) }.render().unwrap());
        let elapsed = started.elapsed().as_nanos();
        if sample >= 100 {
            direct.push(elapsed);
        }

        let started = Instant::now();
        let snapshot = registry.snapshot();
        let route = snapshot.find(&owner, PageScope::Global, "page-15").unwrap();
        let mut lease = route.admit().unwrap();
        let body = lease.render(context.clone()).await.unwrap();
        let elapsed = started.elapsed().as_nanos();
        assert_eq!(body.html(), expected);
        black_box(body);
        drop(lease);
        if sample >= 100 {
            dispatch.push(elapsed);
        }

        let started = Instant::now();
        black_box(empty_registry.snapshot().find(&owner, PageScope::Global, "page-15"));
        let elapsed = started.elapsed().as_nanos();
        if sample >= 100 {
            empty.push(elapsed);
        }
    }
    let (core_p50, core_p95) = percentiles(&mut direct);
    let (plugin_p50, plugin_p95) = percentiles(&mut dispatch);
    let (empty_p50, empty_p95) = percentiles(&mut empty);
    println!(
        "1000 warmed samples, identical compiled Askama HTML; direct p50={core_p50}ns p95={core_p95}ns; snapshot+lookup(last of 16)+admit+bounded render p50={plugin_p50}ns p95={plugin_p95}ns; empty snapshot+lookup p50={empty_p50}ns p95={empty_p95}ns"
    );
}
