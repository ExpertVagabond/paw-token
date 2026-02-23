# $PAW Token Launch Checklist

## Pre-Launch (You Do These)

### 1. Create Token Image
- [ ] Design 1000x1000px PNG with lobster claw / paw motif
- [ ] Save to `../assets/paw-token.png`
- [ ] Optional: 1500x500 banner for DexScreener

### 2. Check Wallet Balance
```bash
solana balance
# Need at least 0.1 SOL (0.05 dev buy + fees)
```

### 3. Install Dependencies
```bash
pip install solders requests
```

### 4. Dry Run (Uploads metadata, doesn't create token)
```bash
python create-token.py --dry-run
```

### 5. Launch
```bash
python create-token.py --dev-buy 0.05
# Type 'LAUNCH' when prompted
```

## Post-Launch (Automated + Manual)

### 6. Update MCP Server
- [ ] Copy mint address from `../assets/token-info.json`
- [ ] Set `PAW_TOKEN_MINT` in `../mcp-server/server.py`

### 7. Register MCP Server
Add to Claude Code settings (`~/.claude/settings.json` or VS settings):
```json
{
  "mcpServers": {
    "paw-token-gate": {
      "command": "python3",
      "args": ["/Volumes/Virtual Server/projects/paw-token/mcp-server/server.py"]
    }
  }
}
```

### 8. Announce on Moltbook
Post to these submolts:
- m/crypto — Token launch announcement
- m/agents — Swarm utility token
- m/tooling — First token-gated MCP server
- m/openclaw — Integration with OpenClaw agents

### 9. Announce on Twitter
- Thread: What $PAW is, why token-gated MCP matters, how to use it
- Tag: @expertvagabond, link pump.fun page

### 10. DexScreener Setup (After Graduation)
- Claim token on DexScreener
- Add banner, description, social links

## Token Details

| Field | Value |
|-------|-------|
| Name | $PAW |
| Ticker | PAW |
| Network | Solana (pump.fun) |
| Supply | 1,000,000,000 (standard pump.fun) |
| Decimals | 6 |
| Dev Buy | 0.05 SOL |
| Graduation | ~85 SOL raised → PumpSwap AMM |
| Creator Revenue | 0.30% of trading volume (pre-grad), up to 0.95% (post-grad) |

## Utility Hooks

| Feature | Min Balance | Description |
|---------|-------------|-------------|
| Access Check | 0 | Anyone can check their balance |
| Swarm Search | 1,000 PAW | Query ChromaDB knowledge base |
| Agent Status | 1,000 PAW | View swarm agent status |
| Memory Share | 1,000 PAW | Contribute to swarm memory |
| Task Queue (low) | 500 PAW | Queue task, 1 agent |
| Task Queue (normal) | 1,000 PAW | Queue task, 2 agents |
| Task Queue (high) | 10,000 PAW | Priority task, 3 agents |
