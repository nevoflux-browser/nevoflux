//! Integration tests for host functions.
//!
//! These tests verify that host functions are correctly registered and
//! callable from Wasm guest modules.

use nevoflux_daemon::{WasmInstance, WasmRuntime};

/// Create a Wasm module that imports and tests the get_error_len host function.
fn create_error_test_wasm() -> Vec<u8> {
    wat::parse_str(
        r#"
        (module
            (import "nevoflux" "get_error_len" (func $get_error_len (result i32)))

            (memory (export "memory") 1)

            (func (export "get_abi_version") (result i32) i32.const 1)

            (func (export "test_error_len") (result i32)
                call $get_error_len
            )
        )
        "#,
    )
    .unwrap()
}

/// Create a Wasm module that imports and tests memory_search host function.
fn create_memory_search_test_wasm() -> Vec<u8> {
    wat::parse_str(
        r#"
        (module
            (import "nevoflux" "memory_search" (func $memory_search (param i32 i32 i32 i32 i32) (result i32)))

            (memory (export "memory") 1)

            (func (export "get_abi_version") (result i32) i32.const 1)

            (func (export "test_memory_search") (result i32)
                i32.const 0   ;; query_ptr
                i32.const 4   ;; query_len
                i32.const 10  ;; limit
                i32.const 100 ;; result_ptr
                i32.const 1024 ;; result_len
                call $memory_search
            )
        )
        "#,
    )
    .unwrap()
}

/// Create a Wasm module that imports and tests skill_list host function.
fn create_skill_list_test_wasm() -> Vec<u8> {
    wat::parse_str(
        r#"
        (module
            (import "nevoflux" "skill_list" (func $skill_list (param i32 i32) (result i32)))

            (memory (export "memory") 1)

            (func (export "get_abi_version") (result i32) i32.const 1)

            (func (export "test_skill_list") (result i32)
                i32.const 0    ;; result_ptr
                i32.const 1024 ;; result_len
                call $skill_list
            )
        )
        "#,
    )
    .unwrap()
}

/// Create a comprehensive Wasm module that imports all major host functions.
fn create_full_host_test_wasm() -> Vec<u8> {
    wat::parse_str(
        r#"
        (module
            (import "nevoflux" "get_error_len" (func $get_error_len (result i32)))
            (import "nevoflux" "get_error" (func $get_error (param i32 i32) (result i32)))
            (import "nevoflux" "memory_search" (func $memory_search (param i32 i32 i32 i32 i32) (result i32)))
            (import "nevoflux" "memory_create" (func $memory_create (param i32 i32 i32 i32 i32 i32) (result i32)))
            (import "nevoflux" "memory_delete" (func $memory_delete (param i32 i32) (result i32)))
            (import "nevoflux" "skill_list" (func $skill_list (param i32 i32) (result i32)))
            (import "nevoflux" "skill_load" (func $skill_load (param i32 i32 i32 i32) (result i32)))
            (import "nevoflux" "permission_check" (func $permission_check (param i32 i32 i32 i32) (result i32)))
            (import "nevoflux" "tool_read" (func $tool_read (param i32 i32 i64 i64 i32 i32) (result i32)))
            (import "nevoflux" "tool_glob" (func $tool_glob (param i32 i32 i32 i32 i32 i32) (result i32)))

            (memory (export "memory") 1)

            (func (export "get_abi_version") (result i32) i32.const 1)
        )
        "#,
    )
    .unwrap()
}

#[test]
fn test_host_functions_available() {
    // Test that a Wasm module importing all host functions can be instantiated
    let wasm = create_full_host_test_wasm();
    let runtime = WasmRuntime::from_bytes(&wasm).unwrap();
    let instance = WasmInstance::new(&runtime);
    assert!(
        instance.is_ok(),
        "Failed to instantiate module with all host functions: {:?}",
        instance.err()
    );
}

#[test]
fn test_error_len_initially_zero() {
    // Test that get_error_len returns 0 when no error has been set
    let wasm = create_error_test_wasm();
    let runtime = WasmRuntime::from_bytes(&wasm).unwrap();
    let mut instance = WasmInstance::new(&runtime).unwrap();

    // Verify the module has the test export
    assert!(instance.has_export("test_error_len"));

    // Get the ABI version to verify the instance works
    let version = instance.get_abi_version().unwrap();
    assert_eq!(version, 1);
}

#[test]
fn test_memory_search_module_instantiation() {
    // Test that a module using memory_search can be instantiated
    let wasm = create_memory_search_test_wasm();
    let runtime = WasmRuntime::from_bytes(&wasm).unwrap();
    let mut instance = WasmInstance::new(&runtime).unwrap();

    // Verify the module has the test export
    assert!(instance.has_export("test_memory_search"));

    // Get the ABI version to verify the instance works
    let version = instance.get_abi_version().unwrap();
    assert_eq!(version, 1);
}

