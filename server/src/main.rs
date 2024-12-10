use std::{collections::HashMap, sync::Arc};
use async_trait::async_trait;
use protos::{
    ledger_server::LedgerServer, Account, CreateAccountReq, CreateAccountResponse, GetAccountReq,
    Transfer, TransferResult, TransferError, transfer_error, GetHistoryRequest, GetHistoryResponse,
    FreezeAccountRequest, FreezeAccountResponse, UnfreezeAccountRequest, UnfreezeAccountResponse
};
use tokio::sync::Mutex;
use tonic::{transport::Server, Status, Request, Response};
use rand::rngs::OsRng;
use secp256k1::{Secp256k1, PublicKey, Message, ecdsa::Signature};
use sha2::{Sha256, Digest};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let server = tokio::spawn(
        Server::builder()
            .add_service(LedgerServer::new(Ledger::default()))
            .serve("[::]:50051".parse()?),
    );
    println!("Listening on [::]:50051");
    server.await??;
    Ok(())
}

#[derive(Default, Clone)]
struct Ledger {
    accounts: Arc<Mutex<HashMap<Vec<u8>, Account>>>,
}

#[async_trait]
impl protos::ledger_server::Ledger for Ledger {
    async fn freeze_account(
        &self,
        request: Request<FreezeAccountRequest>,
    ) -> Result<Response<FreezeAccountResponse>, Status> {
        let request = request.into_inner();
        let id = request.id;
        let mut accounts = self.accounts.lock().await;
        let account = accounts.get_mut(&id).ok_or_else(|| Status::not_found("Account not found"))?;
        if account.is_frozen {
            return Ok(tonic::Response::new(protos::FreezeAccountResponse {
                success: false,
                message: "Account is already frozen".to_string(),
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
        request: Request<UnfreezeAccountRequest>,
    ) -> Result<Response<UnfreezeAccountResponse>, Status> {
        let request = request.into_inner();
        let id = request.id;
        let mut accounts = self.accounts.lock().await;
        let account = accounts.get_mut(&id).ok_or_else(|| Status::not_found("Account not found"))?;
        if !account.is_frozen {
            return Ok(tonic::Response::new(protos::UnfreezeAccountResponse {
                success: false,
                message: "Account is not frozen".to_string(),
            }));
        }
        account.is_frozen = false;
        Ok(tonic::Response::new(protos::UnfreezeAccountResponse {
            success: true,
            message: "Account has been unfrozen".to_string(),
        }))
    }

    async fn create_account(
        &self,
        request: Request<CreateAccountReq>,
    ) -> Result<Response<CreateAccountResponse>, Status> {
        let req = request.into_inner();

        let secp = Secp256k1::new();
        let (secret_key, public_key) = secp.generate_keypair(&mut OsRng);

        let account = Account {
            id: public_key.serialize().to_vec(),
            name: req.name,
            balance: req.balance,
            is_frozen: false,
        };

        let mut accounts = self.accounts.lock().await;
        accounts.insert(
            account.id.clone(), 
            account.clone()
        );

        Ok(Response::new(CreateAccountResponse {
            account: Some(account),
            private_key: secret_key.secret_bytes().to_vec(),
        }))
    }

    async fn get_account(
        &self,
        request: Request<GetAccountReq>,
    ) -> Result<Response<Account>, Status> {
        let req = request.into_inner();
        let accounts = self.accounts.lock().await;

        accounts
            .get(&req.id)
            .cloned()
            .ok_or_else(|| Status::not_found("Account not found"))
            .map(Response::new)
    }

    async fn create_transfer(
        &self,
        request: Request<Transfer>,
    ) -> Result<Response<TransferResult>, Status> {
        let transfer = request.into_inner();

        let message_hash = self.create_transfer_message_hash(&transfer)?;
        let from_account = self.get_account(Request::new(GetAccountReq { id: transfer.from_account.clone() })).await?.into_inner();
        let to_account = self.get_account(Request::new(GetAccountReq { id: transfer.to_account.clone() })).await?.into_inner();

        match self.verify_transfer_conditions(&transfer, &from_account, &to_account, &message_hash) {
            Ok(_) => {},
            Err(err) => return Ok(Response::new(TransferResult{
                error: Some(err)
            }))
        };

        let accounts = self.accounts.lock().await;

        match self.update_balances(&transfer, accounts, &from_account, &to_account) {
            Ok(_) => {},
            Err(err) => return Ok(Response::new(TransferResult {
                error: Some(err)
            }))
        }

        Ok(Response::new(TransferResult {
            error: None,
        }))
    }

    async fn get_history(
        &self, 
        request: Request<GetHistoryRequest>
    ) -> Result<Response<GetHistoryResponse>, Status> {
        Ok(Response::new(GetHistoryResponse {
            actions: vec![],
        }))
    }
}

impl Ledger {
    fn create_transfer_message_hash(&self, transfer: &Transfer) -> Result<[u8; 32], Status> {
        let mut message = Vec::new();
        message.extend_from_slice(&transfer.from_account);
        message.extend_from_slice(&transfer.to_account);
        message.extend_from_slice(&transfer.amount.to_le_bytes());
        Ok(Sha256::digest(&message).into())
    }

    fn verify_transfer_conditions(
        &self,
        transfer: &Transfer,
        from_account: &Account,
        to_account: &Account,
        message_hash: &[u8; 32],
    ) -> Result<(), TransferError> {
        if from_account.is_frozen {
            return Err(TransferError {
                code: transfer_error::Code::FrozenAccount.into(),
                message: "From account is frozen".to_string(),
            });
        }
    
        if to_account.is_frozen {
            return Err(TransferError {
                code: transfer_error::Code::FrozenAccount.into(),
                message: "To account is frozen".to_string(),
            });
        }
    
        let public_key = PublicKey::from_slice(&from_account.id)
            .map_err(|_| TransferError {
                code: transfer_error::Code::InvalidSignature.into(),
                message: "Invalid public key".to_string(),
            })?;
    
        let secp = Secp256k1::verification_only();
        let secp_message = Message::from_slice(message_hash)
            .map_err(|_| TransferError {
                code: transfer_error::Code::InvalidSignature.into(),
                message: "Invalid message".to_string(),
            })?;
    
        let signature = Signature::from_compact(&transfer.signature)
            .map_err(|_| TransferError {
                code: transfer_error::Code::InvalidSignature.into(),
                message: "Invalid signature format".to_string(),
            })?;
    
        secp.verify_ecdsa(&secp_message, &signature, &public_key)
            .map_err(|_| TransferError {
                code: transfer_error::Code::InvalidSignature.into(),
                message: "Invalid signature".to_string(),
            })?;
    
        Ok(())
    }

    fn update_balances(
        &self,
        transfer: &Transfer,
        mut accounts: tokio::sync::MutexGuard<HashMap<Vec<u8>, Account>>,
        from_account: &Account,
        to_account: &Account,
    ) -> Result<(), TransferError> {
        let new_from_balance = from_account.balance.checked_sub(transfer.amount).ok_or_else(|| {
            TransferError {
                code: transfer_error::Code::InsufficientBalance.into(),
                message: "Insufficient balance".to_string(),
            }
        })?;
        let new_to_balance = to_account.balance.checked_add(transfer.amount).ok_or_else(|| {
            TransferError{
                code: transfer_error::Code::BalanceOverflow.into(),
                message: "Balance overflow".to_string()
            }
        })?;

        let from_account = accounts
            .get_mut(&transfer.from_account)
            .ok_or_else(|| {
                TransferError {
                    code: transfer_error::Code::AccountNotFound.into(),
                    message: "From account not found".to_string(),
                }
            })?;
        from_account.balance = new_from_balance;

        let to_account = accounts
            .get_mut(&transfer.to_account)
            .ok_or_else(|| {
                TransferError {
                    code: transfer_error::Code::AccountNotFound.into(),
                    message: "To account not found".to_string(),
                }
            })?;
        to_account.balance = new_to_balance;

        Ok(())
    }
}
