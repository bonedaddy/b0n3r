//! provides a reverse proxy server that forwards connections
//! received on an i2p stream to an ip-based service.
use bytes::{BufMut, BytesMut};
use rug::Integer;

use anyhow::{anyhow, Result};
use axum::{
    extract::Extension,
    http::{uri::Uri, Request, Response},
    routing::get,
    Router,
};
use config::Configuration;
use hyper::{client::HttpConnector, server::conn::AddrIncoming, Body};
use i2p::net::I2pStream;
use i2p::{
    net::{I2pAddr, I2pListener},
    sam::SessionStyle,
    SamConnection, Session,
};
use log::{error, info, warn};
use rand::Rng;
use std::io::{Read, Write};
use std::time::Duration;
use std::{
    borrow::BorrowMut,
    net::{Shutdown, TcpListener},
    str::FromStr,
    sync::Arc,
};
use std::{convert::TryFrom, net::SocketAddr};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use futures::FutureExt;
type Client = hyper::client::Client<HttpConnector, Body>;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

pub struct Server {
    cfg: Configuration,
}

impl Server {
    pub fn new(cfg: Configuration) -> Result<Arc<Server>> {
        Ok(Arc::new(Server { cfg }))
    }
    /// tunnel_name specifies the name of the server tunnel and
    /// it's corresponding lease set to use
    pub fn start(
        self: &Arc<Self>,
        tunnel_name: String,
        destination_name: String,
        // ip address to forward requests too
        forward_ip_address: String,
        read_timeout: Option<Duration>,
        write_timeout: Option<Duration>,
        non_blocking: bool,
    ) -> Result<()> {
        let tunnel = self.cfg.server.tunnel_by_name(&tunnel_name)?;
        let destination = self.cfg.destination_by_name(&destination_name)?;
        let sam_session = match Session::create(
            self.cfg.sam.endpoint.clone(),
            &destination.secret_key.clone(),
            &nickname(),
            SessionStyle::Stream,
            tunnel.options(),
        ) {
            Ok(sam_session) => sam_session,
            Err(err) => return Err(anyhow!("failed to create sam session {:#?}", err)),
        };
        let mut i2p_listener = match I2pListener::bind_with_session(&sam_session) {
            Ok(i2p_listener) => i2p_listener,
            Err(err) => {
                return Err(anyhow!(
                    "failed to bind i2p listener to sam session {:#?}",
                    err
                ))
            }
        };

        let local_addr = i2p_listener.local_addr().unwrap();

        info!(
            "listening for connections on {}",
            I2pAddr::from_b64(&local_addr.dest().string()).unwrap()
        );
        loop {
            match i2p_listener.accept() {
                Ok(incoming) => {
                    let incoming = incoming;
                    let forward_ip = forward_ip_address.clone();
                    tokio::task::spawn(async move {
                        let mut incoming_conn = incoming.0;
                        let incoming_addr = incoming.1;

                        info!("accepted connection from {}", incoming_addr);

                        info!("connecting to {}", forward_ip);

                        // drain the first byte
                        // for some reason 1 byte is always sent, not sure why
                        {
                            let mut buf = [0_u8; 1];
                            match incoming_conn.read_exact(&mut buf) {
                                Ok(_) => (),
                                Err(err) => {
                                    error!("failed to read initial first bytes {:#?}", err);
                                    let _ = incoming_conn.shutdown(Shutdown::Both);
                                    return;
                                }
                            }
                        }
                        // to facilate decentralized ddos mitigation, we use a vdf based captcha format
                        // which takes approximately 13s to generate, and 0.2s to verify
                        {
                            let rand_seed = rug::Integer::from(rand_int());

                            let captcha = VDFCaptcha {
                                rand_seed: rand_seed.to_string(),
                                steps: 1024 * 1024,
                            };

                            println!(
                                "rand_seed {}, rand_seed_str {}",
                                rand_seed, captcha.rand_seed
                            );

                            // write the random seed
                            match incoming_conn.write(&match captcha.serialize() {
                                Ok(captcha_bytes) => captcha_bytes,
                                Err(err) => {
                                    error!("failed to serialize vdf captcha {:#?}", err);
                                    let _ = incoming_conn.shutdown(Shutdown::Both);
                                    return;
                                }
                            }[..]) {
                                Ok(n) => info!("wrote {} bytes of random_seed", n),
                                Err(err) => {
                                    error!("failed to write random_seed {:#?}", err);
                                    let _ = incoming_conn.shutdown(Shutdown::Both);
                                    return;
                                }
                            };
                            let mut buf = [0_u8; 128];
                            match incoming_conn.read(&mut buf) {
                                Ok(n) => {
                                    info!("read {} bytes of hashed_random_seed", n);
                                    match captcha.verify(&buf[0..n])  {
                                        Ok(_) => info!("vdf verified"),
                                        Err(err) => {
                                            error!("failed to verify vdf {:#?}", err);
                                            let _ = incoming_conn.shutdown(Shutdown::Both);
                                            return;
                                        }
                                    }
                                }
                                Err(err) => {
                                    error!("failed to write random_seed {:#?}", err);
                                    let _ = incoming_conn.shutdown(Shutdown::Both);
                                    return;
                                }
                            }
                        }
                        match configure_incoming_stream(
                            &incoming_conn,
                            non_blocking,
                            read_timeout,
                            write_timeout,
                        ) {
                            Ok(_) => (),
                            Err(err) => {
                                error!("failed to configure incoming connection {:#?}", err);
                                let _ = incoming_conn.shutdown(Shutdown::Both);
                                return;
                            }
                        }
                        incoming_conn.set_nonblocking(true).unwrap();
                        let incoming_conn =
                            match tokio::net::TcpStream::from_std(incoming_conn.inner.sam.conn) {
                                Ok(incoming_conn) => incoming_conn,
                                Err(err) => {
                                    error!(
                                        "failed to configure incoming connection for {}: {:#?}",
                                        incoming_addr, err
                                    );
                                    return;
                                }
                            };
                        let transfer = transfer(incoming_conn, forward_ip.clone()).map(|r| {
                            if let Err(e) = r {
                                println!("Failed to transfer; error={}", e);
                            }
                        });
                        tokio::spawn(async move { 
                            info!("starting background workloop");
                            transfer.await;
                            info!("background workloop ended");
                        });
                    });
                }
                Err(err) => {
                    error!("failed to accept incoming connection {:#?}", err);
                    i2p_listener = match I2pListener::bind_with_session(&sam_session) {
                        Ok(i2p_listener) => i2p_listener,
                        Err(err) => {
                            return Err(anyhow!(
                                "failed to bind i2p listener to sam session {:#?}",
                                err
                            ))
                        }
                    }
                }
            }
        }
    }
}

