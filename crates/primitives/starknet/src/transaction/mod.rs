//! Starknet transaction related functionality.
/// Types related to transactions.
pub mod types;

use alloc::vec;

use blockifier::block_context::BlockContext;
use blockifier::execution::contract_class::ContractClass;
use blockifier::execution::entry_point::CallInfo;
use blockifier::state::cached_state::CachedState;
use blockifier::state::state_api::StateReader;
use blockifier::transaction::errors::InvokeTransactionError;
use blockifier::transaction::objects::AccountTransactionContext;
use blockifier::transaction::transactions::Executable;
use frame_support::BoundedVec;
use sp_core::{H256, U256};
use starknet_api::api_core::{ContractAddress as StarknetContractAddress, EntryPointSelector, Nonce};
use starknet_api::hash::{StarkFelt, StarkHash};
use starknet_api::transaction::{
    ContractAddressSalt, DeclareTransaction, DeployAccountTransaction, EventContent, Fee, InvokeTransaction,
    L1HandlerTransaction, TransactionHash, TransactionSignature, TransactionVersion,
};
use starknet_api::StarknetApiError;

use self::types::{
    EventError, EventWrapper, MaxArraySize, Transaction, TransactionExecutionErrorWrapper,
    TransactionExecutionResultWrapper, TxType,
};
use crate::execution::{CallEntryPointWrapper, ContractAddressWrapper, ContractClassWrapper};
use crate::starknet_block::block::Block;
use crate::starknet_block::serialize::SerializeBlockContext;

impl EventWrapper {
    /// Creates a new instance of an event.
    ///
    /// # Arguments
    ///
    /// * `keys` - Event keys.
    /// * `data` - Event data.
    /// * `from_address` - Contract Address where the event was emitted from.
    pub fn new(
        keys: BoundedVec<H256, MaxArraySize>,
        data: BoundedVec<H256, MaxArraySize>,
        from_address: ContractAddressWrapper,
    ) -> Self {
        Self { keys, data, from_address }
    }

    /// Creates an empty event.
    pub fn empty() -> Self {
        Self {
            keys: BoundedVec::try_from(vec![]).unwrap(),
            data: BoundedVec::try_from(vec![]).unwrap(),
            from_address: ContractAddressWrapper::default(),
        }
    }

    /// Creates a new instance of an event builder.
    pub fn builder() -> EventBuilder {
        EventBuilder::default()
    }
}

/// Builder pattern for `EventWrapper`.
#[derive(Default)]
pub struct EventBuilder {
    keys: vec::Vec<H256>,
    data: vec::Vec<H256>,
    from_address: Option<StarknetContractAddress>,
}

impl EventBuilder {
    /// Sets the keys of the event.
    ///
    /// # Arguments
    ///
    /// * `keys` - Event keys.
    pub fn with_keys(mut self, keys: vec::Vec<H256>) -> Self {
        self.keys = keys;
        self
    }

    /// Sets the data of the event.
    ///
    /// # Arguments
    ///
    /// * `data` - Event data.
    pub fn with_data(mut self, data: vec::Vec<H256>) -> Self {
        self.data = data;
        self
    }

    /// Sets the from address of the event.
    ///
    /// # Arguments
    ///
    /// * `from_address` - Contract Address where the event was emitted from.
    pub fn with_from_address(mut self, from_address: StarknetContractAddress) -> Self {
        self.from_address = Some(from_address);
        self
    }

    /// Sets keys and data from an event content.
    ///
    /// # Arguments
    ///
    /// * `event_content` - Event content retrieved from the `CallInfo`.
    pub fn with_event_content(mut self, event_content: EventContent) -> Self {
        self.keys = event_content.keys.iter().map(|k| H256::from_slice(k.0.bytes())).collect::<vec::Vec<H256>>();
        self.data = event_content.data.0.iter().map(|d| H256::from_slice(d.bytes())).collect::<vec::Vec<H256>>();
        self
    }

