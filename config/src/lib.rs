pub mod tunnels;
use std::fs::File;
use anyhow::{anyhow, Result};
use i2p::sam::SamConnection;
use serde::{Deserialize, Serialize};
use tunnels::Tunnel;
#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct Configuration {
    pub destinations: Vec<Destination>,
    pub proxy: Proxy,
    pub sam: SAM,
    pub server: Server,
}

/// configuration for a Proxy, which receives
/// connections over tcp, forwarding them to an i2p eepsite
#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct Proxy {
    pub listen_address: String,
    pub forward_address: String,
}

/// configuration for a Server, which registers
/// an i2p eepsite, and receives connections over i2p
/// forwarding them to a tcp service
#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct Server {
    pub listen_address: String,
    pub forward_address: String,
    pub private_key: String,
    pub public_key: String,
    /// various tunnels that the server service can use
    pub tunnels: Vec<Tunnel>,
}

/// keeps track of a destination leaseset which have been generated
#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct Destination {
    pub public_key: String,
    pub secret_key: String,
    /// is not set to the i2p router, exists only in the config file
    pub name: String,
}

/// configuration for the SAM bridge
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SAM {
    pub endpoint: String,
}


impl Configuration {
    pub fn new() -> Self {
        Default::default()
    }
    pub fn save(&self, path: &str) -> Result<()> {
        let config_data = serde_yaml::to_string(self)?;
        std::fs::write(path, config_data)?;
        Ok(())
    }
    pub fn new_sam_client(&self) -> Result<SamConnection> {
        match SamConnection::connect(self.sam.endpoint.clone()) {
            Ok(sam_conn) => Ok(sam_conn),
            Err(err) => return Err(anyhow!("failed to connect to sam bridge {:#?}", err)),
        }
    }
    pub fn load(path: &str) -> Result<Self> {
        let data = std::fs::read(path)?;
        Ok(serde_yaml::from_slice(&data[..])?)
    }
    pub fn destination_by_name(&self, name: &str) -> Result<Destination> {
        for destination in self.destinations.iter() {
            if destination.name.eq(name) {
                return Ok(destination.clone());
            }
        }
        Err(anyhow!("failed to find destination with name {}", name))
    }
}

impl Server {
    pub fn tunnel_by_name(&self, name: &str) -> Result<Tunnel> {
        for tunnel in self.tunnels.iter() {
            if tunnel.name.eq(name) { return Ok(tunnel.clone()) }
        }
        Err(anyhow!("failed to find tunnel with name {}", name))
    }
}

impl Default for SAM {
    fn default() -> Self {
        Self {
            endpoint: "127.0.0.1:7656".to_string(),
        }
    }
}
