use anyhow::{Result, anyhow};
use config::Configuration;
use server::echo::Server;
pub async fn start_echo_server(matches: &clap::ArgMatches<'_>, config_file_path: &str) -> Result<()> {
    let config = config::Configuration::load(config_file_path)?;
    let server = Server::new(config)?;
    let tunnel_name = matches.value_of("tunnel-name").unwrap();
    let destination_name = matches.value_of("destination-name").unwrap();
    server.start(tunnel_name, destination_name).await?;
    Ok(())
}