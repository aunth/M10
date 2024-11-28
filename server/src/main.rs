use std::{collections::HashMap, sync::Arc};
use async_trait::async_trait;
use protos::{ledger_server::LedgerServer, transfer_error, Account, TransferError, TransferResult};
use tokio::sync::Mutex;
use tonic::{transport::Server, Status};
use uuid::Uuid;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let server = tokio::spawn(
        Server::builder()
            .add_service(LedgerServer::new(Ledger::default()))
            .serve("[::]:50051".parse()?),
    );
    println!("listening on [::]:50051");
    server.await??;
    Ok(())
}

#[derive(Default, Clone)]
struct Ledger {
    accounts: Arc<Mutex<HashMap<uuid::Uuid, Account>>>,
}

#[async_trait]
impl protos::ledger_server::Ledger for Ledger {
    async fn freeze_account(
        &self,
        request: tonic::Request<protos::FreezeAccountRequest>,
    ) -> Result<tonic::Response<protos::FreezeAccountResponse>, tonic::Status> {
        let request = request.into_inner();
        let id = Uuid::from_slice(&request.id)
            .map_err(|err| tonic::Status::invalid_argument(format!("{}", err)))?;
        let mut accounts = self.accounts.lock().await;
        let account = accounts.get_mut(&id).ok_or_else(|| Status::not_found("account not found"))?;
        if account.is_frozen {
            return Ok(tonic::Response::new(protos::FreezeAccountResponse {
                success: false,
                message: "account is already frozen".to_string(),
            }));
        }
        account.is_frozen = true;
        Ok(tonic::Response::new(protos::FreezeAccountResponse {
            success: true,
            message: "Account has been frozen".to_string(),
        }))
    }

    async fn unfreeze_account(
        &self,
        request: tonic::Request<protos::UnfreezeAccountRequest>,
    ) -> Result<tonic::Response<protos::UnfreezeAccountResponse>, tonic::Status> {
        let request = request.into_inner();
        let id = Uuid::from_slice(&request.id)
            .map_err(|err| tonic::Status::invalid_argument(format!("{}", err)))?;
        let mut accounts = self.accounts.lock().await;
        let account = accounts.get_mut(&id).ok_or_else(|| Status::not_found("account not found"))?;
        if !account.is_frozen {
            return Ok(tonic::Response::new(protos::UnfreezeAccountResponse {
                success: false,
                message: "account is not frozen".to_string(),
            }));
        }
        account.is_frozen = false;
        Ok(tonic::Response::new(protos::UnfreezeAccountResponse {
            success: true,
            message: "account has been unfrozen".to_string(),
        }))
    }

    async fn create_account(
        &self,
        request: tonic::Request<protos::CreateAccountReq>,
    ) -> Result<tonic::Response<protos::Account>, tonic::Status> {
        let request = request.into_inner();
        let id = Uuid::new_v4();
        let mut accounts = self.accounts.lock().await;
        let account = Account {
            name: request.name,
            balance: request.balance,
            id: id.as_bytes().to_vec(),
            is_frozen: false,
        };
        accounts.insert(id, account.clone());
        Ok(tonic::Response::new(account))
    }

    async fn get_account(
        &self,
        request: tonic::Request<protos::GetAccountReq>,
    ) -> Result<tonic::Response<protos::Account>, tonic::Status> {
        let request = request.into_inner();
        let id = Uuid::from_slice(&request.id)
            .map_err(|err| tonic::Status::invalid_argument(format!("{}", err)))?;
        let accounts = self.accounts.lock().await;
        let account = accounts
            .get(&id)
            .ok_or_else(|| Status::not_found("account not found"))?
            .clone();
        Ok(tonic::Response::new(account))
    }

    async fn create_transfer(
        &self,
        request: tonic::Request<protos::Transfer>,
    ) -> Result<tonic::Response<protos::TransferResult>, tonic::Status> {
        let request = request.into_inner();
        let from_id = Uuid::from_slice(&request.from_account)
            .map_err(|err| tonic::Status::invalid_argument(format!("{}", err)))?;
        let to_id = Uuid::from_slice(&request.to_account)
            .map_err(|err| tonic::Status::invalid_argument(format!("{}", err)))?;

        let mut accounts = self.accounts.lock().await;
        let from_account = accounts
            .get(&from_id)
            .ok_or_else(|| Status::not_found("account not found"))?;

        let to_account = accounts
            .get(&to_id)
            .ok_or_else(|| Status::not_found("account not found"))?;

        if from_account.is_frozen {
            return Ok(tonic::Response::new(TransferResult {
                error: Some(TransferError {
                    code: transfer_error::Code::FrozenAccount.into(),
                    message: "Source account is frozen".to_string(),
                }),
            }));
        }

        if to_account.is_frozen {
            return Ok(tonic::Response::new(TransferResult {
                error: Some(TransferError {
                    code: transfer_error::Code::FrozenAccount.into(),
                    message: "Target account is frozen".to_string(),
                }),
            }));
        }

        let Some(new_from_balance) = from_account.balance.checked_sub(request.amount) else {
            return Ok(tonic::Response::new(TransferResult {
                error: Some(TransferError {
                    code: transfer_error::Code::InsufficientBalance.into(),
                    message: "insufficent balance".to_string(),
                }),
            }));
        };
        let Some(new_to_balance) = to_account.balance.checked_add(request.amount) else {
            return Ok(tonic::Response::new(TransferResult {
                error: Some(TransferError {
                    code: transfer_error::Code::Unknown.into(),
                    message: "balance overflow".to_string(),
                }),
            }));

        };
        let from_account = accounts
            .get_mut(&from_id)
            .ok_or_else(|| Status::not_found("account not found"))?;
        from_account.balance = new_from_balance;

        let to_account = accounts
            .get_mut(&to_id)
            .ok_or_else(|| Status::not_found("account not found"))?;
        to_account.balance = new_to_balance;
        Ok(tonic::Response::new(TransferResult { error: None }))
    }
}
