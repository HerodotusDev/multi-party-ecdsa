use anyhow::{anyhow, Context, Result};
use structopt::StructOpt;

mod gg20_sm_client;
use gg20_sm_client::{join_computation, BlockInfo};

use std::path::PathBuf;

use multi_party_ecdsa::protocols::multi_party_ecdsa::gg_2020::state_machine::sign::{
    OfflineStage, SignManual,
};
use round_based::async_runtime::AsyncProtocol;
use round_based::Msg;

use curv::arithmetic::Converter;
use curv::BigInt;

use futures::{SinkExt, StreamExt, TryStreamExt};

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
        parent_hash: "0xf26200a961237db4c3d3d00af839a9a220aa5c3d5301c07ba0143d4b05b1436d".to_string(),
        blocknumber: "16284668".to_string(),
        address: "vitalik.eth".to_string(),
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

    let (_i, incoming, _outgoing) = join_computation(args.address, &format!("{}-online", args.room))
        .await
        .context("join online computation")?;

    tokio::pin!(incoming);

    let (signing, _partial_signature) = SignManual::new(
        BigInt::from_bytes(&bincode::serialize(&info).unwrap()),
        completed_offline_stage,
    )?;

    let partial_signatures: Vec<_> = incoming
        .take(number_of_parties-1)
        .map_ok(|msg| msg.body)
        .try_collect()
        .await?;

    let signature = signing
        .complete(&partial_signatures)
        .context("online stage failed")?;
    let signature = serde_json::to_string(&signature).context("serialize signature")?;
    println!("{}", signature);

    Ok(())
}
