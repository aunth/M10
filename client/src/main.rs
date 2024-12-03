use anyhow::Result;
use clap::Parser;
use protos::{ledger_client::LedgerClient, CreateAccountReq, GetAccountReq, Transfer, FreezeAccountRequest, UnfreezeAccountRequest, Account};
use hex;

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
        from: uuid::Uuid,
        to: uuid::Uuid,
        amount: u64,
    },
    Freeze {
        id: String,
    },
    Unfreeze {
        id: String,
    },
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
                let resp = client
                    .get_account(GetAccountReq {
                        id: hex::decode(&id).unwrap(),
                    })
                    .await?
                    .into_inner();
                display_account(&resp);
            }
            Cmd::Transfer { from, to, amount } => {
                let resp = client
                    .create_transfer(Transfer {
                        from_account: from.as_bytes().to_vec(),
                        to_account: to.as_bytes().to_vec(),
                        amount,
                        signature: vec![],
                    })
                    .await?
                    .into_inner();
                println!("{:#?}", resp);
            }
            Cmd::Freeze { id } => {
                let resp = client.freeze_account(FreezeAccountRequest { id: hex::decode(id).unwrap() }).await?.into_inner();
                println!("{:#?}", resp);
            }
            Cmd::Unfreeze { id } => {
                let resp = client.unfreeze_account(UnfreezeAccountRequest { id: hex::decode(id).unwrap() }).await?.into_inner();
                println!("{:#?}", resp);
            }
        }
        Ok(())
    }
}

fn display_account(account: &Account) {
    println!("Account {{");
    println!("    id: {}", hex::encode(&account.id));
    println!("    name: \"{}\"", account.name);
    println!("    balance: {}", account.balance);
    println!("    is_frozen: {}", account.is_frozen);
    println!("}}");
}