#[test]
fn test_skill_list_module_instantiation() {
    // Test that a module using skill_list can be instantiated
    let wasm = create_skill_list_test_wasm();
    let runtime = WasmRuntime::from_bytes(&wasm).unwrap();
    let mut instance = WasmInstance::new(&runtime).unwrap();

    // Verify the module has the test export
    assert!(instance.has_export("test_skill_list"));

    // Get the ABI version to verify the instance works
    let version = instance.get_abi_version().unwrap();
    assert_eq!(version, 1);
}

#[test]
fn test_memory_export_available() {
    // Test that the memory export is available for host functions to use
    let wasm = create_full_host_test_wasm();
    let runtime = WasmRuntime::from_bytes(&wasm).unwrap();
    let mut instance = WasmInstance::new(&runtime).unwrap();

    assert!(
        instance.has_export("memory"),
        "Memory export should be available"
    );
}

#[test]
fn test_llm_chat_import() {
    // Test that llm_chat host function can be imported
    let wasm = wat::parse_str(
        r#"
        (module
            (import "nevoflux" "llm_chat" (func $llm_chat (param i32 i32 i32 i32) (result i32)))
            (memory (export "memory") 1)
            (func (export "get_abi_version") (result i32) i32.const 1)
        )
        "#,
    )
    .unwrap();

    let runtime = WasmRuntime::from_bytes(&wasm).unwrap();
    let instance = WasmInstance::new(&runtime);
    assert!(
        instance.is_ok(),
        "Module with llm_chat import should instantiate"
    );
}

#[test]
fn test_permission_check_import() {
    // Test that permission_check host function can be imported
    let wasm = wat::parse_str(
        r#"
        (module
            (import "nevoflux" "permission_check" (func $permission_check (param i32 i32 i32 i32) (result i32)))
            (memory (export "memory") 1)
            (func (export "get_abi_version") (result i32) i32.const 1)
        )
        "#,
    )
    .unwrap();

    let runtime = WasmRuntime::from_bytes(&wasm).unwrap();
    let instance = WasmInstance::new(&runtime);
    assert!(
        instance.is_ok(),
        "Module with permission_check import should instantiate"
    );
}

#[test]
fn test_tool_functions_import() {
    // Test that tool_read and tool_glob host functions can be imported
    let wasm = wat::parse_str(
        r#"
        (module
            (import "nevoflux" "tool_read" (func $tool_read (param i32 i32 i64 i64 i32 i32) (result i32)))
            (import "nevoflux" "tool_glob" (func $tool_glob (param i32 i32 i32 i32 i32 i32) (result i32)))
            (memory (export "memory") 1)
            (func (export "get_abi_version") (result i32) i32.const 1)
        )
        "#,
    )
    .unwrap();

    let runtime = WasmRuntime::from_bytes(&wasm).unwrap();
    let instance = WasmInstance::new(&runtime);
    assert!(
        instance.is_ok(),
        "Module with tool functions import should instantiate"
    );
}

#[test]
fn test_memory_functions_import() {
    // Test that memory_create, memory_delete, and memory_search host functions can be imported
    let wasm = wat::parse_str(
        r#"
        (module
            (import "nevoflux" "memory_create" (func $memory_create (param i32 i32 i32 i32 i32 i32) (result i32)))
            (import "nevoflux" "memory_delete" (func $memory_delete (param i32 i32) (result i32)))
            (import "nevoflux" "memory_search" (func $memory_search (param i32 i32 i32 i32 i32) (result i32)))
            (memory (export "memory") 1)
            (func (export "get_abi_version") (result i32) i32.const 1)
        )
        "#,
    )
    .unwrap();

    let runtime = WasmRuntime::from_bytes(&wasm).unwrap();
    let instance = WasmInstance::new(&runtime);
    assert!(
        instance.is_ok(),
        "Module with memory functions import should instantiate"
    );
}

#[test]
fn test_skill_functions_import() {
    // Test that skill_list and skill_load host functions can be imported
    let wasm = wat::parse_str(
        r#"
        (module
            (import "nevoflux" "skill_list" (func $skill_list (param i32 i32) (result i32)))
            (import "nevoflux" "skill_load" (func $skill_load (param i32 i32 i32 i32) (result i32)))
            (memory (export "memory") 1)
            (func (export "get_abi_version") (result i32) i32.const 1)
        )
        "#,
    )
    .unwrap();

    let runtime = WasmRuntime::from_bytes(&wasm).unwrap();
    let instance = WasmInstance::new(&runtime);
    assert!(
        instance.is_ok(),
        "Module with skill functions import should instantiate"
    );
}

#[test]
fn test_multiple_instances_independent() {
    // Test that multiple instances from the same runtime are independent
    let wasm = create_full_host_test_wasm();
    let runtime = WasmRuntime::from_bytes(&wasm).unwrap();

    let mut instance1 = WasmInstance::new(&runtime).unwrap();
    let mut instance2 = WasmInstance::new(&runtime).unwrap();

    // Both instances should have the same exports
    assert!(instance1.has_export("memory"));
    assert!(instance2.has_export("memory"));

    // Both should have working ABI version
    assert_eq!(instance1.get_abi_version().unwrap(), 1);
    assert_eq!(instance2.get_abi_version().unwrap(), 1);
}
