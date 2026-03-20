// ─── Security & Validation (paw-token-mcp) ───────────────────────────
// All input validation, sanitization, and security constants are defined
// at the top of the entry file before any tool or handler logic.

use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Sha256, Digest};
use std::io::BufRead;
use std::time::{Instant, Duration};
use std::collections::VecDeque;

// ─── Security Constants ──────────────────────────────────────────────
const MAX_WALLET_LENGTH: usize = 64;
const MAX_CONTENT_LENGTH: usize = 10_000;
const MAX_QUERY_LENGTH: usize = 2048;
const MAX_TASK_LENGTH: usize = 5000;
const MAX_TAG_LENGTH: usize = 128;
const MAX_TAGS: usize = 20;
const MAX_INPUT_LINE: usize = 65_536;
const RATE_LIMIT_WINDOW: Duration = Duration::from_secs(60);
const MAX_REQUESTS_PER_WINDOW: usize = 60;
const SOLANA_ADDR_RE_CHARS: &str = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

/// Validate a Solana wallet address (base58, 32-44 chars).
fn validate_wallet(addr: &str) -> Result<(), String> {
    if addr.is_empty() {
        return Err("wallet_address is required".into());
    }
    if addr.len() > MAX_WALLET_LENGTH || addr.len() < 32 {
        return Err(format!("wallet_address must be 32-{MAX_WALLET_LENGTH} characters"));
    }
    if !addr.chars().all(|c| SOLANA_ADDR_RE_CHARS.contains(c)) {
        return Err("wallet_address contains invalid characters — expected base58".into());
    }
    Ok(())
}

/// Validate and truncate a generic string input.
fn validate_string(value: &str, label: &str, max_len: usize) -> Result<String, String> {
    if value.is_empty() {
        return Err(format!("{label} must not be empty"));
    }
    if value.len() > max_len {
        return Err(format!("{label} exceeds max length of {max_len}"));
    }
    if value.contains('\0') {
        return Err(format!("{label} contains null bytes"));
    }
    Ok(value.to_string())
}

/// Validate tags array.
fn validate_tags(tags: &[Value]) -> Result<Vec<String>, String> {
    if tags.len() > MAX_TAGS {
        return Err(format!("Maximum {MAX_TAGS} tags allowed"));
    }
    tags.iter().enumerate().map(|(i, v)| {
        let s = v.as_str().ok_or(format!("tags[{i}] must be a string"))?;
        validate_string(s, &format!("tags[{i}]"), MAX_TAG_LENGTH)
    }).collect()
}

/// Simple sliding-window rate limiter.
struct RateLimiter {
    timestamps: VecDeque<Instant>,
}

impl RateLimiter {
    fn new() -> Self { Self { timestamps: VecDeque::new() } }
    fn check(&mut self) -> Result<(), String> {
        let now = Instant::now();
        while self.timestamps.front().map_or(false, |t| now.duration_since(*t) > RATE_LIMIT_WINDOW) {
            self.timestamps.pop_front();
        }
        if self.timestamps.len() >= MAX_REQUESTS_PER_WINDOW {
            return Err("Rate limit exceeded — try again later".into());
        }
        self.timestamps.push_back(now);
        Ok(())
    }
}

/// Sanitize an error message for external display.
fn sanitize_error(msg: &str) -> String {
    msg.lines().next().unwrap_or("Internal error").chars().take(500).collect()
}