fn rand_int() -> u64 {
    rand::thread_rng().gen_range(u64::MIN..u64::MAX)
}

fn rand_value() -> String {
    use rand::distributions::Alphanumeric;
    use rand::distributions::DistString;
    Alphanumeric.sample_string(&mut rand::thread_rng(), 64)
}

fn nickname() -> String {
    use rand::distributions::Alphanumeric;
    use rand::distributions::DistString;
    Alphanumeric.sample_string(&mut rand::thread_rng(), 16)
}

fn configure_incoming_stream(
    incoming_conn: &I2pStream,
    nonblocking: bool,
    read_timeout: Option<Duration>,
    write_timeout: Option<Duration>,
) -> Result<()> {
    match incoming_conn.set_nonblocking(nonblocking) {
        Ok(_) => (),
        Err(err) => {
            return Err(anyhow!(
                "failed to set incoming connection to non blocking mode {:#?}",
                err
            ));
        }
    }
    if read_timeout.is_some() {
        match incoming_conn.set_read_timeout(read_timeout) {
            Ok(_) => (),
            Err(err) => {
                return Err(anyhow!(
                    "failed to set incoming connection read timeout {:#?}",
                    err
                ));
            }
        }
    }
    if write_timeout.is_some() {
        match incoming_conn.set_write_timeout(write_timeout) {
            Ok(_) => (),
            Err(err) => {
                return Err(anyhow!(
                    "failed to set incoming connection write timeout {:#?}",
                    err
                ));
            }
        }
    }

    Ok(())
}

async fn transfer(
    mut inbound: TcpStream,
    proxy_addr: String,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut outbound = TcpStream::connect(proxy_addr).await?;

    let (mut ri, mut wi) = inbound.split();
    let (mut ro, mut wo) = outbound.split();

    let client_to_server = async {
        tokio::io::copy(&mut ri, &mut wo).await?;
        wo.shutdown().await
    };

    let server_to_client = async {
        tokio::io::copy(&mut ro, &mut wi).await?;
        wi.shutdown().await
    };

    tokio::try_join!(client_to_server, server_to_client)?;

    Ok(())
}

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct VDFCaptcha {
    pub rand_seed: String,
    pub steps: u64,
}

impl VDFCaptcha {
    pub fn deserialize(data: &[u8]) -> Result<VDFCaptcha> {
        Ok(bincode::deserialize(data)?)
    }
    pub fn serialize(&self) -> Result<Vec<u8>> {
        Ok(bincode::serialize(self)?)
    }
    pub fn rand_seed(&self) -> Result<Integer> {
        Ok(Integer::from_str(&self.rand_seed)?)
    }
    pub fn eval(&self) -> Result<Integer> {
        let start = std::time::SystemTime::now();
        let witness = vdf::vdf_mimc::eval(&self.rand_seed()?, self.steps);
        let duration = start.elapsed()?;
        info!("vdf evaluated in {} seconds", duration.as_secs_f64());
        println!("vdf evaluated in {} seconds", duration.as_secs_f64());
        Ok(witness)
    }
    pub fn verify(&self, witness: &[u8]) -> Result<()> {
        let start = std::time::SystemTime::now();
        if !vdf::vdf_mimc::verify(
            &self.rand_seed()?,
            self.steps,
            &Integer::from_str(&String::from_utf8(witness.to_vec())?)?,
        ) {
            return Err(anyhow!("failed to verify mimc"));
        }
        let duration = start.elapsed()?;
        info!("vdf verified in {} seconds", duration.as_secs_f64());
        Ok(())
    }
}
