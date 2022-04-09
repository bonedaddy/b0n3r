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

    {
        let mut rand_seed_buf = [0_u8; 64];
        i2p_stream.read(&mut rand_seed_buf).unwrap();
        use ring::digest::{Context, SHA256};
        let mut context = Context::new(&SHA256);
        context.update(&rand_seed_buf);
        let digest = context.finish();
        i2p_stream.write(digest.as_ref()).unwrap();
    }

    let mut i2p_conn = tokio::net::TcpStream::from_std(i2p_stream.inner.sam.conn)?;
    let (mut i2p_reader, mut i2p_writer) = i2p_conn.into_split();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    println!("listening on {}", listener.local_addr().unwrap());
        match listener.accept().await {
            Ok((conn, addr)) => {
                let (mut reader, mut writer) = conn.into_split();
                    tokio::spawn(async move {
                        tokio::io::copy(&mut i2p_reader, &mut writer).await;
                    });
                    let copied = match tokio::io::copy(&mut reader, &mut i2p_writer).await {
                        Ok(n) => n,
                        Err(err) => {
                            println!("failed to copy {:#?}", err);
                            return Ok(());
                        }
                    };
                    println!("copied {} bytes", copied);
            },
            Err(err) => {
                println!("failed to accept connection {:#?}", err);
            }
        }
    Ok(())
}