use std::{process::exit, u64};

use anyhow::Result;
use clap::Parser;
use protos::{ledger_client::LedgerClient, Account, Action, CreateAccountReq, FreezeAccountRequest, GetAccountReq, 
    GetHistoryRequest, Transfer, UnfreezeAccountRequest};
use protos::action::ActionType;
use hex;
use secp256k1::{SecretKey, Secp256k1, Message};
use sha2::{Sha256, Digest};
use rand::Rng;

#[tokio::main]
async fn main() -> Result<()> {
    let cmd = Cmd::parse();
    cmd.exec().await
}

#[derive(Parser)]
#[command(author, version, about)]
enum Cmd {
    Create {
        name: String,
        balance: u64,
    },
    Get {
        id: String,
    },
    Transfer {
        from: String,
        to: String,
        amount: u64,
        private_key: String,
    },
    Freeze {
        id: String,
    },
    Unfreeze {
        id: String,
    },
    GetHistory {
        id: String,
        limit: Option<u64>
    }
}


impl Cmd {
    async fn exec(self) -> Result<()> {
        let mut client = LedgerClient::connect("http://localhost:50051").await?;
        match self {
            Cmd::Create { name, balance } => {
                let resp = client
                    .create_account(CreateAccountReq { name, balance })
                    .await?
                    .into_inner();
                println!("id: {}", hex::encode(&resp.account.as_ref().unwrap().id));
                println!("name: {}", resp.account.as_ref().unwrap().name);
                println!("balance: {}", resp.account.as_ref().unwrap().balance);
                println!("private_key: {}", hex::encode(resp.private_key));
            }
            Cmd::Get { id } => {
                let decoded_id = hex::decode(&id);
                match decoded_id {
                    Ok(decoded) => {
                        let resp = client
                            .get_account(GetAccountReq { id: decoded })
                            .await?
                            .into_inner();
                        display_account(&resp);
                    },
                    Err(_) => {
                        println!("Error: Invalid ID format. The ID should be a valid hexadecimal string.");
                    }
                }
            }
            Cmd::Transfer { from, to, amount, private_key } => {
                let from_decoded = match hex::decode(&from) {
                    Ok(decoded) => decoded,
                    Err(_) => {
                        println!("Error: Invalid 'from' address. Expected a valid hex string.");
                        return Ok(());
                    }
                };

                let to_decoded = match hex::decode(&to) {
                    Ok(decoded) => decoded,
                    Err(_) => {
                        println!("Error: Invalid 'to' address. Expected a valid hex string.");
                        return Ok(());
                    }
                };

                let mut rng = rand::thread_rng();
                let nonce_decoded: u64 = rng.gen();

                let mut message = Vec::new();
                message.extend_from_slice(&from_decoded);
                message.extend_from_slice(&to_decoded);
                message.extend_from_slice(&amount.to_le_bytes());
                message.extend_from_slice(&nonce_decoded.to_le_bytes());

                let secp = Secp256k1::new();
                let message_hash = Sha256::digest(&message);

                let secret_key = match hex::decode(&private_key) {
                    Ok(decoded) => SecretKey::from_slice(&decoded).unwrap_or_else(|e| {
                        println!("Error: Invalid private key: {}", e);
                        exit(1);
                    }),
                    Err(err) => {
                        println!("Error: Invalid private key format: {}", err);
                        exit(1);
                    }
                };

                let message_hash = Message::from_digest_slice(&message_hash)
                    .map_err(|e| {
                        println!("Error: Failed to create message hash: {}", e);
                    })
                    .unwrap();

                let signature = secp.sign_ecdsa(&message_hash, &secret_key);

                let resp = client
                    .create_transfer(Transfer {
                        from_account: from_decoded,
                        to_account: to_decoded,
                        amount,
                        signature: signature.serialize_compact().to_vec(),
                        nonce: nonce_decoded
                    })
                    .await?
                    .into_inner();

                println!("Transfer response: {:#?}", resp);
            }
            
            Cmd::Freeze { id } => {
                let decoded_id = hex::decode(&id);
                match decoded_id {
                    Ok(decoded) => {
                        let resp = client.freeze_account(FreezeAccountRequest { id: decoded }).await?.into_inner();
                        println!("{:#?}", resp);
                    },
                    Err(_) => {
                        println!("Error: Invalid ID format. The ID should be a valid hexadecimal string.");
                    }
                }
            }
            Cmd::Unfreeze { id } => {
                let decoded_id = hex::decode(&id);
                match decoded_id {
                    Ok(decoded) => {
                        let resp = client.unfreeze_account(UnfreezeAccountRequest { id: decoded }).await?.into_inner();
                        println!("{:#?}", resp);
                    },
                    Err(_) => {
                        println!("Error: Invalid ID format. The ID should be a valid hexadecimal string.");
                    }
                }
            }
            Cmd::GetHistory { id, limit } => {
                let decoded_id = hex::decode(&id);
                match decoded_id {
                    Ok(decoded) => {
                        let resp = client.get_history(GetHistoryRequest {
                            id: decoded,
                            limit: limit.unwrap_or(u64::MAX),
                        }).await?.into_inner();

                        if resp.actions.len() < 1 {
                            println!("Account not found");
                            return Ok(());
                        }
                        
                        for (index, i) in resp.actions.iter().enumerate() {
                            println!("--------------------------");
                            println!("Index: {}", index + 1);
                            display_action(i);
                            println!("--------------------------");
                        }
                    },
                    Err(_) => {
                        println!("Error: Invalid ID format. The ID should be a valid hexadecimal string.");
                    }
                }
            }
        }
        Ok(())
    }
}

fn action_from_u32(value: i32) -> Option<ActionType>{
    match value {
        0 => Some(ActionType::Transfer),
        1 => Some(ActionType::CreateAccount),
        2 => Some(ActionType::FreezeAccount),
        3 => Some(ActionType::UnfreezeAccount),
        _ => None
    }
}

fn display_action(action: &Action) {
    println!("Aciton type: {:?}", action_from_u32(action.r#type).unwrap());
    println!("Timestamp: {}", action.timestamp);
    println!("From id: {}", hex::encode(action.from.clone()));
    println!("To id: {}", hex::encode(action.to.clone()));
    println!("Amount: {}", action.sum);
}

fn display_account(account: &Account) {
    println!("Account {{");
    println!("    id: {}", hex::encode(&account.id));
    println!("    name: \"{}\"", account.name);
    println!("    balance: {}", account.balance);
    println!("    is_frozen: {}", account.is_frozen);
    println!("}}");
}

