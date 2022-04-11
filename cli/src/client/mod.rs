use anyhow::{Result, anyhow};
use bytes::BytesMut;
use config::Configuration;
use clap;
use i2p::net::I2pStream;
use std::io::{Write, Read};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
pub async fn echo_client_test(matches: &clap::ArgMatches<'_>, config_file_path: &str) -> Result<()> {
    let cfg = Configuration::load(config_file_path)?;
    let mut i2p_stream = match I2pStream::connect(matches.value_of("destination").unwrap()) {
        Ok(i2p_stream) => i2p_stream,
        Err(err) => return Err(anyhow!("failed to connect to destination {:#?}", err)),
    };

    {
        let mut buf = [0_u8; 1024];
        match i2p_stream.read(&mut buf) {
            Ok(n) => {
                println!("read {} bytes", n);
                let mut buf_int = [0_u8; 8];
                buf_int.copy_from_slice(&buf[0..8]);
                let vdf_seed =  rug::Integer::from(
                    u64::from_le_bytes(buf_int)
                );
                println!("rand_seed {}", vdf_seed);
                const NUM_STEPS: u64 = 1024 * 512;
                let witness = vdf::vdf_mimc::eval(&vdf_seed, NUM_STEPS);
                println!("witness {}", witness);
                let witness_str = witness.to_string();
                match i2p_stream.write(witness_str.as_bytes()) {
                    Ok(n) => println!("wrote {} bytes", n),
                    Err(err) => return Err(anyhow!("failed to write data {:#?}", err)),
                };
            }
            Err(err) => return Err(anyhow!("failed to read data {:#?}", err)),
        }
    }
    i2p_stream.inner.set_nonblocking(true).unwrap();
    let mut i2p_conn = tokio::net::TcpStream::from_std(i2p_stream.inner.sam.conn)?;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    println!("listening on {}", listener.local_addr().unwrap());

        match listener.accept().await {
            Ok((mut conn, addr)) => {
                    // read(outgoing) -> write(incoming)


                        let (mut ri, mut wi) = conn.split();
                        let (mut ro, mut wo) = i2p_conn.split();
                    
                        let client_to_server = async {
                            tokio::io::copy(&mut ri, &mut wo).await.unwrap();
                            wo.shutdown().await
                        };
                    
                        let server_to_client = async {
                            tokio::io::copy(&mut ro, &mut wi).await.unwrap();
                            wi.shutdown().await
                        };
                    
                        tokio::try_join!(client_to_server, server_to_client).unwrap();
            },
            Err(err) => {
                println!("failed to accept connection {:#?}", err);
            }
        }


    Ok(())
}