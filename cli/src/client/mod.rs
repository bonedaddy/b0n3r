use anyhow::{Result, anyhow};
use config::Configuration;
use clap;
use i2p::net::I2pStream;

pub fn echo_client_test(matches: &clap::ArgMatches, config_file_path: &str) -> Result<()> {
    let cfg = Configuration::load(config_file_path)?;
    let i2p_stream = match I2pStream::connect(matches.value_of("destination").unwrap()) {
        Ok(i2p_stream) => i2p_stream,
        Err(err) => return Err(anyhow!("failed to connect to destination {:#?}", err)),
    };
    Ok(())
}