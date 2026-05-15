//! Integration tests for Wasm runtime.
//!
//! These tests verify the WebAssembly runtime functionality including
//! module loading, instance creation, and function execution.

use nevoflux_daemon::{WasmConfig, WasmInstance, WasmRuntime};

/// Create a test Wasm module with basic exports.
fn create_test_wasm() -> Vec<u8> {
    wat::parse_str(
        r#"
        (module
            (func (export "get_abi_version") (result i32) i32.const 1)
            (func (export "get_version_len") (result i32) i32.const 5)
            (memory (export "memory") 1)
        )
    "#,
    )
    .expect("Failed to parse WAT")
}

/// Create a minimal Wasm module (empty module).
fn create_minimal_wasm() -> Vec<u8> {
    wat::parse_str("(module)").expect("Failed to parse minimal WAT")
}

/// Create a Wasm module with a specific ABI version.
fn create_wasm_with_abi_version(version: i32) -> Vec<u8> {
    wat::parse_str(format!(
        r#"
        (module
            (func (export "get_abi_version") (result i32) i32.const {version})
            (memory (export "memory") 1)
        )
    "#
    ))
    .expect("Failed to parse WAT with ABI version")
}

#[test]
fn test_wasm_runtime_creation() {
    // Test that we can create a WasmRuntime from bytes
    let wasm_bytes = create_test_wasm();
    let runtime = WasmRuntime::from_bytes(&wasm_bytes);

    assert!(
        runtime.is_ok(),
        "Failed to create WasmRuntime: {:?}",
        runtime.err()
    );

    let runtime = runtime.unwrap();
    // Verify the engine is accessible
    let _engine = runtime.engine();
    // Verify the module is accessible
    let _module = runtime.module();
}

#[test]
fn test_wasm_runtime_from_minimal_module() {
    // Test loading a minimal (empty) Wasm module
    let wasm_bytes = create_minimal_wasm();
    let runtime = WasmRuntime::from_bytes(&wasm_bytes);

    assert!(
        runtime.is_ok(),
        "Failed to create WasmRuntime from minimal module"
    );
}

#[test]
fn test_wasm_runtime_invalid_bytes() {
    // Test that invalid bytes produce an error
    let invalid_bytes = b"not valid wasm binary";
    let result = WasmRuntime::from_bytes(invalid_bytes);

    assert!(result.is_err(), "Expected error for invalid Wasm bytes");
}

#[test]
fn test_wasm_instance_creation() {
    // Test creating a WasmInstance from a WasmRuntime
    let wasm_bytes = create_test_wasm();
    let runtime = WasmRuntime::from_bytes(&wasm_bytes).expect("Failed to create runtime");

    let instance = WasmInstance::new(&runtime);

    assert!(
        instance.is_ok(),
        "Failed to create WasmInstance: {:?}",
        instance.err()
    );
}

#[test]
fn test_wasm_abi_version() {
    // Test calling the get_abi_version export
    let wasm_bytes = create_test_wasm();
    let runtime = WasmRuntime::from_bytes(&wasm_bytes).expect("Failed to create runtime");
    let mut instance = WasmInstance::new(&runtime).expect("Failed to create instance");

    let version = instance.get_abi_version();

    assert!(
        version.is_ok(),
        "Failed to get ABI version: {:?}",
        version.err()
    );
    assert_eq!(version.unwrap(), 1, "Expected ABI version 1");
}

#[test]
fn test_wasm_abi_version_different_values() {
    // Test modules with different ABI versions
    for expected_version in [1, 2, 42, 255] {
        let wasm_bytes = create_wasm_with_abi_version(expected_version);
        let runtime = WasmRuntime::from_bytes(&wasm_bytes).expect("Failed to create runtime");
        let mut instance = WasmInstance::new(&runtime).expect("Failed to create instance");

        let version = instance
            .get_abi_version()
            .expect("Failed to get ABI version");
        assert_eq!(
            version, expected_version as u32,
            "Expected ABI version {expected_version}"
        );
    }
}

#[test]
fn test_wasm_exports() {
    // Test that the instance has the expected exports
    let wasm_bytes = create_test_wasm();
    let runtime = WasmRuntime::from_bytes(&wasm_bytes).expect("Failed to create runtime");
    let mut instance = WasmInstance::new(&runtime).expect("Failed to create instance");

    // Check for existing exports
    assert!(
        instance.has_export("get_abi_version"),
        "Expected get_abi_version export"
    );
    assert!(
        instance.has_export("get_version_len"),
        "Expected get_version_len export"
    );
    assert!(instance.has_export("memory"), "Expected memory export");

    // Check for non-existing exports
    assert!(
        !instance.has_export("nonexistent_function"),
        "Expected no nonexistent_function export"
    );
    assert!(
        !instance.has_export("another_missing"),
        "Expected no another_missing export"
    );
}

#[test]
fn test_wasm_custom_config() {
    // Test creating a runtime with custom configuration
    let wasm_bytes = create_test_wasm();
    let config = WasmConfig::new()
        .with_max_memory_pages(512)
        .with_wasi_preview2(false);

    let runtime =
        WasmRuntime::from_bytes_with_config(&wasm_bytes, config).expect("Failed to create runtime");

    // Verify the config is applied
    assert_eq!(runtime.config().max_memory_pages, 512);
    assert!(!runtime.config().wasi_preview2);

    // Verify the instance still works
    let mut instance = WasmInstance::new(&runtime).expect("Failed to create instance");
    let version = instance
        .get_abi_version()
        .expect("Failed to get ABI version");
    assert_eq!(version, 1);
}

#[test]
fn test_wasm_config_builder_pattern() {
    // Test the WasmConfig builder pattern
    let config = WasmConfig::new()
        .with_max_memory_pages(2048)
        .with_wasi_preview2(true);

    assert_eq!(config.max_memory_pages, 2048);
    assert!(config.wasi_preview2);
}

#[test]
fn test_wasm_config_defaults() {
    // Test the default WasmConfig values
    let config = WasmConfig::default();

    assert_eq!(config.max_memory_pages, 1024);
    assert!(!config.wasi_preview2);
}

#[test]
fn test_wasm_multiple_instances_from_same_runtime() {
    // Test creating multiple instances from the same runtime
    let wasm_bytes = create_test_wasm();
    let runtime = WasmRuntime::from_bytes(&wasm_bytes).expect("Failed to create runtime");

    // Create multiple instances
    let mut instance1 = WasmInstance::new(&runtime).expect("Failed to create instance 1");
    let mut instance2 = WasmInstance::new(&runtime).expect("Failed to create instance 2");
    let mut instance3 = WasmInstance::new(&runtime).expect("Failed to create instance 3");

    // All instances should work independently
    assert_eq!(instance1.get_abi_version().unwrap(), 1);
    assert_eq!(instance2.get_abi_version().unwrap(), 1);
    assert_eq!(instance3.get_abi_version().unwrap(), 1);
}

#[test]
fn test_wasm_store_access() {
    // Test accessing the store from an instance
    let wasm_bytes = create_test_wasm();
    let runtime = WasmRuntime::from_bytes(&wasm_bytes).expect("Failed to create runtime");
    let mut instance = WasmInstance::new(&runtime).expect("Failed to create instance");

    // Verify we can access the store
    let _store = instance.store();
    let _store_mut = instance.store_mut();
}
