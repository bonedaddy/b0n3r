use anyhow::{Result, anyhow};
use config::Configuration;
use clap;
use i2p::net::I2pStream;
use std::io::{Write, Read};

pub fn echo_client_test(matches: &clap::ArgMatches, config_file_path: &str) -> Result<()> {
    let cfg = Configuration::load(config_file_path)?;
    let mut i2p_stream = match I2pStream::connect(matches.value_of("destination").unwrap()) {
        Ok(i2p_stream) => i2p_stream,
        Err(err) => return Err(anyhow!("failed to connect to destination {:#?}", err)),
    };
    let n = i2p_stream.write(String::from("hello_world").as_bytes())?;
    println!("client wrote {} bytes", n);
    loop {
        let n = i2p_stream.write(String::from("hello_world").as_bytes())?;
        println!("client wrote {} bytes", n);
        let mut buf = [0_u8; 1024];
        let n = i2p_stream.read(&mut buf)?;
        if n == 0 {
            std::thread::sleep(std::time::Duration::from_millis(250));
            continue;
        }
        unsafe{println!("client read {} bytes. msg {}", n, String::from_utf8_unchecked(buf[0..n].to_vec()))}
    }

    Ok(())
}