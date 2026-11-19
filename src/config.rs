use std::{env, path::PathBuf};

#[derive(Clone, Debug)]
pub struct Config {
    pub database_path: PathBuf,
    pub bind_addr: String,
    pub base_url: String,
    pub list_title: String,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            database_path: env::var_os("DATABASE_PATH")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("2buy.sqlite3")),
            bind_addr: env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".to_string()),
            base_url: env::var("BASE_URL")
                .unwrap_or_else(|_| "http://localhost:8080".to_string())
                .trim_end_matches('/')
                .to_string(),
            list_title: env::var("LIST_TITLE").unwrap_or_else(|_| "Shopping list".to_string()),
        }
    }
}
