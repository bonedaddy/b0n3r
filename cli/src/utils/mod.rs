use anyhow::{anyhow, Result};
use config::{Configuration, Destination};
use i2p::sam_options::SignatureType;
use log::info;

pub fn gen_destination(matches: &clap::ArgMatches, config_file_path: &str) -> Result<()> {
    let mut conf = config::Configuration::load(config_file_path)?;
    let destination_name = matches.value_of("destination-name").unwrap();
    if conf.destination_by_name(destination_name).is_ok() {
        return Err(anyhow!(
            "destination leaset set with name {} alrady exists in config file",
            destination_name
        ));
    }
    let mut sam_client = conf.new_sam_client()?;
    let (pubkey, seckey) = sam_client
        .generate_destination(SignatureType::EdDsaSha512Ed25519)
        .unwrap();
    conf.destinations.push(Destination {
        public_key: pubkey.clone(),
        secret_key: seckey.clone(),
        name: destination_name.to_string(),
    });
    conf.save(config_file_path)?;
    info!("public key {}", pubkey);
    info!("secret key {}", seckey);
    Ok(())
}
