use i2p::sam_options::{SAMOptions, I2CPOptions, I2CPRouterOptions, I2CPTunnelInboundOptions, I2CPTunnelOutboundOptions};
use serde::{Serialize, Deserialize};

#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct Tunnel {
    pub in_length: u8,
    pub in_quantity: u8,
    pub in_backup_quantity: u8,
    pub out_length: u8,
    pub out_quantity: u8,
    pub out_backup_quantity: u8,
    pub name: String,
    pub random_key: Option<String>,
}

impl Tunnel {
    pub fn options(&self) -> SAMOptions {
        let mut sam_options = SAMOptions::default();
        sam_options.i2cp_options = Some(I2CPOptions {
            router_options: Some(I2CPRouterOptions {
                inbound: Some(I2CPTunnelInboundOptions {
                    length: Some(self.in_length),
                    quantity: Some(self.in_quantity),
                    backup_quantity: Some(self.in_backup_quantity),
                    random_key: self.random_key.clone(),
                    ..Default::default()
                }),
                outbound: Some(I2CPTunnelOutboundOptions {
                    length: Some(self.in_length),
                    quantity: Some(self.in_quantity),
                    backup_quantity: Some(self.in_backup_quantity),
                    random_key: self.random_key.clone(),
                    ..Default::default()
                }),
                fast_receive: Some(true),
                should_bundle_reply_info: Some(false),
                ..Default::default()
            }),
            ..Default::default()
        });
        sam_options
    }
}