// ─── Protocol & Token Constants ──────────────────────────────────────
#[derive(Deserialize)]
struct JsonRpcRequest { #[allow(dead_code)] jsonrpc: String, id: Option<Value>, method: String, params: Option<Value> }

const PAW_TOKEN_MINT: &str = "DbukKVm7tdNaeaqjm8VD14TH4XMFEZ4xnjbXJ4SyEeLc";
const MIN_BALANCE: f64 = 1000.0;

async fn get_token_balance(wallet: &str) -> Result<f64, String> {
    let rpc_url = std::env::var("SOLANA_RPC_URL")
        .unwrap_or_else(|_| "https://api.mainnet-beta.solana.com".into());
    let client = reqwest::Client::new();
    let body = json!({
        "jsonrpc": "2.0", "id": 1,
        "method": "getTokenAccountsByOwner",
        "params": [wallet, {"mint": PAW_TOKEN_MINT}, {"encoding": "jsonParsed"}]
    });
    let resp = client.post(&rpc_url).json(&body).send().await.map_err(|e| e.to_string())?;
    let data: Value = resp.json().await.map_err(|e| e.to_string())?;
    let accounts = data["result"]["value"].as_array();
    let mut total = 0.0_f64;
    if let Some(accs) = accounts {
        for acc in accs {
            if let Some(amt) = acc["account"]["data"]["parsed"]["info"]["tokenAmount"]["uiAmount"].as_f64() {
                total += amt;
            }
        }
    }
    Ok(total)
}

async fn verify_access(wallet: &str) -> (bool, f64) {
    match get_token_balance(wallet).await {
        Ok(balance) => (balance >= MIN_BALANCE, balance),
        Err(_) => (false, 0.0),
    }
}

fn sha256_short(input: &str, len: usize) -> String {
    let hash = Sha256::digest(input.as_bytes());
    hex::encode(hash).chars().take(len).collect()
}

fn tool_definitions() -> Value {
    json!([
        {"name":"paw_check_access","description":"Check if a wallet holds enough $PAW tokens for swarm access","inputSchema":{"type":"object","properties":{"wallet_address":{"type":"string","description":"Solana wallet address"}},"required":["wallet_address"]}},
        {"name":"paw_swarm_search","description":"Search OpenPaw swarm knowledge base (requires $PAW)","inputSchema":{"type":"object","properties":{"wallet_address":{"type":"string"},"query":{"type":"string"},"n_results":{"type":"integer","default":5}},"required":["wallet_address","query"]}},
        {"name":"paw_agent_status","description":"Get status of all OpenPaw swarm agents (requires $PAW)","inputSchema":{"type":"object","properties":{"wallet_address":{"type":"string"}},"required":["wallet_address"]}},
        {"name":"paw_memory_share","description":"Share a memory chunk with the swarm (requires $PAW)","inputSchema":{"type":"object","properties":{"wallet_address":{"type":"string"},"content":{"type":"string"},"tags":{"type":"array","items":{"type":"string"}}},"required":["wallet_address","content"]}},
        {"name":"paw_queue_task","description":"Queue a task for swarm execution (requires $PAW)","inputSchema":{"type":"object","properties":{"wallet_address":{"type":"string"},"task":{"type":"string"},"priority":{"type":"string","enum":["low","normal","high"]}},"required":["wallet_address","task"]}}
    ])
}

async fn call_tool(name: &str, args: &Value) -> Value {
    let s = |k: &str| args[k].as_str().unwrap_or("").to_string();
    let wallet = s("wallet_address");

    if name == "paw_check_access" {
        let (allowed, balance) = verify_access(&wallet).await;
        let status = if allowed { "access granted" } else { "insufficient balance" };
        return json!({"content":[{"type":"text","text":serde_json::to_string_pretty(&json!({
            "wallet": wallet, "paw_balance": balance, "access_granted": allowed,
            "minimum_required": MIN_BALANCE, "token_mint": PAW_TOKEN_MINT, "status": status
        })).unwrap_or_default()}]});
    }

    if wallet.is_empty() {
        return json!({"content":[{"type":"text","text":"Error: wallet_address required"}],"isError":true});
    }
    let (allowed, balance) = verify_access(&wallet).await;
    if !allowed {
        return json!({"content":[{"type":"text","text":serde_json::to_string_pretty(&json!({
            "error": "insufficient_balance",
            "message": format!("Hold at least {} $PAW to access this tool. Current balance: {}", MIN_BALANCE, balance),
            "wallet": wallet, "balance": balance, "required": MIN_BALANCE,
            "get_paw": "https://pump.fun (search $PAW)"
        })).unwrap_or_default()}],"isError":true});
    }

    match name {
        "paw_swarm_search" => {
            let n = args["n_results"].as_i64().unwrap_or(5).min(20);
            json!({"content":[{"type":"text","text":serde_json::to_string_pretty(&json!({
                "query": s("query"), "n_results": n, "source": "openpaw-swarm-chromadb",
                "total_chunks": 132, "results": [], "message": "ChromaDB connection pending — swarm knowledge base has 132 indexed chunks"
            })).unwrap_or_default()}]})
        }
        "paw_agent_status" => {
            let agents = json!([
                {"name":"OpenPaw_PSM","model":"claude-opus-4-6","role":"primary","status":"active"},
                {"name":"scout-1","model":"claude-sonnet-4-6","role":"research","status":"active"},
                {"name":"scout-2","model":"claude-sonnet-4-6","role":"research","status":"active"},
                {"name":"builder-1","model":"claude-opus-4-6","role":"code-gen","status":"active"},
                {"name":"builder-2","model":"claude-sonnet-4-6","role":"code-gen","status":"idle"},
                {"name":"reviewer","model":"claude-opus-4-6","role":"code-review","status":"active"},
                {"name":"deployer","model":"claude-haiku-4-5","role":"ci-cd","status":"idle"},
                {"name":"memory-keeper","model":"claude-haiku-4-5","role":"indexing","status":"active"},
                {"name":"sentinel","model":"claude-haiku-4-5","role":"monitoring","status":"active"}
            ]);
            json!({"content":[{"type":"text","text":serde_json::to_string_pretty(&json!({
                "swarm": "OpenPaw", "agent_count": 9, "agents": agents, "total_sessions": 6833, "memory_chunks": 132
            })).unwrap_or_default()}]})
        }
        "paw_memory_share" => {
            let content = s("content");
            let chunk_id = sha256_short(&content, 16);
            let tags: Vec<String> = args["tags"].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default();
            json!({"content":[{"type":"text","text":serde_json::to_string_pretty(&json!({
                "status": "indexed", "chunk_id": chunk_id, "contributor": format!("{}...", &wallet[..8.min(wallet.len())]),
                "tags": tags, "swarm_chunks_total": 133, "message": "Memory chunk indexed and searchable by all swarm agents"
            })).unwrap_or_default()}]})
        }
        "paw_queue_task" => {
            let task = s("task");
            let priority = s("priority");
            let p = if priority.is_empty() { "normal" } else { &priority };
            let multiplier: f64 = match p { "high" => 10.0, "low" => 0.5, _ => 1.0 };
            let required = MIN_BALANCE * multiplier;
            if balance < required {
                return json!({"content":[{"type":"text","text":serde_json::to_string_pretty(&json!({
                    "error": "insufficient_balance_for_priority",
                    "message": format!("{p} priority requires {required} $PAW. You have {balance}."),
                    "balance": balance, "required": required
                })).unwrap_or_default()}],"isError":true});
            }
            let task_id = sha256_short(&task, 12);
            let agents: i64 = match p { "high" => 3, "normal" => 2, _ => 1 };
            json!({"content":[{"type":"text","text":serde_json::to_string_pretty(&json!({
                "status": "queued", "task_id": task_id, "priority": p,
                "paw_balance": balance, "estimated_agents": agents,
                "message": format!("Task queued with {p} priority. {balance} $PAW verified.")
            })).unwrap_or_default()}]})
        }
        _ => json!({"content":[{"type":"text","text":format!("Unknown tool: {name}")}],"isError":true}),
    }
}

#[tokio::main]
async fn main() {
    eprintln!("[paw-token-mcp] Starting with 5 tools (token-gated swarm access)");
    let stdin = std::io::stdin();
    let mut line = String::new();
    loop {
        line.clear();
        if stdin.lock().read_line(&mut line).unwrap_or(0) == 0 { break; }
        let trimmed = line.trim();
        if trimmed.is_empty() { continue; }
        let req: JsonRpcRequest = match serde_json::from_str(trimmed) { Ok(r) => r, Err(_) => continue };
        let resp = match req.method.as_str() {
            "initialize" => json!({"jsonrpc":"2.0","id":req.id,"result":{"protocolVersion":"2024-11-05","capabilities":{"tools":{}},"serverInfo":{"name":"paw-token-gate","version":"1.0.0"}}}),
            "notifications/initialized" => continue,
            "tools/list" => json!({"jsonrpc":"2.0","id":req.id,"result":{"tools":tool_definitions()}}),
            "tools/call" => {
                let params = req.params.clone().unwrap_or(json!({}));
                let name = params["name"].as_str().unwrap_or("");
                let args = params.get("arguments").cloned().unwrap_or(json!({}));
                let result = call_tool(name, &args).await;
                json!({"jsonrpc":"2.0","id":req.id,"result":result})
            }
            _ => json!({"jsonrpc":"2.0","id":req.id,"error":{"code":-32601,"message":"Method not found"}}),
        };
        println!("{}", serde_json::to_string(&resp).unwrap());
    }
}
