use blockifier::execution::contract_class::ContractClass;
use blockifier::state::cached_state::{ContractClassMapping, ContractStorageKey};
use blockifier::state::errors::StateError;
use blockifier::state::state_api::{StateReader, StateResult};
use starknet_api::api_core::{ClassHash, ContractAddress, Nonce};
use starknet_api::hash::StarkFelt;
use starknet_api::state::StorageKey;
use starknet_api::stdlib::collections::HashMap;

/// A simple implementation of `StateReader` using `HashMap`s as storage.
#[derive(Debug, Default)]
pub struct DictStateReader {
    /// The storage layout.
    pub storage_view: HashMap<ContractStorageKey, StarkFelt>,
    /// The nonce of each contract.
    pub address_to_nonce: HashMap<ContractAddress, Nonce>,
    /// The class hash of each contract.
    pub address_to_class_hash: HashMap<ContractAddress, ClassHash>,
    /// The class of each class hash.
    pub class_hash_to_class: ContractClassMapping,
}

impl StateReader for DictStateReader {
    fn get_storage_at(&mut self, contract_address: ContractAddress, key: StorageKey) -> StateResult<StarkFelt> {
        let contract_storage_key = (contract_address, key);
        let value = self.storage_view.get(&contract_storage_key).copied().unwrap_or_default();
        Ok(value)
    }

    fn get_nonce_at(&mut self, contract_address: ContractAddress) -> StateResult<Nonce> {
        let nonce = self.address_to_nonce.get(&contract_address).copied().unwrap_or_default();
        Ok(nonce)
    }

    fn get_contract_class(&mut self, class_hash: &ClassHash) -> StateResult<ContractClass> {
        let contract_class = self.class_hash_to_class.get(class_hash).cloned();
        match contract_class {
            Some(contract_class) => Ok(contract_class),
            None => Err(StateError::UndeclaredClassHash(*class_hash)),
        }
    }

    fn get_class_hash_at(&mut self, contract_address: ContractAddress) -> StateResult<ClassHash> {
        let class_hash = self.address_to_class_hash.get(&contract_address).copied().unwrap_or_default();
        Ok(class_hash)
    }
}
