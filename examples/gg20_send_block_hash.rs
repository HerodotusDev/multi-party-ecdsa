use anyhow::{Context, Result};
use structopt::StructOpt;

mod gg20_sm_client;
use gg20_sm_client::join_computation;

use round_based::Msg;

use futures::{SinkExt, StreamExt, TryStreamExt};

use serde::{Serialize, Deserialize};

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

#[derive(StructOpt, Debug)]
struct Cli {
    #[structopt(short, long, default_value= "http://localhost:8000/")]
    address: surf::Url,
    #[structopt(short, long, default_value = "block-hashes")]
    room: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Cli = Cli::from_args();

    let (i, _, outgoing) =
        join_computation(args.address.clone(), &args.room)
            .await
            .context("join computation")?;

    tokio::pin!(outgoing);

    outgoing
        .send(Msg {
            sender: i,
            receiver: None,
            body: BlockInfo {
                selector: "sel".to_string(),
                parent_hash: "0x01".to_string(),
                blocknumber: "420".to_string(),
                address: "vitalikkkkk.eth".to_string(),
            },
        })
        .await?;

    Ok(())
}
