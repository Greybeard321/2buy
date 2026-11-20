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

    /// Public path prefix from `BASE_URL`, e.g. `https://host/2buy` -> `/2buy`.
    /// A reverse proxy strips this before forwarding requests to 2buy, but the
    /// browser still sees it, so redirects must preserve it.
    pub fn public_prefix(&self) -> &str {
        let after_scheme = self
            .base_url
            .split_once("://")
            .map(|(_, rest)| rest)
            .unwrap_or_else(|| self.base_url.as_str());
        match after_scheme.find('/') {
            Some(index) => &after_scheme[index..],
            None => "",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(base_url: &str) -> Config {
        Config {
            database_path: PathBuf::from("test.sqlite3"),
            bind_addr: "127.0.0.1:8080".to_string(),
            base_url: base_url.trim_end_matches('/').to_string(),
            list_title: "Shopping list".to_string(),
        }
    }

    #[test]
    fn public_prefix_comes_from_base_url() {
        assert_eq!(config("http://localhost:8080").public_prefix(), "");
        assert_eq!(config("https://shop.example.com").public_prefix(), "");
        assert_eq!(
            config("https://shop.example.com/2buy").public_prefix(),
            "/2buy"
        );
        assert_eq!(
            config("https://shop.example.com/2buy/").public_prefix(),
            "/2buy"
        );
        assert_eq!(
            config("https://shop.example.com/shop/2buy").public_prefix(),
            "/shop/2buy"
        );
        assert_eq!(config("shop.example.com/2buy").public_prefix(), "/2buy");
    }
}
