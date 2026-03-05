use clap::Parser;
use ed25519_dalek::{SigningKey, Signer, VerifyingKey};
use rand::rngs::OsRng;
use serde_json::{json, Value};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "paw-launch", about = "$PAW token launcher — pump.fun via PumpPortal")]
struct Args {
    #[arg(long, default_value = "0.05")]
    dev_buy: f64,
    #[arg(long, default_value_t = default_wallet())]
    wallet: String,
    #[arg(long, default_value = "assets/paw-token.png")]
    image: String,
    #[arg(long)]
    dry_run: bool,
}

fn default_wallet() -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    format!("{}/.config/solana/id.json", home)
}

const TOKEN_NAME: &str = "$PAW";
const TOKEN_TICKER: &str = "PAW";
const TOKEN_DESCRIPTION: &str = "The first token-gated MCP server on Solana. \
Hold $PAW to access the OpenPaw swarm: 9 AI agents, \
semantic search across 132+ memory chunks, agent coordination, \
and priority task queuing. Built by OpenPaw_PSM.";

fn load_keypair(path: &str) -> Result<SigningKey, String> {
    let data = std::fs::read_to_string(path).map_err(|e| format!("read wallet: {e}"))?;
    let bytes: Vec<u8> = serde_json::from_str(&data).map_err(|e| format!("parse wallet: {e}"))?;
    if bytes.len() < 64 {
        return Err(format!("wallet file too short: {} bytes", bytes.len()));
    }
    let mut secret = [0u8; 32];
    secret.copy_from_slice(&bytes[..32]);
    Ok(SigningKey::from_bytes(&secret))
}

fn pubkey_b58(key: &SigningKey) -> String {
    let vk: VerifyingKey = key.verifying_key();
    bs58::encode(vk.as_bytes()).into_string()
}

async fn get_balance(client: &reqwest::Client, pubkey: &str) -> f64 {
    let resp = client
        .post("https://api.mainnet-beta.solana.com")
        .json(&json!({
            "jsonrpc": "2.0", "id": 1,
            "method": "getBalance",
            "params": [pubkey]
        }))
        .send()
        .await;
    match resp {
        Ok(r) => {
            let v: Value = r.json().await.unwrap_or(json!({}));
            v["result"]["value"].as_u64().unwrap_or(0) as f64 / 1e9
        }
        Err(_) => 0.0,
    }
}

