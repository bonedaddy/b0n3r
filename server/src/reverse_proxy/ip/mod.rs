//! provides a reverse proxy server that forwards connections
//! received on an i2p stream to an ip-based service.

use std::io::{Write, Read};
use std::time::Duration;
use std::{sync::Arc, str::FromStr, borrow::BorrowMut, net::{TcpListener, Shutdown}};
use config::Configuration;
use anyhow::{Result, anyhow};
use i2p::net::I2pStream;
use i2p::{SamConnection, Session, sam::SessionStyle, net::{I2pListener, I2pAddr}};
use log::{info, warn ,error};
use axum::{
    extract::Extension,
    http::{uri::Uri, Request, Response},
    routing::get,
    Router,
};
use hyper::{client::HttpConnector, Body, server::conn::AddrIncoming};
use rand::Rng;
use tokio::io::{AsyncWriteExt, AsyncReadExt};
use tokio::net::TcpStream;
use std::{convert::TryFrom, net::SocketAddr};

use futures::FutureExt;
type Client = hyper::client::Client<HttpConnector, Body>;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};



pub struct Server {
    cfg: Configuration,
}


impl Server {
    pub fn new(
        cfg: Configuration,
    ) -> Result<Arc<Server>> {
        Ok(Arc::new(Server{cfg}))
    }
    /// tunnel_name specifies the name of the server tunnel and
    /// it's corresponding lease set to use
   pub async fn start(
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
        let i2p_listener = match I2pListener::bind_with_session(&sam_session) {
            Ok(i2p_listener) => i2p_listener,
            Err(err) => return Err(anyhow!("failed to bind i2p listener to sam session {:#?}", err))
        };

        let local_addr = i2p_listener.local_addr().unwrap();
        
        info!("listening for connections on {}",  I2pAddr::from_b64(&local_addr.dest().string()).unwrap());
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
                        // until vdfs are implemented, generate a random value
                        // that must be hashed by the incoming connection
                        // this isn't fail-safe, and wont prevent ddos
                        // but it serves as a temporary method of stopping  people 
                        // from randomly opening tunnels
                        {
                            let rand_seed = rand_value();
                            let want_rand_seed_hash = {
                                use ring::digest::{Context, SHA256};
                                let mut context = Context::new(&SHA256);
                                context.update(rand_seed.as_bytes());
                                let digest = context.finish();
                                digest.as_ref().to_vec()
                            };

                            // write the random seed
                            match incoming_conn.write(rand_seed.as_bytes()) {
                                Ok(n) => info!("wrote {} bytes of random_seed", n),
                                Err(err) => {
                                    error!("failed to write random_seed {:#?}", err);
                                    let _ = incoming_conn.shutdown(Shutdown::Both);
                                    return;
                                }
                            };
                            // read the hashed random seed
                            let mut rand_seed_hash = [0_u8; 32];
                            match incoming_conn.read(&mut rand_seed_hash) {
                                Ok(n) => info!("read {} bytes of hashed_random_seed", n),
                                Err(err) => {
                                    error!("failed to write random_seed {:#?}", err);
                                    let _ = incoming_conn.shutdown(Shutdown::Both);
                                    return;
                                }
                            };

                            // ensure it matches the expected value
                            if rand_seed_hash.to_vec().ne(&want_rand_seed_hash) {
                                error!("mismatched hashes. want {:?}, got {:?}", want_rand_seed_hash, rand_seed_hash);
                                let _ = incoming_conn.shutdown(Shutdown::Both);
                                return;
                            }
                        }
                        incoming_conn.set_nonblocking(true).unwrap();
                        let incoming_conn = match tokio::net::TcpStream::from_std(incoming_conn.inner.sam.conn) {
                            Ok(incoming_conn) => incoming_conn,
                            Err(err) => {
                                error!("failed to configure incoming connection for {}: {:#?}", incoming_addr, err);
                                return;
                            }
                        };

                        let transfer = transfer(incoming_conn, forward_ip.clone()).map(|r| {
                            if let Err(e) = r {
                                println!("Failed to transfer; error={}", e);
                            }
                        });
                        tokio::spawn(async move {
                            transfer.await
                        });
                    });
                }
                Err(err) => {
                    error!("failed to accept incoming connection {:#?}", err);
                }
            }
        }
    }

    
}

fn rand_value() -> String {
    use rand::distributions::DistString;
    use rand::distributions::Alphanumeric;
    Alphanumeric.sample_string(&mut rand::thread_rng(), 64)
}

fn nickname() -> String {
    use rand::distributions::DistString;
    use rand::distributions::Alphanumeric;
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
            return Err(anyhow!("failed to set incoming connection to non blocking mode {:#?}", err));
        }
    }
    if read_timeout.is_some() {
        match incoming_conn.set_read_timeout(read_timeout) {
            Ok(_) => (),
            Err(err) => {
                return Err(anyhow!("failed to set incoming connection read timeout {:#?}", err));
            }
        }
    }
    if write_timeout.is_some() {
        match incoming_conn.set_write_timeout(write_timeout) {
            Ok(_) => (),
            Err(err) => {
                return Err(anyhow!("failed to set incoming connection write timeout {:#?}", err));
            }
        }
    }

    Ok(())
}

async fn transfer(mut inbound: TcpStream, proxy_addr: String) -> Result<(), Box<dyn std::error::Error>> {
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
