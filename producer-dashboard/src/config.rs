
use std::path::PathBuf;

use clap::Parser;


/// Awesome producer proxy
/// 
/// TODO
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Config {
    /// RPC port for the proxy
    #[arg(short, long, default_value_t = 3000)]
    rpc_port: u16,

    // TODO(adonagy)
    // /// Path to the producer's private key file
    // /// 
    // /// MINA_PRIVKEY_PASS environmental variable must be set!
    // #[arg(short, long)]
    // private_key_path: PathBuf,
}