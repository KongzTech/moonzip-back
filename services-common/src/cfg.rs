use serde::de::DeserializeOwned;

pub fn load_config<T: DeserializeOwned>() -> T {
    let app_name = std::env::var("APP_NAME").expect("pass APP_NAME env to load correct config");
    let run_mode = std::env::var("APP_RUN_MODE").unwrap_or_else(|_| "dev".into());
    let base_path = format!("config/{}", run_mode);

    config::Config::builder()
        .add_source(config::File::with_name(&format!("{base_path}/base")).required(false))
        .add_source(config::File::with_name(&format!("{base_path}/{app_name}")).required(true))
        .add_source(
            config::File::with_name(&format!("{base_path}/{app_name}.local")).required(false),
        )
        .add_source(
            config::Environment::default()
                .prefix("APP")
                .prefix_separator("_")
                .separator("__"),
        )
        .build()
        .unwrap()
        .try_deserialize::<T>()
        .unwrap()
}
