"""
$PAW Token Launch Script — PumpPortal API

Creates $PAW token on pump.fun via PumpPortal's local transaction API.
Your private key never leaves your machine.

Requirements:
- pip install solders requests
- Solana wallet with ~0.1 SOL (creation + initial dev buy)
- Token image at ../assets/paw-token.png

Usage:
    python create-token.py [--dev-buy 0.05]
"""

import argparse
import json
import sys
import os
import time
import requests
from pathlib import Path

# PumpPortal endpoints
PUMPPORTAL_IPFS = "https://pump.fun/api/ipfs"
PUMPPORTAL_TRADE = "https://pumpportal.fun/api/trade-local"

# Token metadata
TOKEN_NAME = "$PAW"
TOKEN_TICKER = "PAW"
TOKEN_DESCRIPTION = (
    "The first token-gated MCP server on Solana. "
    "Hold $PAW to access the OpenPaw swarm: 9 AI agents, "
    "semantic search across 132+ memory chunks, agent coordination, "
    "and priority task queuing. Built by OpenPaw_PSM."
)
TOKEN_WEBSITE = ""  # Set before launch
TOKEN_TWITTER = "https://x.com/expertvagabond"
TOKEN_TELEGRAM = ""  # Set before launch

# Paths
ASSETS_DIR = Path(__file__).parent.parent / "assets"
TOKEN_IMAGE = ASSETS_DIR / "paw-token.png"
WALLET_PATH = os.path.expanduser("~/.config/solana/id.json")


def load_keypair(path: str):
    """Load Solana keypair from JSON file."""
    from solders.keypair import Keypair
    with open(path) as f:
        secret = json.load(f)
    return Keypair.from_bytes(bytes(secret))


def upload_metadata(image_path: str) -> str:
    """Upload token image and metadata to IPFS via pump.fun."""
    print(f"Uploading metadata to IPFS...")

    if not os.path.exists(image_path):
        print(f"ERROR: Token image not found at {image_path}")
        print(f"Create a 1000x1000 PNG token image and save it there.")
        sys.exit(1)

    with open(image_path, "rb") as img:
        files = {"file": (os.path.basename(image_path), img, "image/png")}
        data = {
            "name": TOKEN_NAME,
            "symbol": TOKEN_TICKER,
            "description": TOKEN_DESCRIPTION,
            "website": TOKEN_WEBSITE,
            "twitter": TOKEN_TWITTER,
            "telegram": TOKEN_TELEGRAM,
            "showName": "true"
        }
        resp = requests.post(PUMPPORTAL_IPFS, files=files, data=data)

    if resp.status_code != 200:
        print(f"IPFS upload failed: {resp.status_code} {resp.text[:200]}")
        sys.exit(1)

    result = resp.json()
    metadata_uri = result.get("metadataUri", "")
    print(f"Metadata URI: {metadata_uri}")
    return metadata_uri


def create_token(keypair, metadata_uri: str, dev_buy_sol: float = 0.05):
    """Create token on pump.fun via PumpPortal local transaction API."""
    from solders.keypair import Keypair as SoldersKeypair
    from solders.transaction import VersionedTransaction

    # Generate a new keypair for the token mint
    mint_keypair = SoldersKeypair()
    mint_address = str(mint_keypair.pubkey())

    print(f"\nToken mint address: {mint_address}")
    print(f"Creator wallet: {keypair.pubkey()}")
    print(f"Dev buy: {dev_buy_sol} SOL")
    print(f"Metadata: {metadata_uri}")

    # Request transaction from PumpPortal
    print(f"\nRequesting transaction from PumpPortal...")
    resp = requests.post(PUMPPORTAL_TRADE, json={
        "publicKey": str(keypair.pubkey()),
        "action": "create",
        "tokenMetadata": {
            "name": TOKEN_NAME,
            "symbol": TOKEN_TICKER,
            "uri": metadata_uri
        },
        "mint": mint_address,
        "denominatedInSol": "true",
        "amount": dev_buy_sol,
        "slippage": 10,
        "priorityFee": 0.0005,
        "pool": "pump"
    })

    if resp.status_code != 200:
        print(f"PumpPortal error: {resp.status_code} {resp.text[:200]}")
        sys.exit(1)

    # Deserialize, sign, and send
    tx_bytes = resp.content
    tx = VersionedTransaction.from_bytes(tx_bytes)

    # Sign with both creator and mint keypairs
    signed_tx = VersionedTransaction(tx.message, [keypair, mint_keypair])

    print(f"Sending transaction to Solana...")
    send_resp = requests.post(
        "https://api.mainnet-beta.solana.com",
        json={
            "jsonrpc": "2.0",
            "id": 1,
            "method": "sendTransaction",
            "params": [
                str(signed_tx),  # base58 encoded
                {"encoding": "base58", "skipPreflight": False}
            ]
        },
        headers={"Content-Type": "application/json"}
    )

    result = send_resp.json()
    if "error" in result:
        print(f"Transaction failed: {json.dumps(result['error'], indent=2)}")
        sys.exit(1)

    tx_sig = result.get("result", "")
    print(f"\nTransaction signature: {tx_sig}")
    print(f"Explorer: https://solscan.io/tx/{tx_sig}")
    print(f"Pump.fun: https://pump.fun/coin/{mint_address}")

    # Save token info
    token_info = {
        "mint_address": mint_address,
        "creator": str(keypair.pubkey()),
        "tx_signature": tx_sig,
        "metadata_uri": metadata_uri,
        "dev_buy_sol": dev_buy_sol,
        "timestamp": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "pump_fun_url": f"https://pump.fun/coin/{mint_address}",
        "solscan_url": f"https://solscan.io/tx/{tx_sig}"
    }

    info_path = ASSETS_DIR / "token-info.json"
    with open(info_path, "w") as f:
        json.dump(token_info, f, indent=2)
    print(f"\nToken info saved to {info_path}")

    return token_info