    /// Builds the event.
    pub fn build(self) -> Result<EventWrapper, EventError> {
        Ok(EventWrapper {
            keys: BoundedVec::try_from(self.keys).map_err(|_| EventError::InvalidKeys)?,
            data: BoundedVec::try_from(self.data).map_err(|_| EventError::InvalidData)?,
            from_address: self
                .from_address
                .unwrap_or_default()
                .0
                .key()
                .bytes()
                .try_into()
                .map_err(|_| EventError::InvalidFromAddress)?,
        })
    }
}

impl Default for EventWrapper {
    fn default() -> Self {
        let one = H256::from_slice(&[
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ]);
        Self {
            keys: BoundedVec::try_from(vec![one, one]).unwrap(),
            data: BoundedVec::try_from(vec![one, one]).unwrap(),
            from_address: ContractAddressWrapper::from(one),
        }
    }
}

/// Try to convert a `&Transaction` into a `DeployAccountTransaction`.
impl TryInto<DeployAccountTransaction> for &Transaction {
    type Error = StarknetApiError;

    fn try_into(self) -> Result<DeployAccountTransaction, Self::Error> {
        Ok(DeployAccountTransaction {
            transaction_hash: TransactionHash(StarkFelt::new(self.hash.0)?),
            max_fee: Fee(2),
            version: TransactionVersion(StarkFelt::new(self.version.into())?),
            signature: TransactionSignature(
                self.signature.clone().into_inner().iter().map(|x| StarkFelt::new(x.0).unwrap()).collect(),
            ),
            nonce: Nonce(StarkFelt::new(self.nonce.into())?),
            contract_address: StarknetContractAddress::try_from(StarkFelt::new(self.sender_address)?)?,
            class_hash: self.call_entrypoint.to_starknet_call_entry_point().class_hash.unwrap_or_default(),
            constructor_calldata: self.call_entrypoint.to_starknet_call_entry_point().calldata,
            // TODO: add salt
            contract_address_salt: ContractAddressSalt(StarkFelt::new([0; 32])?),
        })
    }
}

/// Try to convert a `&Transaction` into a `L1HandlerTransaction`.
impl TryInto<L1HandlerTransaction> for &Transaction {
    type Error = StarknetApiError;

    fn try_into(self) -> Result<L1HandlerTransaction, Self::Error> {
        Ok(L1HandlerTransaction {
            transaction_hash: TransactionHash(StarkFelt::new(self.hash.0)?),
            version: TransactionVersion(StarkFelt::new(self.version.into())?),
            nonce: Nonce(StarkFelt::new(self.nonce.into())?),
            contract_address: StarknetContractAddress::try_from(StarkFelt::new(self.sender_address)?)?,
            calldata: self.call_entrypoint.to_starknet_call_entry_point().calldata,
            entry_point_selector: EntryPointSelector(StarkHash::new(
                *self.call_entrypoint.entrypoint_selector.unwrap_or_default().as_fixed_bytes(),
            )?),
        })
    }
}

/// Try to convert a `&Transaction` into a `InvokeTransaction`.
impl TryInto<InvokeTransaction> for &Transaction {
    type Error = StarknetApiError;

    fn try_into(self) -> Result<InvokeTransaction, Self::Error> {
        Ok(InvokeTransaction {
            transaction_hash: TransactionHash(StarkFelt::new(self.hash.0)?),
            max_fee: Fee(2),
            version: TransactionVersion(StarkFelt::new(self.version.into())?),
            signature: TransactionSignature(
                self.signature.clone().into_inner().iter().map(|x| StarkFelt::new(x.0).unwrap()).collect(),
            ),
            nonce: Nonce(StarkFelt::new(self.nonce.into())?),
            sender_address: StarknetContractAddress::try_from(StarkFelt::new(self.sender_address)?)?,
            calldata: self.call_entrypoint.to_starknet_call_entry_point().calldata,
            entry_point_selector: None,
        })
    }
}

