use anyhow::{anyhow, Context, Result};
use structopt::StructOpt;

mod gg20_sm_client;
use gg20_sm_client::{join_computation, Claims};

use std::path::PathBuf;

use multi_party_ecdsa::protocols::multi_party_ecdsa::gg_2020::state_machine::sign::{
    OfflineStage, SignManual,
};
use round_based::async_runtime::AsyncProtocol;
use round_based::Msg;

use curv::arithmetic::Converter;
use curv::BigInt;

use futures::{SinkExt, StreamExt, TryStreamExt};

use jsonwebtoken::{Validation, Algorithm, decode, DecodingKey};

#[derive(StructOpt, Debug)]
struct Cli {
    #[structopt(short, long, default_value= "http://localhost:8000/")]
    address: surf::Url,
    #[structopt(short, long)]
    submission: surf::Url,
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

    let (_i, incoming, _outgoing) =
        join_computation::<String>(args.address.clone(), &format!("{}-jwt", args.room))
            .await
            .context("join offline computation")?;

    tokio::pin!(incoming);

    let mut stream_index = 0;

    while let Some(jwt) = incoming.next().await {
        let key = b"secret";
        // Encoded by the API
        //let claims = Claims {
            //chain: "ethereum".to_string(),
            //parent_hash: "0xf26200a961237db4c3d3d00af839a9a220aa5c3d5301c07ba0143d4b05b1436d".to_string(),
            //blocknumber: "16284668".to_string(),
            //exp: 10000000000
        //};
        //let token = encode(&Header::default(), &claims, &EncodingKey::from_secret(key)).unwrap();

        let token = jwt.unwrap().body;

        println!("JWT token: {:?}", token);

        let validation = Validation::new(Algorithm::HS256);
        let token_data = decode::<Claims>(&token, &DecodingKey::from_secret(key) , &validation).unwrap();
        println!("Decoded token: {:?}", token_data);

        let info = token_data.claims;

        let (i, _, outgoing) =
            join_computation(args.address.clone(), &args.room)
                .await
                .context("join computation")?;

        tokio::pin!(outgoing);

        outgoing
            .send(Msg {
                sender: i,
                receiver: None,
                body: info.clone(),
            })
            .await?;

        let local_share = tokio::fs::read(args.local_share.clone())
            .await
            .context("cannot read local share")?;
        let local_share = serde_json::from_slice(&local_share).context("parse local share")?;

        let (i, incoming, outgoing) =
            join_computation(args.address.clone(), &format!("{}-{}-offline", args.room, stream_index))
                .await
                .context("join offline computation")?;

        let incoming = incoming.fuse();
        tokio::pin!(incoming);
        tokio::pin!(outgoing);

        let number_of_parties = args.parties.len();

        let signing = OfflineStage::new(i, args.parties.clone(), local_share)?;
        let completed_offline_stage = AsyncProtocol::new(signing, incoming, outgoing)
            .run()
            .await
            .map_err(|e| anyhow!("protocol execution terminated with error: {}", e))?;

        let (_i, incoming, _outgoing) = join_computation(args.address.clone(), &format!("{}-{}-online", args.room, stream_index))
            .await
            .context("join online computation")?;

        stream_index += 1;

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

        let r = BigInt::from_bytes(signature.r.to_bytes().as_ref()).to_str_radix(16);
        let s = BigInt::from_bytes(signature.s.to_bytes().as_ref()).to_str_radix(16);
        let v = signature.recid;

        let client = reqwest::Client::new();
        client.post(&args.submission.to_string())
            .body(format!(r#"{{"r": {}, "s": {}, "v": {}}}"#, r, s, v))
            .send()?;


        let signature = serde_json::to_string(&signature).context("serialize signature")?;
        println!("Signature: {}", signature);
    }

    Ok(())
}
