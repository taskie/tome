use anyhow::Result;
use clap::Args;
use serde::{Deserialize, Serialize};
use tracing::info;

#[derive(Args)]
pub struct InitArgs {
    /// URL of the central tome-server for machine_id registration
    #[arg(long)]
    pub server: String,

    /// Machine name to register
    #[arg(long)]
    pub name: Option<String>,

    /// Machine description
    #[arg(long, default_value = "")]
    pub description: String,

    /// Overwrite existing machine_id in config
    #[arg(long)]
    pub force: bool,
}

#[derive(Serialize)]
struct RegisterRequest {
    name: String,
    description: String,
}

#[derive(Deserialize)]
struct MachineResponse {
    machine_id: i16,
    name: String,
}

pub async fn run(args: InitArgs) -> Result<()> {
    // Check if machine_id is already configured.
    let config_dir = dirs::config_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    let config_path = config_dir.join("tome/tome.toml");

    if config_path.exists() && !args.force {
        let text = std::fs::read_to_string(&config_path)?;
        if let Ok(parsed) = text.parse::<toml::Table>() {
            if parsed.contains_key("machine_id") {
                let id = parsed["machine_id"].as_integer().unwrap_or(0);
                anyhow::bail!("machine_id = {} already set in {:?}. Use --force to overwrite.", id, config_path);
            }
        }
    }

    // Determine machine name: --name flag, or hostname.
    let machine_name = args.name.unwrap_or_else(|| {
        hostname::get().ok().and_then(|h| h.into_string().ok()).unwrap_or_else(|| "unknown".to_owned())
    });

    // Register with the central server.
    let url = format!("{}/machines", args.server.trim_end_matches('/'));
    info!("registering machine {:?} at {}", machine_name, url);

    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .json(&RegisterRequest { name: machine_name.clone(), description: args.description })
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("registration failed ({}): {}", status, body);
    }

    let machine: MachineResponse = resp.json().await?;
    println!("registered: machine_id={}, name={}", machine.machine_id, machine.name);

    // Write machine_id to ~/.config/tome/tome.toml.
    let tome_dir = config_dir.join("tome");
    std::fs::create_dir_all(&tome_dir)?;

    if config_path.exists() {
        let text = std::fs::read_to_string(&config_path)?;
        let mut table: toml::Table = text.parse().unwrap_or_default();
        table.insert("machine_id".to_owned(), toml::Value::Integer(machine.machine_id as i64));
        std::fs::write(&config_path, toml::to_string_pretty(&table)?)?;
    } else {
        let mut table = toml::Table::new();
        table.insert("machine_id".to_owned(), toml::Value::Integer(machine.machine_id as i64));
        std::fs::write(&config_path, toml::to_string_pretty(&table)?)?;
    }

    println!("wrote machine_id = {} to {:?}", machine.machine_id, config_path);
    Ok(())
}
