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
mod server;
mod client;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let destination_name_flag = Arg::with_name("destination-name")
    .alias("dest-name")
    .long("destination-name")
    .long_help("when generating destinations, or using destinations this is the name of said destination")
    .takes_value(true)
    .required(true);
    let tunnel_name_flag = Arg::with_name("tunnel-name")
    .long("tunnel-name")
    .help("the name of the tunnel to use within a server, etc..")
    .takes_value(true)
    .required(true);
    let destination_flag = Arg::with_name("destination")
    .long("destination")
    .help("the destination (i2p address) to connection to, usually by a client process")
    .takes_value(true)
    .required(true);
    let forward_ip_flag = Arg::with_name("forward-ip")
    .long("forward-ip")
    .help("the ip address to forward connections to")
    .takes_value(true)
    .required(true);
    let listen_ip_flag = Arg::with_name("listen-ip")
    .long("listen-ip")
    .help("the ip address to listening for connections on")
    .takes_value(true)
    .required(true);
    let non_blocking_flag = Arg::with_name("nonblocking")
    .long("nonblocking")
    .help("enable socket nonblocking mode")
    .takes_value(false)
    .required(false);
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
            SubCommand::with_name("server")
            .about("server management commands")
            .subcommands(vec![
                SubCommand::with_name("echo")
                .about("starts the basic echo server")
                .arg(destination_name_flag.clone())
                .arg(tunnel_name_flag.clone()),
                SubCommand::with_name("tcp-echo")
                .about("starts the basic tcp echo server")
                .arg(listen_ip_flag.clone()),
                SubCommand::with_name("reverse-proxy")
                .about("starts the reverse proxy server")
                .arg(destination_name_flag.clone())
                .arg(tunnel_name_flag.clone())
                .arg(forward_ip_flag.clone())
                .arg(non_blocking_flag.clone())
            ]),
            SubCommand::with_name("client")
            .about("client management commands")
            .subcommands(vec![
                SubCommand::with_name("echo")
                .about("starts the basic echo client test")
                .arg(destination_flag.clone())
            ])
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
        ("server", Some(server)) => match server.subcommand() {
            ("echo", Some(echo_server)) => {
                server::start_echo_server(echo_server, &config_file_path).await
            }
            ("tcp-echo", Some(tcp_echo)) => {
                server::start_tcp_echo_server(tcp_echo, &config_file_path).await
            }
            ("reverse-proxy", Some(reverse_proxy)) => {
                server::start_reverse_proxy(reverse_proxy, config_file_path).await
            }
            _ => return Err(anyhow!(invalid_subcommand("server")))
        }
        ("client", Some(client)) => match client.subcommand() {
            ("echo", Some(echo_client)) => {
                client::echo_client_test(echo_client, config_file_path).await
            }
            _ => return Err(anyhow!(invalid_subcommand("client")))
        }
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
