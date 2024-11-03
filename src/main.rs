use solana_client::{
    rpc_client::RpcClient,
    rpc_client::GetConfirmedSignaturesForAddress2Config,
    rpc_config::RpcTransactionConfig,
};
use solana_sdk::{
    pubkey::Pubkey,
    signature::Signature,
    commitment_config::CommitmentConfig,
};
use solana_transaction_status::{UiTransactionEncoding, UiInstruction, UiParsedInstruction};
use serde_json::Value;
use std::str::FromStr;
use tokio::time::{sleep, Duration};
use reqwest::Client;

#[tokio::main]
async fn main() {
    let api_key = ""; // 替换为您的实际 helius API 密钥
    let discord_webhook_url = ""; // 替换为您的 Discord Webhook URL
    let rpc_url = format!("https://rpc.helius.xyz?api-key={}", api_key);
    let address_str = "39azUYFWPz3VHgKCf3VChUwbpURdCHRxjWVowf5jUJjg"; //Pump.fun-Raydium 迁移地址
    let address = Pubkey::from_str(address_str).expect("无效的公钥");
    let rpc_client = RpcClient::new(&rpc_url);
    let mut last_processed_signature = None;
    let raydium_liquidity_pool_owner = "675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8";
    let excluded_mint_address = "So11111111111111111111111111111111111111112";
    let mut mint_address_count = 0;
    let http_client = Client::new();

    println!("开始监控Pump.fun: Raydium Migration");

    loop {
        let config = GetConfirmedSignaturesForAddress2Config {
            commitment: Some(CommitmentConfig::confirmed()),
            limit: Some(10),
            ..GetConfirmedSignaturesForAddress2Config::default()
        };
        let signatures_result = rpc_client.get_signatures_for_address_with_config(&address, config);

        match signatures_result {
            Ok(signatures) => {
                let mut new_signatures = Vec::new();
                for signature_info in signatures {
                    let signature_str = &signature_info.signature;
                    if Some(signature_str) != last_processed_signature.as_ref() {
                        new_signatures.push(signature_str.clone());
                    } else {
                        break;
                    }
                }

                if let Some(latest_signature) = new_signatures.first() {
                    last_processed_signature = Some(latest_signature.clone());
                }

                new_signatures.reverse();
                for signature_str in new_signatures {
                    println!("检测到新交易: {}", signature_str);

                    let signature = signature_str.parse::<Signature>().expect("无效的签名");

                    match rpc_client.get_transaction_with_config(
                        &signature,
                        RpcTransactionConfig {
                            encoding: Some(UiTransactionEncoding::JsonParsed),
                            commitment: Some(CommitmentConfig::confirmed()),
                            max_supported_transaction_version: Some(0),
                        },
                    ) {
                        Ok(tx) => {
                            let mut found_liquidity_pool = false;
                            if let Some(meta) = tx.transaction.meta {
                                if let Some(inner_instructions) = Option::<Vec<_>>::from(meta.inner_instructions.clone()) {
                                    for inner in &inner_instructions {
                                        for instruction in &inner.instructions {
                                            if let UiInstruction::Parsed(parsed_instruction) = instruction {
                                                if let UiParsedInstruction::Parsed(parsed) = parsed_instruction {
                                                    if parsed.program == "system" {
                                                        if let Value::Object(map) = &parsed.parsed {
                                                            if let Some(Value::Object(info)) = map.get("info") {
                                                                if let Some(Value::String(owner)) = info.get("owner") {
                                                                    if owner == raydium_liquidity_pool_owner {
                                                                        found_liquidity_pool = true;
                                                                        println!("检测到 Raydium 流动性池 V4 交易。");
                                                                        break;
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }

                                    if found_liquidity_pool {
                                        for inner in &inner_instructions {
                                            for instruction in &inner.instructions {
                                                if let UiInstruction::Parsed(parsed_instruction) = instruction {
                                                    if let UiParsedInstruction::Parsed(parsed) = parsed_instruction {
                                                        if parsed.program == "spl-token" {
                                                            if let Value::Object(map) = &parsed.parsed {
                                                                if let Some(Value::String(instruction_type)) = map.get("type") {
                                                                    if instruction_type == "initializeAccount" {
                                                                        if let Some(Value::Object(info)) = map.get("info") {
                                                                            if let Some(Value::String(mint_address)) = info.get("mint") {
                                                                                if mint_address != excluded_mint_address {
                                                                                    mint_address_count += 1;
                                                                                    println!("✅️新代币: {} | 总计: {}", mint_address, mint_address_count);

                                                                                    // 发送到 Discord
                                                                                    let content = format!("✅️新代币: {}\n[ape.pro](https://ape.pro/solana/{}) ", mint_address, mint_address);
                                                                                    if let Err(e) = send_to_discord(&http_client, discord_webhook_url, &content).await {
                                                                                        eprintln!("发送到 Discord 失败: {}", e);
                                                                                    }
                                                                                }
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    } else {
                                        println!("未检测到匹配的 Raydium 流动性池 V4 交易。");
                                    }
                                } else {
                                    println!("未找到内部指令");
                                }
                            } else {
                                println!("未找到交易元数据");
                            }
                        }
                        Err(err) => {
                            eprintln!("获取交易失败: {}", err);
                        }
                    }
                }
            }
            Err(err) => {
                eprintln!("获取签名失败: {}", err);
            }
        }

        println!("等待10秒");
        sleep(Duration::from_secs(10)).await;
    }
}

async fn send_to_discord(client: &Client, webhook_url: &str, content: &str) -> Result<(), reqwest::Error> {
    let json = serde_json::json!({
        "content": content,
    });
    client.post(webhook_url)
        .json(&json)
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}