async fn upload_metadata(client: &reqwest::Client, image_path: &str) -> Result<String, String> {
    println!("Uploading metadata to IPFS...");
    let path = PathBuf::from(image_path);
    if !path.exists() {
        return Err(format!("Token image not found at {image_path}"));
    }
    let file_bytes = std::fs::read(&path).map_err(|e| format!("read image: {e}"))?;
    let file_name = path.file_name().unwrap_or_default().to_string_lossy().to_string();

    let file_part = reqwest::multipart::Part::bytes(file_bytes)
        .file_name(file_name)
        .mime_str("image/png")
        .unwrap();

    let form = reqwest::multipart::Form::new()
        .part("file", file_part)
        .text("name", TOKEN_NAME)
        .text("symbol", TOKEN_TICKER)
        .text("description", TOKEN_DESCRIPTION)
        .text("twitter", "https://x.com/expertvagabond")
        .text("showName", "true");

    let resp = client
        .post("https://pump.fun/api/ipfs")
        .multipart(form)
        .send()
        .await
        .map_err(|e| format!("IPFS upload: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("IPFS upload failed: {status} {}", &text[..200.min(text.len())]));
    }

    let data: Value = resp.json().await.map_err(|e| format!("parse IPFS response: {e}"))?;
    let uri = data["metadataUri"].as_str().unwrap_or_default().to_string();
    println!("Metadata URI: {uri}");
    Ok(uri)
}

async fn create_token(
    client: &reqwest::Client,
    keypair: &SigningKey,
    metadata_uri: &str,
    dev_buy: f64,
) -> Result<Value, String> {
    let creator = pubkey_b58(keypair);

    // Generate mint keypair
    let mut mint_secret = [0u8; 32];
    rand::RngCore::fill_bytes(&mut OsRng, &mut mint_secret);
    let mint_key = SigningKey::from_bytes(&mint_secret);
    let mint_address = pubkey_b58(&mint_key);

    println!("\nToken mint: {mint_address}");
    println!("Creator: {creator}");
    println!("Dev buy: {dev_buy} SOL");

    // Request transaction from PumpPortal
    println!("\nRequesting transaction from PumpPortal...");
    let resp = client
        .post("https://pumpportal.fun/api/trade-local")
        .json(&json!({
            "publicKey": creator,
            "action": "create",
            "tokenMetadata": {
                "name": TOKEN_NAME,
                "symbol": TOKEN_TICKER,
                "uri": metadata_uri
            },
            "mint": mint_address,
            "denominatedInSol": "true",
            "amount": dev_buy,
            "slippage": 10,
            "priorityFee": 0.0005,
            "pool": "pump"
        }))
        .send()
        .await
        .map_err(|e| format!("PumpPortal request: {e}"))?;

    if !resp.status().is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("PumpPortal error: {}", &text[..200.min(text.len())]));
    }

    let tx_bytes = resp.bytes().await.map_err(|e| format!("read tx: {e}"))?;

    // Sign transaction — we need to sign the message with both keypairs
    // The transaction format from PumpPortal is a serialized VersionedTransaction
    // We sign the message portion (skip first byte count + signatures section)
    // For simplicity, send the raw bytes back to Solana RPC with our signatures
    
    // Extract message from transaction bytes and sign it
    // VersionedTransaction wire format: num_signatures (compact-u16) + signatures + message
    let num_sigs = tx_bytes[0] as usize;
    let sig_start = 1;
    let msg_start = sig_start + num_sigs * 64;
    let message = &tx_bytes[msg_start..];

    let sig1 = keypair.sign(message);
    let sig2 = mint_key.sign(message);

    // Rebuild transaction with our signatures
    let mut signed_tx = Vec::new();
    signed_tx.push(2u8); // 2 signatures
    signed_tx.extend_from_slice(&sig1.to_bytes());
    signed_tx.extend_from_slice(&sig2.to_bytes());
    signed_tx.extend_from_slice(message);

    let tx_b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &signed_tx);

    println!("Sending transaction...");
    let send_resp = client
        .post("https://api.mainnet-beta.solana.com")
        .json(&json!({
            "jsonrpc": "2.0", "id": 1,
            "method": "sendTransaction",
            "params": [tx_b64, {
                "encoding": "base64",
                "skipPreflight": false,
                "preflightCommitment": "confirmed"
            }]
        }))
        .send()
        .await
        .map_err(|e| format!("send tx: {e}"))?;

    let result: Value = send_resp.json().await.map_err(|e| format!("parse result: {e}"))?;
    if let Some(err) = result.get("error") {
        return Err(format!("Transaction failed: {}", serde_json::to_string_pretty(err).unwrap()));
    }

    let tx_sig = result["result"].as_str().unwrap_or("unknown");
    println!("\nTransaction: {tx_sig}");
    println!("Explorer: https://solscan.io/tx/{tx_sig}");
    println!("Pump.fun: https://pump.fun/coin/{mint_address}");

    let info = json!({
        "mint_address": mint_address,
        "creator": creator,
        "tx_signature": tx_sig,
        "metadata_uri": metadata_uri,
        "dev_buy_sol": dev_buy,
        "timestamp": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
        "pump_fun_url": format!("https://pump.fun/coin/{mint_address}"),
        "solscan_url": format!("https://solscan.io/tx/{tx_sig}")
    });

    Ok(info)
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    println!("{}", "=".repeat(50));
    println!("$PAW TOKEN LAUNCH — pump.fun via PumpPortal");
    println!("{}", "=".repeat(50));

    let keypair = load_keypair(&args.wallet).expect("Failed to load wallet");
    let pubkey = pubkey_b58(&keypair);
    println!("Wallet: {pubkey}");

    let client = reqwest::Client::new();
    let balance = get_balance(&client, &pubkey).await;
    println!("Balance: {balance:.4} SOL");

    let min_required = args.dev_buy + 0.01;
    if balance < min_required {
        eprintln!("ERROR: Need at least {min_required} SOL (dev buy {} + 0.01 fees)", args.dev_buy);
        std::process::exit(1);
    }

    let metadata_uri = upload_metadata(&client, &args.image).await.expect("Metadata upload failed");

    if args.dry_run {
        println!("\n--- DRY RUN --- Metadata uploaded but token not created.");
        println!("Metadata URI: {metadata_uri}");
        return;
    }

    println!("\n{}", "=".repeat(50));
    println!("READY TO LAUNCH");
    println!("  Token: {TOKEN_NAME} ({TOKEN_TICKER})");
    println!("  Dev buy: {} SOL", args.dev_buy);
    println!("  Balance: {balance:.4} SOL");
    println!("{}", "=".repeat(50));
    println!("Launching in 3 seconds... (Ctrl+C to abort)");
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    match create_token(&client, &keypair, &metadata_uri, args.dev_buy).await {
        Ok(info) => {
            // Save token info
            let info_path = PathBuf::from(&args.image)
                .parent()
                .unwrap_or(&PathBuf::from("."))
                .join("token-info.json");
            let _ = std::fs::write(&info_path, serde_json::to_string_pretty(&info).unwrap());
            println!("\nToken info saved to {}", info_path.display());

            println!("\n{}", "=".repeat(50));
            println!("$PAW LAUNCHED!");
            println!("  Mint: {}", info["mint_address"].as_str().unwrap_or("?"));
            println!("  pump.fun: {}", info["pump_fun_url"].as_str().unwrap_or("?"));
            println!("{}", "=".repeat(50));
        }
        Err(e) => {
            eprintln!("Launch failed: {e}");
            std::process::exit(1);
        }
    }
}
