mod auth;
mod config;
mod db;
mod web;

use clap::{Parser, Subcommand};
use config::Config;
use std::{error::Error, process::exit};

#[derive(Parser)]
#[command(name = "2buy", about = "A tiny shared shopping list")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run the HTTP server
    Serve,
    /// Generate a new user URL
    GenerateUrl { username: String },
    /// List generated tokens
    ListTokens {
        #[arg(long)]
        username: Option<String>,
    },
    /// Print the stored URL for a token ID
    ShowUrl { token_id: String },
    /// Permanently revoke one token
    Revoke { token_id: String },
    /// List items (admin debugging)
    ListItems,
}

fn url(base: &str, username: &str, token: &str) -> String {
    format!("{base}/u/{username}/{token}")
}

fn run() -> Result<(), Box<dyn Error>> {
    let config = Config::from_env();
    let cli = Cli::parse();
    let mut conn = db::open(&config.database_path)?;

    match cli.command {
        Commands::Serve => {
            drop(conn);
            web::run(config)?;
        }
        Commands::GenerateUrl { username } => {
            if !auth::valid_username(&username) {
                return Err("invalid username (use lowercase letters, digits, '-' or '_')".into());
            }
            let token = db::create_token(&mut conn, &username)?;
            println!("Token ID: {}", token.id);
            println!("URL: {}", url(&config.base_url, &token.username, &token.token));
        }
        Commands::ListTokens { username } => {
            let tokens = db::list_tokens(&conn, username.as_deref())?;
            println!(
                "{:<10} {:<12} {:<10} {:<25} {}",
                "ID", "USERNAME", "TOKEN", "CREATED", "STATUS"
            );
            for token in tokens {
                println!("{}", db::token_display(&token));
            }
        }
        Commands::ShowUrl { token_id } => {
            let token = db::find_token(&conn, &token_id)?
                .ok_or_else(|| format!("token not found: {token_id}"))?;
            println!("{}", url(&config.base_url, &token.username, &token.token));
        }
        Commands::Revoke { token_id } => {
            if !db::revoke_token(&conn, &token_id)? {
                return Err(format!("token not found: {token_id}").into());
            }
            println!("Revoked {token_id}");
        }
        Commands::ListItems => {
            for item in db::list_items(&conn)? {
                println!(
                    "#{} [{}] {} — {} {} (by {}, created {}, updated {})",
                    item.id,
                    if item.disabled { "off" } else { "on" },
                    item.name,
                    item.quantity,
                    item.category,
                    item.created_by,
                    item.created_at,
                    item.updated_at
                );
                if !item.comment.is_empty() {
                    println!("    comment: {}", item.comment);
                }
            }
        }
    }
    Ok(())
}

fn main() {
    if let Err(err) = run() {
        eprintln!("error: {err}");
        exit(1);
    }
}
