use anyhow::{anyhow, Context, Result};
use curv::elliptic::curves::Secp256k1;
use multi_party_ecdsa::protocols::multi_party_ecdsa::gg_2020::state_machine::keygen::LocalKey;
use structopt::StructOpt;

use dotenv::dotenv;
use regex::Regex;

use futures::{SinkExt, StreamExt, TryStreamExt};
use curv::arithmetic::Converter;
use curv::BigInt;

use std::path::PathBuf;

mod gg20_sm_client;
use gg20_sm_client::{join_computation, Claims};

use multi_party_ecdsa::protocols::multi_party_ecdsa::gg_2020::state_machine::sign::{
    OfflineStage, SignManual,
};
use round_based::async_runtime::AsyncProtocol;
use round_based::Msg;

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
    dotenv().ok();
    let args: Cli = Cli::from_args();

    let local_share = tokio::fs::read(args.local_share)
        .await
        .context("cannot read local share")?;
    let local_share: LocalKey<Secp256k1> = serde_json::from_slice(&local_share).context("parse local share")?;

    let (_i, incoming, _outgoing) =
        join_computation::<Claims>(args.address.clone(), &args.room)
            .await
            .context("join computation")?;

    tokio::pin!(incoming);

    let mut stream_index = 0;

    // fetch all the blocks info
    while let Some(block_info) = incoming.next().await {
        let data_to_sign = block_info.unwrap();
        println!("Received to sign: {:?}", data_to_sign);

        let block_number = &data_to_sign.body.blocknumber;
        let expected_hash = &data_to_sign.body.parent_hash;

        let client = reqwest::Client::new();
        let rpc = std::env::var("ETHEREUM_RPC").unwrap();

        let res = client.post(&rpc)
            .body(format!(r#"{{"jsonrpc":"2.0","method":"eth_getBlockByNumber","params":["{}", true],"id":1}}"#, block_number))
            .send()?
            .text()?;

        let re = Regex::new(r#""parentHash":"([^"]+)"#).unwrap();
        let captures = re.captures(&res).unwrap();
        let hash = &captures[1];

        // Verifying hash
        assert_eq!(expected_hash, hash, "Invalid hash");

        //let sender = data_to_sign.sender;
        
        let (i, incoming, outgoing) =
            join_computation(args.address.clone(), &format!("{}-{}-offline", args.room, stream_index))
                .await
                .context("join offline computation")?;

        let incoming = incoming.fuse();
        tokio::pin!(incoming);
        tokio::pin!(outgoing);

        println!("1------------------ Before offline ");

        let signing = OfflineStage::new(i, args.parties.clone(), local_share.clone())?;
        let completed_offline_stage = AsyncProtocol::new(signing, incoming, outgoing)
            .run()
            .await
            .map_err(|e| anyhow!("protocol execution terminated with error: {}", e))?;

        println!("2------------------ Offline completed ");

        let (i, _incoming, outgoing) = join_computation(args.address.clone(), &format!("{}-{}-online", args.room, stream_index))
            .await
            .context("join online computation")?;

        stream_index += 1;

        tokio::pin!(outgoing);

        let (_signing, partial_signature) = SignManual::new(
            BigInt::from_bytes(&bincode::serialize(&data_to_sign.body).unwrap()),
            completed_offline_stage,
        )?;

        println!("3------------------ Partial signature completed, sending to master node");

        outgoing
            .send(Msg {
                sender: i,
                // TODO: receiver to master node
                //receiver: Some(sender),
                receiver: None,
                body: partial_signature.clone(),
            })
            .await?;

        //println!("{:?} sent partial_signature {:?} to {:?}", i, partial_signature, sender);
        println!("{:?} sent partial_signature {:?}", i, partial_signature);
    }
    
    Ok(())
}