def main():
    parser = argparse.ArgumentParser(description="Launch $PAW token on pump.fun")
    parser.add_argument("--dev-buy", type=float, default=0.05,
                        help="SOL amount for initial dev buy (default: 0.05)")
    parser.add_argument("--wallet", type=str, default=WALLET_PATH,
                        help="Path to Solana keypair JSON")
    parser.add_argument("--dry-run", action="store_true",
                        help="Upload metadata only, don't create token")
    args = parser.parse_args()

    print("=" * 50)
    print("$PAW TOKEN LAUNCH — pump.fun via PumpPortal")
    print("=" * 50)
    print()

    # Check wallet
    if not os.path.exists(args.wallet):
        print(f"ERROR: Wallet not found at {args.wallet}")
        sys.exit(1)

    keypair = load_keypair(args.wallet)
    print(f"Wallet: {keypair.pubkey()}")

    # Check balance
    resp = requests.post("https://api.mainnet-beta.solana.com", json={
        "jsonrpc": "2.0", "id": 1,
        "method": "getBalance",
        "params": [str(keypair.pubkey())]
    })
    balance_lamports = resp.json().get("result", {}).get("value", 0)
    balance_sol = balance_lamports / 1e9
    print(f"Balance: {balance_sol:.4f} SOL")

    min_required = args.dev_buy + 0.01  # dev buy + fees
    if balance_sol < min_required:
        print(f"ERROR: Need at least {min_required} SOL (dev buy {args.dev_buy} + 0.01 fees)")
        sys.exit(1)

    # Upload metadata
    metadata_uri = upload_metadata(str(TOKEN_IMAGE))

    if args.dry_run:
        print(f"\n--- DRY RUN --- Metadata uploaded but token not created.")
        print(f"Metadata URI: {metadata_uri}")
        return

    # Confirm
    print(f"\n{'=' * 50}")
    print(f"READY TO LAUNCH")
    print(f"  Token: {TOKEN_NAME} ({TOKEN_TICKER})")
    print(f"  Dev buy: {args.dev_buy} SOL")
    print(f"  Wallet balance: {balance_sol:.4f} SOL")
    print(f"{'=' * 50}")
    confirm = input("Type 'LAUNCH' to proceed: ")
    if confirm != "LAUNCH":
        print("Aborted.")
        sys.exit(0)

    # Create token
    token_info = create_token(keypair, metadata_uri, args.dev_buy)

    print(f"\n{'=' * 50}")
    print(f"$PAW LAUNCHED!")
    print(f"  Mint: {token_info['mint_address']}")
    print(f"  pump.fun: {token_info['pump_fun_url']}")
    print(f"{'=' * 50}")
    print(f"\nNext steps:")
    print(f"  1. Update PAW_TOKEN_MINT in mcp-server/server.py")
    print(f"  2. Register MCP server in Claude Code settings")
    print(f"  3. Announce on Moltbook and Twitter")


if __name__ == "__main__":
    main()
