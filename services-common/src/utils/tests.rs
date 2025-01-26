pub fn logger_setup() {
    let subscriber = tracing_subscriber::fmt().with_test_writer().finish();
    tracing::subscriber::set_global_default(subscriber).unwrap();
}
