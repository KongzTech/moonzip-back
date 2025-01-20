use serde::de::DeserializeOwned;

pub fn load_config<T: DeserializeOwned>() -> T {
    let run_mode = std::env::var("APP_RUN_MODE").unwrap_or_else(|_| "dev".into());

    config::Config::builder()
        .add_source(config::File::with_name("config/default").required(false))
        .add_source(config::File::with_name(&format!("config/{}", run_mode)).required(false))
        .add_source(config::File::with_name("config/local").required(false))
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
