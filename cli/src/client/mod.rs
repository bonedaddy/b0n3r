use anyhow::{Result, anyhow};
use config::Configuration;
use clap;
use i2p::net::I2pStream;
use std::io::{Write, Read};

pub async fn echo_client_test(matches: &clap::ArgMatches<'_>, config_file_path: &str) -> Result<()> {
    let cfg = Configuration::load(config_file_path)?;
    let mut i2p_stream = match I2pStream::connect(matches.value_of("destination").unwrap()) {
        Ok(i2p_stream) => i2p_stream,
        Err(err) => return Err(anyhow!("failed to connect to destination {:#?}", err)),
    };
    let mut i2p_conn = tokio::net::TcpStream::from_std(i2p_stream.inner.sam.conn)?;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    println!("listening on {}", listener.local_addr().unwrap());
        match listener.accept().await {
            Ok((mut conn, addr)) => {
                match tokio::io::copy_bidirectional(&mut i2p_conn,&mut conn).await {
                    Ok((na, nb)) => {

                    }
                    Err(err) => {
                        println!("failed to copy {:#?}", err);
                        return Ok(()); 
                    }
                }
            },
            Err(err) => {
                println!("failed to accept connection {:#?}", err);
            }
        }
    Ok(())
}