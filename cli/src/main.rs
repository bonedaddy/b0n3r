use anyhow::{anyhow, Result};
use clap::{App, Arg, ArgMatches, SubCommand};
use i2p::net::{I2pAddr, I2pListener, I2pStream};
use i2p::sam_options::SignatureType;
use log::*;
use std::io::{Read, Write};
use std::str::from_utf8;
use std::sync::Arc;
use std::{thread, time};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
mod utils;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let destination_name_flag = Arg::with_name("destination-name")
    .alias("dest-name")
    .long("destination-name")
    .long_help("when generating destinations, or using destinations this is the name of said destination")
    .takes_value(true)
    .required(true);

    let matches = App::new("boner-cli")
        .about("another innapropriately named but maybe useful piece of software written by me")
        .long_about("ideal application outcome is a self configuring tcp proxy that uses i2p")
        .arg(
            Arg::with_name("config")
                .help("path to the config file")
                .long("config")
                .takes_value(true)
                .value_name("FILE"),
        )
        .subcommands(vec![
            SubCommand::with_name("config")
                .about("config management commands")
                .subcommands(vec![
                    SubCommand::with_name("new").about("generate a new config file")
                ]),
            SubCommand::with_name("utils")
                .about("utility commands")
                .subcommands(vec![SubCommand::with_name("generate-destination")
                    .alias("gen-dest")
                    .about("generate a destination public/private keypair within the local router")
                    .arg(destination_name_flag.clone())]),
        ])
        .get_matches();

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
            _ => return Err(anyhow!(invalid_subcommand("config"))),
        },
        ("utils", Some(utils)) => match utils.subcommand() {
            ("generate-destination", Some(gen_dest)) => {
                utils::gen_destination(gen_dest, config_file_path)
            }
            _ => return Err(anyhow!(invalid_subcommand("utils"))),
        },
        _ => {
            println!("{}", matches.usage());
            println!("    run --help for more information");
            Ok(())
        }
    }
}

fn invalid_subcommand(command: &str) -> String {
    format!(
        "invalid subcommand {}, please run --help for more information",
        command
    )
}
