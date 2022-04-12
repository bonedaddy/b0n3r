use anyhow::{anyhow, Result};
use config::Configuration;
use server::echo::Server;
pub async fn start_echo_server(
    matches: &clap::ArgMatches<'_>,
    config_file_path: &str,
) -> Result<()> {
    let config = config::Configuration::load(config_file_path)?;
    let server = Server::new(config)?;
    let tunnel_name = matches.value_of("tunnel-name").unwrap();
    let destination_name = matches.value_of("destination-name").unwrap();
    server.start(tunnel_name, destination_name).await?;
    Ok(())
}

pub async fn start_tcp_echo_server(
    matches: &clap::ArgMatches<'_>,
    config_file_path: &str,
) -> Result<()> {
    let listener = tokio::net::TcpListener::bind(matches.value_of("listen-ip").unwrap()).await?;
    println!("listening on {}", listener.local_addr().unwrap());
    loop {
        match listener.accept().await {
            Ok((conn, addr)) => {
                tokio::task::spawn(async move {
                    println!("accepted connection from {}", addr);
                    let (mut reader, mut writer) = conn.into_split();
                    let copied = match tokio::io::copy(&mut reader, &mut writer).await {
                        Ok(n) => n,
                        Err(err) => {
                            println!("failed to copy {:#?}", err);
                            return;
                        }
                    };
                    println!("copied {} bytes", copied);
                });
            }
            Err(err) => {
                println!("failed to accept connection {:#?}", err);
            }
        }
    }
}

pub async fn start_reverse_proxy(
    matches: &clap::ArgMatches<'_>,
    config_file_path: &str,
) -> Result<()> {
    let config = config::Configuration::load(config_file_path)?;
    let server = server::reverse_proxy::ip::Server::new(config)?;
    let tunnel_name = matches.value_of("tunnel-name").unwrap().to_string();
    let destination_name = matches.value_of("destination-name").unwrap().to_string();
    let forward_ip = matches.value_of("forward-ip").unwrap().to_string();
    let non_blocking = matches.is_present("nonblocking");
    tokio::task::spawn_blocking(move || {
        match server.start(
            tunnel_name,
            destination_name,
            forward_ip,
            None,
            None,
            non_blocking,
        ) {
            Ok(_) => (),
            Err(err) => {
                println!("reverse proxy encountered error {:#?}", err)
            }
        }
    })
    .await?;
    Ok(())
}
