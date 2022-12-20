use anyhow::{Context, Result};
use structopt::StructOpt;

mod gg20_sm_client;
use gg20_sm_client::join_computation;

use futures::{SinkExt, StreamExt, TryStreamExt};

use round_based::Msg;

use serde::{Serialize, Deserialize};

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
struct BlockInfo {
    selector: String,
    parent_hash: String,
    blocknumber: String,
    address: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Cli = Cli::from_args();

    let (i, incoming, outgoing) =
        join_computation(args.address.clone(), &args.room)
            .await
            .context("join computation")?;

    let incoming = incoming.fuse();
    tokio::pin!(incoming);
    tokio::pin!(outgoing);

    outgoing
        .send(Msg {
            sender: i+1,
            receiver: None,
            body: BlockInfo {
                selector: "sel".to_string(),
                parent_hash: "0x01".to_string(),
                blocknumber: "420".to_string(),
                address: "vitalik.eth".to_string(),
            },
        })
        .await?;
    
    let val: Vec<_> = incoming
                .take(1)
                .map_ok(|msg| msg.body)
                .try_collect()
                .await?;

    println!("{:?}", val);

    Ok(())
}
