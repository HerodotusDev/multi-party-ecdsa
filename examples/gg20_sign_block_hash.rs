use anyhow::{Context, Result};
use structopt::StructOpt;

use futures::{StreamExt, TryStreamExt};
use serde::{Serialize, Deserialize};

mod gg20_sm_client;
use gg20_sm_client::join_computation;

#[derive(StructOpt, Debug)]
struct Cli {
    #[structopt(short, long, default_value= "http://localhost:8000/")]
    address: surf::Url,
    #[structopt(short, long, default_value = "block-hashes")]
    room: String,
}

//bytes4 - method selector
//bytes32 - parenthash
//uint256 - blocknumber
//address - verifying contract address
#[derive(Debug, Serialize, Deserialize)]
pub struct BlockInfo {
    selector: String,
    parent_hash: String,
    blocknumber: String,
    address: String,
}


#[tokio::main]
async fn main() -> Result<()> {
    let args: Cli = Cli::from_args();

    let (_, incoming, outgoing) =
        join_computation::<BlockInfo>(args.address.clone(), &args.room)
            .await
            .context("join computation")?;

    //let incoming = incoming.fuse();
    tokio::pin!(incoming);
    tokio::pin!(outgoing);
    
    // get the receiver, sign the hash and send the signature back to the receiver
    let val: Vec<_> = incoming
                .take(1)
                .map_ok(|msg| msg.body)
                .try_collect()
                .await?;

    println!("{:?}", val);

    Ok(())
}
