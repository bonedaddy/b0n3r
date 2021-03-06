//! echo is a basic echo server
use anyhow::{anyhow, Result};
use config::Configuration;
use i2p::{
    net::{I2pAddr, I2pListener},
    sam::SessionStyle,
    SamConnection, Session,
};
use log::{error, info, warn};
use std::sync::Arc;

pub struct Server {
    cfg: Configuration,
}

impl Server {
    pub fn new(cfg: Configuration) -> Result<Arc<Server>> {
        Ok(Arc::new(Server { cfg }))
    }
    /// tunnel_name specifies the name of the server tunnel and
    /// it's corresponding lease set to use
    pub async fn start(self: &Arc<Self>, tunnel_name: &str, destination_name: &str) -> Result<()> {
        let tunnel = self.cfg.server.tunnel_by_name(tunnel_name)?;
        let destination = self.cfg.destination_by_name(destination_name)?;
        info!("tunnel {:#?}", tunnel);
        println!("options {:#?}", tunnel.options().options());
        // create a sam session
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
        // bind an i2p listener to said session
        let i2p_listener = match I2pListener::bind_with_session(&sam_session) {
            Ok(i2p_listener) => i2p_listener,
            Err(err) => {
                return Err(anyhow!(
                    "failed to bind i2p listener to sam session {:#?}",
                    err
                ))
            }
        };
        let our_dest_addr =
            I2pAddr::from_b64(&format!("{}", i2p_listener.local_addr().unwrap().dest())).unwrap();
        info!(
            "echo server online, waiting for connections on {} ...",
            our_dest_addr
        );
        loop {
            match i2p_listener.accept() {
                Ok(conn) => {
                    info!("accepted connection {:#?}", conn);
                }
                Err(err) => {
                    error!("failed to accept incoming connection {:#?}", err);
                }
            }
        }
    }
}

fn nickname() -> String {
    use rand::distributions::Alphanumeric;
    use rand::distributions::DistString;
    Alphanumeric.sample_string(&mut rand::thread_rng(), 16)
}
