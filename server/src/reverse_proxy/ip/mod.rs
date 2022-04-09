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
                        let incoming_conn = incoming.0;
                        let incoming_addr = incoming.1;
                        // configure the incoming connection
                        match configure_incoming_stream(&incoming_conn, non_blocking, read_timeout, write_timeout) {
                            Ok(_) => (),
                            Err(err) => {
                                error!("failed to configure incoming connection for {}: {:#?}", incoming_addr, err);
                                let _ = incoming_conn.shutdown(Shutdown::Both);
                                return;
                            }
                        }

                        let mut incoming_conn = match tokio::net::TcpStream::from_std(incoming_conn.inner.sam.conn) {
                            Ok(incoming_conn) => incoming_conn,
                            Err(err) => {
                                error!("failed to configure incoming connection for {}: {:#?}", incoming_addr, err);
                                return;
                            }
                        };
                        
                        info!("accepted connection from {}", incoming_addr);

                        info!("connecting to {}", forward_ip);
                        let mut outgoing_conn = match tokio::net::TcpStream::connect(&forward_ip).await {
                            Ok(dest_conn) => dest_conn,
                            Err(err) => {
                                error!("failed to conenct to traget ip {:#?}", err);
                                let _ = incoming_conn.shutdown().await;
                                return;
                            }
                        };

                        {
                            let mut buf = [0_u8; 1];
                            match incoming_conn.read_exact(&mut buf).await {
                                Ok(_) => (),
                                Err(err) => {
                                    error!("failed to read initial first bytes {:#?}", err);
                                    let _ = incoming_conn.shutdown().await;
                                    return;
                                }
                            }
                        }
                        loop {
                            let mut read_buf =[0_u8; 1024];
                            match incoming_conn.read(&mut read_buf).await {
                                Ok(n) => if n > 0_usize {
                                    info!("read {} bytes from {}", n, incoming_addr);
                                    match outgoing_conn.write(&read_buf[..n]).await {
                                        Ok(n) => if n > 0_usize {
                                            info!("wrote {} bytes to outgoing connection", n);
                                        } else {
                                            error!("wrote 0 bytes to outgoing connection");
                                            let _ = incoming_conn.shutdown().await;
                                            return;
                                        }
                                        Err(err) => {
                                            error!("failed to write data to outgoing connection {:#?}", err);
                                            let _ = incoming_conn.shutdown().await;
                                            return;
                                        }
                                    }
                                    let mut write_buf = [0_u8; 1024];
                                    match outgoing_conn.read(&mut write_buf).await {
                                        Ok(n) => if n > 0_usize {
                                            info!("read {} bytes from outgoing connection", n);
                                            match incoming_conn.write(&write_buf[..n]).await {
                                                Ok(n) => if n > 0_usize {
                                                    info!("wrote {} bytes to incoming connection", n);
                                                } else {
                                                    error!("wrote 0 bytes to incoming connection");
                                                    let _ = incoming_conn.shutdown().await;
                                                    return;
                                                }
                                                Err(err) => {
                                                    error!("failed to write data to incoming connection {:#?}", err);
                                                    let _ = incoming_conn.shutdown().await;
                                                    return;
                                                }
                                            }
                                        } else if n == 0_usize {
                                            warn!("received eof");
                                            let _ = incoming_conn.shutdown().await;
                                            return;
                                        }
                                        Err(err) => {
                                            error!("failed to read data from outgoing conection {:#?}", err);
                                            let _ = incoming_conn.shutdown().await;
                                            return;
                                        }
                                    }
                                } else if n == 0_usize {
                                    warn!("received eof");
                                    let _ = incoming_conn.shutdown().await;
                                    return;
                                }
                                Err(err) => {
                                    // if the incoming connection is set to non blocking and this errorkind
                                    // is returned, it means the io operation needs to be retried, so sleep
                                    // for a few milliseconds to give the cpu a break, and introduce a yield point
                                    // where tokio can schedule other tasks
                                    if err.kind().eq(&std::io::ErrorKind::WouldBlock) && non_blocking {
                                        //  sleep for 50ms before looping again
                                        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                                        continue;
                                    }
                                    error!("failed to read from connection {:#?}", err);
                                    let _ = incoming_conn.shutdown().await;
                                    return;
                                }
                            }
                        }


                        // return the random value they need to hash
                        // and use as the seed in a vdf
                        //let rand_seed = rand_value();
                        //match incoming_conn.write(rand_seed.as_bytes()) {
                        //    Ok(n) => if n != 64 {
                        //        error!("failed to fully write random_seed. wrote {} bytes", n);
                        //        let _ = incoming_conn.shutdown(Shutdown::Both);
                        //        return;
                        //    }
                        //    Err(err) => {
                        //        error!("failed to write random_seed {:#?}", err);
                        //        let _ = incoming_conn.shutdown(Shutdown::Both);
                        //        return;
                        //    }
                        //};
                        //let incoming_tcp_stream = match incoming_conn.inner.try_clone_session() {
                        //    Ok(tcp_stream) => tcp_stream,
                        //    Err(err) => {
                        //        error!("failed to parse session into tcp stream {:#?}", err);
                        //        let _ = incoming_conn.shutdown(Shutdown::Both);
                        //        return;
                        //    }
                        //};
                        //let incoming_tcp_stream = match tokio::net::TcpStream::from_std(incoming_tcp_stream) {
                        //    Ok(tcp_stream) => tcp_stream,
                        //    Err(err) => {
                        //        error!("failed to parse session into tcp stream {:#?}", err);
                        //        let _ = incoming_conn.shutdown(Shutdown::Both);
                        //        return;  
                        //    }
                        //};
//
//
                        //let (mut incoming_reader, mut incoming_writer) = tokio::io::split(incoming_tcp_stream);
                        //info!("connecting to {}", forward_ip);
                        //let outgoing_conn = match tokio::net::TcpStream::connect(&forward_ip).await {
                        //    Ok(dest_conn) => dest_conn,
                        //    Err(err) => {
                        //        error!("failed to conenct to traget ip {:#?}", err);
                        //        let _ = incoming_conn.shutdown(Shutdown::Both);
                        //        return;
                        //    }
                        //};
                        //info!("connected to {}", forward_ip);
                        //let (mut outgoing_reader, mut outgoing_writer) = tokio::io::split(outgoing_conn);
//
                        //tokio::task::spawn(async move {
                        //    if let Err(err) = tokio::io::copy(&mut incoming_reader, &mut outgoing_writer).await {
                        //        error!("incoming_conn(rdr) -> outgoing_conn(wrtr) encountered error {:#?}", err);
                        //    }
                        //});
                        //tokio::task::spawn(async move {
                        //    if let Err(err) = tokio::io::copy(&mut outgoing_reader, &mut incoming_writer).await {
                        //        error!("outgoing_conn(rdr) -> incoming_conn(wrtr) encountered error {:#?}", err);
                        //    }
                        //});
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