use anyhow::Result;
use clap::Parser;
use protos::{ledger_client::LedgerClient, CreateAccountReq, GetAccountReq, Transfer, FreezeAccountRequest, UnfreezeAccountRequest};
use uuid::Uuid;

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
        id: uuid::Uuid,
    },
    Transfer {
        from: uuid::Uuid,
        to: uuid::Uuid,
        amount: u64,
    },
    Freeze {
        id: uuid::Uuid,
    },
    Unfreeze {
        id: uuid::Uuid,
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
                println!("id: {}", Uuid::from_slice(&resp.id)?);
                println!("name: {}", resp.name);
                println!("balance: {}", resp.balance);
            }
            Cmd::Get { id } => {
                let resp = client
                    .get_account(GetAccountReq {
                        id: id.as_bytes().to_vec(),
                    })
                    .await?
                    .into_inner();
                println!("{:#?}", resp);
            }
            Cmd::Transfer { from, to, amount } => {
                let resp = client
                    .create_transfer(Transfer {
                        from_account: from.as_bytes().to_vec(),
                        to_account: to.as_bytes().to_vec(),
                        amount,
                    })
                    .await?
                    .into_inner();
                println!("{:#?}", resp);
            }
            Cmd::Freeze { id } => {
                let resp = client.freeze_account(FreezeAccountRequest { id: id.as_bytes().to_vec() }).await?.into_inner();
                println!("{:#?}", resp);
            }
            Cmd::Unfreeze { id } => {
                let resp = client.unfreeze_account(UnfreezeAccountRequest { id: id.as_bytes().to_vec() }).await?.into_inner();
                println!("{:#?}", resp);
            }
        }
        Ok(())
    }
}