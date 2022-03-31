use i2p::sam_options::SignatureType;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use clap::{ArgMatches, App, Arg, SubCommand};
use anyhow::{anyhow, Result};
use i2p::net::{I2pListener, I2pStream, I2pAddr};
use std::io::{Write, Read};
use std::sync::Arc;
use log::*;
use std::{thread, time};
use std::str::from_utf8;
mod utils;

#[tokio::main]
async fn main() -> Result<()> {
	env_logger::init();

    let destination_name_flag = Arg::with_name("destination-name")
    .long("destination-name")
    .long_help("when generating destinations, or using destinations this is the name of said destination")
    .takes_value(true)
    .required(true);

	let matches = App::new("proxy")
    .arg(
        Arg::with_name("config")
        .help("path to the config file")
        .long("config")
        .takes_value(true)
        .value_name("FILE")
    )
	.subcommands(vec![
        SubCommand::with_name("config")
        .about("config management commands")
        .subcommands(vec![
            SubCommand::with_name("new")
            .about("generate a new config file")
        ]),
        SubCommand::with_name("utils")
        .about("utility commands")
        .subcommands(vec![
            SubCommand::with_name("gen-destination")
            .about("generate a destination public/private keypair")
            .arg(destination_name_flag.clone())
        ]),
	]).get_matches();

    let config_file_path = matches.value_of("config").unwrap_or("config.yaml");

	process_matches(&matches, config_file_path).await?;
	Ok(())
}

async fn process_matches(matches: &ArgMatches<'_>, config_file_path: &str) -> Result<()> {
	match matches.subcommand() {
        ("config", Some(cfg)) => match cfg.subcommand() {
            ("new", Some(_)) => {
                let conf = config::Configuration::new();
                conf.save(config_file_path)
            }
            _ => return Err(anyhow!("invalid subcommand")),
        }
        ("utils", Some(utils)) => match utils.subcommand() {
            ("gen-destination", Some(gen_dest)) => {
                utils::gen_destination(gen_dest, config_file_path)
            },
            _ => return Err(anyhow!("invalid subcommand")),
        }
		_ => return Err(anyhow!("invalid subcommand")),
	}
}