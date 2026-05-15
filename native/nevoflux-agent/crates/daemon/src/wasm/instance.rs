//! Wasm instance management module.
//!
//! This module provides the `WasmInstance` struct for creating and managing
//! WebAssembly module instances.

use wasmtime::{Instance, Store};

use super::linker::{create_linker, HostState};
use super::runtime::WasmRuntime;
use crate::error::{DaemonError, Result};

/// A WebAssembly instance with its associated store.
///
/// `WasmInstance` wraps a Wasmtime `Instance` and `Store`, providing
/// a high-level interface for interacting with instantiated Wasm modules.
pub struct WasmInstance {
    /// The Wasmtime store containing the instance state.
    store: Store<HostState>,

    /// The instantiated Wasm module.
    instance: Instance,
}

impl std::fmt::Debug for WasmInstance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmInstance").finish_non_exhaustive()
    }
}

impl WasmInstance {
    /// Create a new WasmInstance from a WasmRuntime.
    ///
    /// This creates a new store with default HostState, creates a linker with
    /// host functions, and instantiates the module.
    ///
    /// # Arguments
    ///
    /// * `runtime` - The WasmRuntime containing the compiled module.
    ///
    /// # Errors
    ///
    /// Returns an error if the linker creation or module instantiation fails.
    pub fn new(runtime: &WasmRuntime) -> Result<Self> {
        let engine = runtime.engine();
        let module = runtime.module();

        // Create the linker with host functions
        let linker = create_linker(engine)?;

        // Create a store with default host state
        let mut store = Store::new(engine, HostState::new());

        // Instantiate the module
        let instance = linker.instantiate(&mut store, module).map_err(|e| {
            DaemonError::InternalError(format!("Failed to instantiate Wasm module: {}", e))
        })?;

        Ok(Self { store, instance })
    }

    /// Get the ABI version from the Wasm module.
    ///
    /// This calls the `get_abi_version` export function which should return
    /// a u32 representing the ABI version of the module.
    ///
    /// # Errors
    ///
    /// Returns an error if the export doesn't exist or the call fails.
    pub fn get_abi_version(&mut self) -> Result<u32> {
        let func = self
            .instance
            .get_typed_func::<(), i32>(&mut self.store, "get_abi_version")
            .map_err(|e| {
                DaemonError::InternalError(format!("Failed to get get_abi_version export: {}", e))
            })?;

        let version = func.call(&mut self.store, ()).map_err(|e| {
            DaemonError::InternalError(format!("Failed to call get_abi_version: {}", e))
        })?;

        Ok(version as u32)
    }

    /// Check if the module has a specific export.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the export to check for.
    ///
    /// # Returns
    ///
    /// `true` if the export exists, `false` otherwise.
    pub fn has_export(&mut self, name: &str) -> bool {
        self.instance.get_export(&mut self.store, name).is_some()
    }

    /// Get a reference to the store.
    pub fn store(&self) -> &Store<HostState> {
        &self.store
    }

    /// Get a mutable reference to the store.
    pub fn store_mut(&mut self) -> &mut Store<HostState> {
        &mut self.store
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Create a test Wasm module with basic exports.
    fn create_test_module() -> Vec<u8> {
        wat::parse_str(
            r#"
            (module
                (func (export "get_abi_version") (result i32) i32.const 1)
                (func (export "get_version_len") (result i32) i32.const 5)
                (memory (export "memory") 1)
            )
        "#,
        )
        .unwrap()
    }

    #[test]
    fn test_instance_creation() {
        let wasm_bytes = create_test_module();
        let runtime = WasmRuntime::from_bytes(&wasm_bytes).expect("Failed to create runtime");

        let instance = WasmInstance::new(&runtime);
        assert!(
            instance.is_ok(),
            "Failed to create instance: {:?}",
            instance.err()
        );
    }

    #[test]
    fn test_get_abi_version() {
        let wasm_bytes = create_test_module();
        let runtime = WasmRuntime::from_bytes(&wasm_bytes).expect("Failed to create runtime");
        let mut instance = WasmInstance::new(&runtime).expect("Failed to create instance");

        let version = instance
            .get_abi_version()
            .expect("Failed to get ABI version");
        assert_eq!(version, 1);
    }

    #[test]
    fn test_has_export() {
        let wasm_bytes = create_test_module();
        let runtime = WasmRuntime::from_bytes(&wasm_bytes).expect("Failed to create runtime");
        let mut instance = WasmInstance::new(&runtime).expect("Failed to create instance");

        // Check for existing exports
        assert!(instance.has_export("get_abi_version"));
        assert!(instance.has_export("get_version_len"));
        assert!(instance.has_export("memory"));

        // Check for non-existing export
        assert!(!instance.has_export("nonexistent_export"));
    }
}