/// Try to convert a `&Transaction` into a `DeclareTransaction`.
impl TryInto<DeclareTransaction> for &Transaction {
    type Error = StarknetApiError;

    fn try_into(self) -> Result<DeclareTransaction, Self::Error> {
        Ok(DeclareTransaction {
            transaction_hash: TransactionHash(StarkFelt::new(self.hash.0)?),
            max_fee: Fee(2),
            version: TransactionVersion(StarkFelt::new(self.version.into())?),
            signature: TransactionSignature(
                self.signature.clone().into_inner().iter().map(|x| StarkFelt::new(x.0).unwrap()).collect(),
            ),
            nonce: Nonce(StarkFelt::new(self.nonce.into())?),
            sender_address: StarknetContractAddress::try_from(StarkFelt::new(self.sender_address)?)?,
            class_hash: self.call_entrypoint.to_starknet_call_entry_point().class_hash.unwrap_or_default(),
        })
    }
}

impl Transaction {
    /// Creates a new instance of a transaction.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        version: U256,
        hash: H256,
        signature: BoundedVec<H256, MaxArraySize>,
        events: BoundedVec<EventWrapper, MaxArraySize>,
        sender_address: ContractAddressWrapper,
        nonce: U256,
        call_entrypoint: CallEntryPointWrapper,
        contract_class: Option<ContractClassWrapper>,
    ) -> Self {
        Self { version, hash, signature, events, sender_address, nonce, call_entrypoint, contract_class }
    }

    /// Creates a new instance of a transaction without signature.
    pub fn from_tx_hash(hash: H256) -> Self {
        Self { hash, ..Self::default() }
    }

    /// Executes a transaction
    ///
    /// # Arguments
    ///
    /// * `self` - The transaction to execute
    /// * `state` - The state to execute the transaction on
    /// * `block` - The block to execute the transaction on
    ///
    /// # Returns
    ///
    /// * `TransactionExecutionResult<TransactionExecutionInfo>` - The result of the transaction
    ///   execution
    pub fn execute<S: StateReader>(
        &self,
        state: &mut CachedState<S>,
        block: Block,
        tx_type: TxType,
        contract_class: Option<ContractClass>,
    ) -> TransactionExecutionResultWrapper<Option<CallInfo>> {
        let block_context = BlockContext::serialize(block.header);
        match tx_type {
            TxType::InvokeTx => {
                let tx = self.try_into().map_err(TransactionExecutionErrorWrapper::StarknetApi)?;
                let account_context = self.get_invoke_transaction_context(&tx);
                // Specifying an entry point selector is not allowed; `__execute__` is called, and
                // the inner selector appears in the calldata.
                if tx.entry_point_selector.is_some() {
                    return Err(TransactionExecutionErrorWrapper::TransactionExecution(
                        InvokeTransactionError::SpecifiedEntryPoint.into(),
                    ))?;
                }

                tx.run_execute(state, &block_context, &account_context, contract_class)
                    .map_err(TransactionExecutionErrorWrapper::TransactionExecution)
            }
            TxType::L1HandlerTx => {
                let tx = self.try_into().map_err(TransactionExecutionErrorWrapper::StarknetApi)?;
                let account_context = self.get_l1_handler_transaction_context(&tx);
                tx.run_execute(state, &block_context, &account_context, contract_class)
                    .map_err(TransactionExecutionErrorWrapper::TransactionExecution)
            }
            TxType::DeclareTx => {
                let tx = self.try_into().map_err(TransactionExecutionErrorWrapper::StarknetApi)?;
                let account_context = self.get_declare_transaction_context(&tx);
                // Execute.
                tx.run_execute(state, &block_context, &account_context, contract_class)
                    .map_err(TransactionExecutionErrorWrapper::TransactionExecution)
            }
            TxType::DeployAccountTx => {
                let tx = self.try_into().map_err(TransactionExecutionErrorWrapper::StarknetApi)?;
                let account_context = self.get_deploy_account_transaction_context(&tx);

                // Execute.
                tx.run_execute(state, &block_context, &account_context, contract_class)
                    .map_err(TransactionExecutionErrorWrapper::TransactionExecution)
            }
        }

        // TODO: Investigate the use of tx.execute() instead of tx.run_execute()
        // Going one lower level gives us more flexibility like not validating the tx as we could do
        // it before the tx lands in the mempool.
        // However it also means we need to copy/paste internal code from the tx.execute() method.
    }

    /// Get the transaction context for a l1 handler transaction
    ///
    /// # Arguments
    ///
    /// * `self` - The transaction to get the context for
    /// * `tx` - The l1 handler transaction to get the context for
    ///
    /// # Returns
    ///
    /// * `AccountTransactionContext` - The context of the transaction
    fn get_l1_handler_transaction_context(&self, tx: &L1HandlerTransaction) -> AccountTransactionContext {
        AccountTransactionContext {
            transaction_hash: tx.transaction_hash,
            max_fee: Fee::default(),
            version: tx.version,
            signature: TransactionSignature::default(),
            nonce: tx.nonce,
            sender_address: tx.contract_address,
        }
    }

    /// Get the transaction context for an invoke transaction
    ///
    /// # Arguments
    ///
    /// * `self` - The transaction to get the context for
    /// * `tx` - The invoke transaction to get the context for
    ///
    /// # Returns
    ///
    /// * `AccountTransactionContext` - The context of the transaction
    fn get_invoke_transaction_context(&self, tx: &InvokeTransaction) -> AccountTransactionContext {
        AccountTransactionContext {
            transaction_hash: tx.transaction_hash,
            max_fee: tx.max_fee,
            version: tx.version,
            signature: tx.signature.clone(),
            nonce: tx.nonce,
            sender_address: tx.sender_address,
        }
    }

    /// Get the transaction context for a deploy account transaction
    ///
    /// # Arguments
    ///
    /// * `self` - The transaction to get the context for
    /// * `tx` - The deploy transaction to get the context for
    ///
    /// # Returns
    ///
    /// * `AccountTransactionContext` - The context of the transaction
    fn get_deploy_account_transaction_context(&self, tx: &DeployAccountTransaction) -> AccountTransactionContext {
        AccountTransactionContext {
            transaction_hash: tx.transaction_hash,
            max_fee: tx.max_fee,
            version: tx.version,
            signature: tx.signature.clone(),
            nonce: tx.nonce,
            sender_address: tx.contract_address,
        }
    }

    /// Get the transaction context for a declare transaction
    ///
    /// # Arguments
    ///
    /// * `self` - The transaction to get the context for
    /// * `tx` - The declare transaction to get the context for
    ///
    /// # Returns
    ///
    /// * `AccountTransactionContext` - The context of the transaction
    fn get_declare_transaction_context(&self, tx: &DeclareTransaction) -> AccountTransactionContext {
        AccountTransactionContext {
            transaction_hash: tx.transaction_hash,
            max_fee: tx.max_fee,
            version: tx.version,
            signature: tx.signature.clone(),
            nonce: tx.nonce,
            sender_address: tx.sender_address,
        }
    }
}

impl Default for Transaction {
    fn default() -> Self {
        let one = H256::from_slice(&[
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ]);
        Self {
            version: U256::default(),
            hash: one,
            signature: BoundedVec::try_from(vec![one, one]).unwrap(),
            events: BoundedVec::try_from(vec![EventWrapper::default(), EventWrapper::default()]).unwrap(),
            nonce: U256::default(),
            sender_address: ContractAddressWrapper::default(),
            call_entrypoint: CallEntryPointWrapper::default(),
            contract_class: None,
        }
    }
}
