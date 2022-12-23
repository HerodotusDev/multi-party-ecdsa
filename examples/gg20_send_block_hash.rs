use anyhow::{anyhow, Context, Result};
use structopt::StructOpt;

mod gg20_sm_client;
use gg20_sm_client::join_computation;

use std::path::PathBuf;

use multi_party_ecdsa::protocols::multi_party_ecdsa::gg_2020::state_machine::sign::{
    OfflineStage, SignManual,
};
use round_based::async_runtime::AsyncProtocol;
use round_based::Msg;

use curv::arithmetic::Converter;
use curv::BigInt;

use futures::{SinkExt, StreamExt, TryStreamExt};

use serde::{Serialize, Deserialize};

//bytes4 - method selector
//bytes32 - parenthash
//uint256 - blocknumber
//address - verifying contract address
#[derive(Debug, Serialize, Deserialize, Clone)]
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
    #[structopt(short, long)]
    local_share: PathBuf,
    #[structopt(short, long, use_delimiter(true))]
    parties: Vec<u16>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Cli = Cli::from_args();

    let (i, _, outgoing) =
        join_computation(args.address.clone(), &args.room)
            .await
            .context("join computation")?;

    tokio::pin!(outgoing);

    let info = BlockInfo {
        selector: "sel".to_string(),
        parent_hash: "0x01".to_string(),
        blocknumber: "420".to_string(),
        address: "vitalikkkkk.eth".to_string(),
    };

    outgoing
        .send(Msg {
            sender: i,
            receiver: None,
            body: info.clone(),
        })
        .await?;

    let local_share = tokio::fs::read(args.local_share)
        .await
        .context("cannot read local share")?;
    let local_share = serde_json::from_slice(&local_share).context("parse local share")?;

    let (i, incoming, outgoing) =
        join_computation(args.address.clone(), &format!("{}-offline", args.room))
            .await
            .context("join offline computation")?;

    let incoming = incoming.fuse();
    tokio::pin!(incoming);
    tokio::pin!(outgoing);

    let number_of_parties = args.parties.len();

    let signing = OfflineStage::new(i, args.parties, local_share)?;
    let completed_offline_stage = AsyncProtocol::new(signing, incoming, outgoing)
        .run()
        .await
        .map_err(|e| anyhow!("protocol execution terminated with error: {}", e))?;

    let (i, incoming, outgoing) = join_computation(args.address, &format!("{}-online", args.room))
        .await
        .context("join online computation")?;

    tokio::pin!(incoming);
    tokio::pin!(outgoing);

    let (signing, partial_signature) = SignManual::new(
        BigInt::from_bytes(&bincode::serialize(&info).unwrap()),
        completed_offline_stage,
    )?;

    let mut partial_signatures: Vec<_> = incoming
        .take(number_of_parties-1)
        .map_ok(|msg| msg.body)
        .try_collect()
        .await?;

    partial_signatures.push(partial_signature);

    let signature = signing
        .complete(&partial_signatures)
        .context("online stage failed")?;
    let signature = serde_json::to_string(&signature).context("serialize signature")?;
    println!("{}", signature);

    Ok(())
}
