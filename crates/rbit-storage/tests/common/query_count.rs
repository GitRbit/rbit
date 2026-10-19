//! SQL query work assertions scoped to one future, including `SQLite`'s worker threads.
//! Connection setup pragmas are excluded: pool growth is not application query work.

use std::{
    collections::HashMap,
    future::Future,
    sync::{
        Arc, Mutex, OnceLock, Weak,
        atomic::{AtomicU64, Ordering},
    },
};

use tracing::{
    Event, Instrument, Span, Subscriber,
    field::{Field, Visit},
    span::{Attributes, Id},
};
use tracing_subscriber::{Layer, layer::Context, prelude::*, registry::LookupSpan};

type Queries = Mutex<Vec<(String, u64)>>;
type Pending = Arc<Mutex<HashMap<u64, Weak<Queries>>>>;

#[derive(Default)]
struct Fields {
    capture: Option<u64>,
    summary: String,
    statement: String,
    rows: u64,
}

impl Visit for Fields {
    fn record_u64(&mut self, field: &Field, value: u64) {
        match field.name() {
            "query_capture" => self.capture = Some(value),
            "rows_affected" => self.rows = value,
            _ => {}
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        match field.name() {
            "summary" => value.clone_into(&mut self.summary),
            "db.statement" => value.trim().clone_into(&mut self.statement),
            _ => {}
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if matches!(field.name(), "summary" | "db.statement") {
            self.record_str(field, &format!("{value:?}"));
        }
    }
}

struct QueryLayer(Pending);

impl<S: Subscriber + for<'span> LookupSpan<'span>> Layer<S> for QueryLayer {
    fn on_new_span(&self, attributes: &Attributes<'_>, id: &Id, context: Context<'_, S>) {
        let mut fields = Fields::default();
        attributes.record(&mut fields);
        if let Some(queries) = fields.capture.and_then(|capture| self.0.lock().unwrap().get(&capture).cloned()) {
            context.span(id).unwrap().extensions_mut().insert(queries);
        }
    }

    fn on_event(&self, event: &Event<'_>, context: Context<'_, S>) {
        if event.metadata().target() != "sqlx::query" {
            return;
        }
        let Some(scope) = context.event_scope(event) else { return };
        for span in scope {
            if let Some(queries) = span.extensions().get::<Weak<Queries>>().and_then(Weak::upgrade) {
                let mut fields = Fields::default();
                event.record(&mut fields);
                let sql = if fields.statement.is_empty() { fields.summary } else { fields.statement };
                // SQLx may lazily open a pooled connection inside the measured span.
                // Its initialization emits a multi-statement PRAGMA batch as a query.
                if sql.split_whitespace().next().is_some_and(|keyword| keyword.eq_ignore_ascii_case("PRAGMA")) {
                    return;
                }
                queries.lock().unwrap().push((sql, fields.rows));
                break;
            }
        }
    }
}

pub struct QueryCounter {
    span: Span,
    queries: Arc<Queries>,
}

impl QueryCounter {
    pub fn new() -> Self {
        static PENDING: OnceLock<Pending> = OnceLock::new();
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let pending = PENDING.get_or_init(|| {
            let pending = Arc::new(Mutex::new(HashMap::new()));
            tracing::subscriber::set_global_default(tracing_subscriber::registry().with(QueryLayer(Arc::clone(&pending)))).unwrap();
            pending
        });
        let queries = Arc::new(Mutex::new(Vec::new()));
        let capture = NEXT.fetch_add(1, Ordering::Relaxed);
        pending.lock().unwrap().insert(capture, Arc::downgrade(&queries));
        let span = tracing::info_span!("count_queries", query_capture = capture);
        pending.lock().unwrap().remove(&capture);
        Self { span, queries }
    }

    pub async fn measure<F: Future>(&self, future: F) -> F::Output {
        future.instrument(self.span.clone()).await
    }

    /// Application SQL statements and rows affected, without connection setup pragmas,
    /// timing or query parameters.
    pub fn queries(&self) -> Vec<(String, u64)> {
        self.queries.lock().unwrap().clone()
    }
}
