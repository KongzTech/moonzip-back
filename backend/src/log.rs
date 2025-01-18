use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

pub fn setup_log() {
    let trace_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "backend=debug,tower_http=info,axum::rejection=trace,axum=info".into());
    tracing_subscriber::registry()
        .with(trace_filter)
        .with(tracing_subscriber::fmt::layer())
        .init();
}
