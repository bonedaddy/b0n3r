use i2p::sam_options::SAMOptions;
use serde::{Serialize, Deserialize};

#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct Tunnel {
    pub in_length: u8,
    pub in_quantity: u8,
    pub out_length: u8,
    pub out_quantity: u8,
    pub name: String,
}

impl Tunnel {
    pub fn options(&self) -> SAMOptions {
        let mut sam_options = SAMOptions::default();
        if let Some(i2cp_options) = sam_options.i2cp_options.as_mut() {
            if let Some(router_options) = i2cp_options.router_options.as_mut() {
                if let Some(inbound_options) = router_options.inbound.as_mut() {
                    inbound_options.length = Some(self.in_length);
                    inbound_options.quantity = Some(self.in_quantity);
                }
                if let Some(outbound_options) = router_options.outbound.as_mut() {
                    outbound_options.length = Some(self.out_length);
                    outbound_options.quantity = Some(self.out_quantity);
                }
            }
        }
        sam_options
    }
}