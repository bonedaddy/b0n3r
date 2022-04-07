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
use std::{convert::TryFrom, net::SocketAddr};

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
   async fn start(
        self: &Arc<Self>, 
        tunnel_name: String,
        destination_name: String,
        // ip address to forward requests too
        forward_ip_address: String,
        listen_address: String,
        read_timeout: Option<Duration>,
        write_timeout: Option<Duration>,
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

        loop {
            match i2p_listener.accept() {
                Ok(incoming) => {
                    let incoming = incoming;
                    let forward_ip = forward_ip_address.clone();
                    tokio::task::spawn(async move {
                        let mut incoming_conn = incoming.0;
                        let incoming_addr = incoming.1;
                        // configure the incoming connection
                        match configure_incoming_stream(&incoming_conn, read_timeout, write_timeout) {
                            Ok(_) => (),
                            Err(err) => {
                                error!("failed to configure incoming connection for {}: {:#?}", incoming_addr, err);
                                let _ = incoming_conn.shutdown(Shutdown::Both);
                                return;
                            }
                        }
                        info!("accepted connection from {}", incoming_addr);
                        // return the random value they need to hash
                        // and use as the seed in a vdf
                        let rand_seed = rand_number();
                        match incoming_conn.write(&rand_seed.to_le_bytes()[..]) {
                            Ok(n) => if n != 8 {
                                error!("failed to fully write random_seed. wrote {} bytes", n);
                                let _ = incoming_conn.shutdown(Shutdown::Both);
                                return;
                            }
                            Err(err) => {
                                error!("failed to write random_seed {:#?}", err);
                                let _ = incoming_conn.shutdown(Shutdown::Both);
                                return;
                            }
                        };
                        {
                            let mut authentication_buffer = [0_u8; 1024];
                            match incoming_conn.read(&mut authentication_buffer) {
                                Ok(_) => (),
                                Err(err) => {
                                    error!("failed to read authentication buffer. {:#?}", err);
                                    let _ = incoming_conn.shutdown(Shutdown::Both);
                                    return;
                                }
                            }
                            // scan through until we find the newline
                            let mut end_idx = 0;
                            authentication_buffer.iter().enumerate().for_each(|(idx, val)| {
                                if *val == '\n' as u8 {
                                    end_idx = idx;
                                }
                            });
                            if end_idx == 0 {
                                error!("failed to find end index");
                                let _ = incoming_conn.shutdown(Shutdown::Both);
                                return;
                            }
                            let mut auth_buff = Vec::with_capacity(end_idx);
                            let mut handle = authentication_buffer.take(end_idx as u64);
                            match handle.read(&mut auth_buff) {
                                Ok(_) => (),
                                Err(err) => {
                                    error!("failed to fill auth_buff {:#?}", err);
                                    let _ = incoming_conn.shutdown(Shutdown::Both);
                                    return;
                                }
                            }; 
                            let vdf_result = rug::Integer::new().set_bit(index, val)
                            warn!("todo(bonedaddy): validate the returned authentication buffer");

                        }
                        let dest_conn = match tokio::net::TcpStream::connect(&forward_ip).await {
                            Ok(dest_conn) => dest_conn,
                            Err(err) => {
                                error!("failed to conenct to traget ip {:#?}", err);
                                let _ = incoming_conn.shutdown(Shutdown::Both);
                                return;
                            }
                        };
                    });
                }
                Err(err) => {
                    error!("failed to accept incoming connection {:#?}", err);
                }
            }
        }
    }

    
}

fn rand_divisor() -> f64 {
    rand::thread_rng().gen_range(1_f64..4_f64)
}

fn rand_number() -> f64 {
    rand::thread_rng().gen_range(
        f64::MAX / (rand_divisor() - 1_f64)
        ..
        f64::MAX,
    )
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
    read_timeout: Option<Duration>,
    write_timeout: Option<Duration>,
) -> Result<()> {
    match incoming_conn.set_nonblocking(true) {
        Ok(_) => (),
        Err(err) => {
            return Err(anyhow!("failed to set incoming connection to non blocking mode {:#?}", err));
        }
    }
    match incoming_conn.set_read_timeout(read_timeout) {
        Ok(_) => (),
        Err(err) => {
            return Err(anyhow!("failed to set incoming connection read timeout {:#?}", err));
        }
    }
    match incoming_conn.set_write_timeout(write_timeout) {
        Ok(_) => (),
        Err(err) => {
            return Err(anyhow!("failed to set incoming connection write timeout {:#?}", err));
        }
    }
    Ok(())
}