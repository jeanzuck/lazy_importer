# lazy_importer

[![Rust](https://img.shields.io/badge/rust-stable%201.85+-orange.svg)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](https://www.apache.org/licenses/LICENSE-2.0)
[![crates.io](https://img.shields.io/crates/v/lazy_importer.svg)](https://crates.io/crates/lazy_importer)
[![docs.rs](https://docs.rs/lazy_importer/badge.svg)](https://docs.rs/lazy_importer)

A `no_std` Rust library that resolves Windows API functions at runtime —
without linking to import libraries. It walks the PEB loader list and PE export
tables directly, so function names never appear in your binary's import table.

Rust port of [Justas Masiulis's original C++ lazy_importer](https://github.com/JustasMasiulis/lazy_importer).

## Quick Start

```toml
# Cargo.toml
[dependencies]
lazy_importer = "0.1"
```

```rust,no_run
// 1. Declare the function signature
type MessageBoxA = unsafe extern "system" fn(
    hwnd: *mut core::ffi::c_void,
    text: *const u8,
    caption: *const u8,
    utype: u32,
) -> i32;

// 2. Resolve + call — nothing appears in the import table
let msgbox: MessageBoxA = unsafe {
    lazy_importer::li_fn!("MessageBoxA")
        .get::<MessageBoxA>()
        .expect("MessageBoxA should resolve")
};

unsafe {
    msgbox(
        core::ptr::null_mut(),
        b"Hello\0".as_ptr(),
        b"lazy_importer\0".as_ptr(),
        0,
    );
}
```

## Usage

### Resolve a function globally

`li_fn!` takes a string literal. `get` casts the resolved address to your
function pointer type and follows forwarded exports.

```rust,no_run
type GetCurrentProcessId = unsafe extern "system" fn() -> u32;

let f: GetCurrentProcessId = unsafe {
    lazy_importer::li_fn!("GetCurrentProcessId")
        .get::<GetCurrentProcessId>()
        .expect("GetCurrentProcessId should resolve")
};
```

Use `.address()` when you only need the raw pointer (still follows forwarded
exports):

```rust,no_run
let addr = lazy_importer::li_fn!("GetCurrentProcessId")
    .address()
    .expect("GetCurrentProcessId should resolve");
```

### Resolve without following forwarded exports

`raw_address` returns the export RVA directly — forwarded exports are **not**
resolved to their final target.

```rust,no_run
let raw = lazy_importer::li_fn!("GetCurrentProcessId").raw_address();
```

### Resolve a loaded module

```rust,no_run
let kernel32 = lazy_importer::li_module!("KERNEL32.DLL")
    .get()
    .expect("kernel32 should be loaded");

// Use the handle directly
use lazy_importer::ModuleHandle;
let handle: ModuleHandle = kernel32;
let ptr: *mut core::ffi::c_void = handle.as_ptr();
```

### Resolve inside a known module

When you already have a `ModuleHandle`, resolve the export in that specific
module. `address_in` follows forwarded exports; `raw_address_in` does not.

```rust,no_run
let kernel32 = lazy_importer::li_module!("KERNEL32.DLL")
    .get()
    .expect("kernel32 should be loaded");

// With forwarded-export resolution
let addr = lazy_importer::li_fn!("GetCurrentProcessId")
    .address_in(kernel32)
    .expect("export should resolve in kernel32");

// Raw — no forwarded-export resolution
let raw = lazy_importer::li_fn!("GetCurrentProcessId")
    .raw_address_in(kernel32)
    .expect("export should exist in kernel32");

// Cast to a function pointer type in one step
type GetCurrentProcessId = unsafe extern "system" fn() -> u32;
let f: GetCurrentProcessId = unsafe {
    lazy_importer::li_fn!("GetCurrentProcessId")
        .get_in::<GetCurrentProcessId>(kernel32)
        .expect("export should resolve in kernel32")
};
```

### Caching

By default every lookup walks the PEB again. Chain `.cached()` to store the
first successful result in a process-wide lock-free table, keyed by the name
you passed to the macro.

```rust,no_run
// Module cache — first .get() resolves; subsequent calls hit the cache
let kernel32 = lazy_importer::li_module!("KERNEL32.DLL")
    .cached()
    .get()
    .expect("kernel32 should be loaded");

// Function cache — works for .address() and .get()
type GetCurrentProcessId = unsafe extern "system" fn() -> u32;
let f: GetCurrentProcessId = unsafe {
    lazy_importer::li_fn!("GetCurrentProcessId")
        .cached()
        .get::<GetCurrentProcessId>()
        .expect("GetCurrentProcessId should resolve")
};
```

- Modules and functions use **separate** caches.
- Cache keys are derived from the name at compile time; the cache stores only
  resolved pointers, never strings.
- The cache is a fixed-size `no_std` table (256 slots, linear probing). If full,
  the lookup still succeeds — it just isn't stored.
- `raw_address` and `raw_address_in` **never** use the cache.

### Case-insensitive hashing

Enable the `case-insensitive` feature to fold ASCII case when hashing module
and export names:

```toml
[dependencies]
lazy_importer = { version = "0.1", features = ["case-insensitive"] }
```

```rust,no_run
// With case-insensitive enabled, both resolve to the same module
let a = lazy_importer::li_module!("KERNEL32.DLL").cached().get();
let b = lazy_importer::li_module!("kernel32.dll").cached().get();
assert_eq!(a.unwrap().as_ptr(), b.unwrap().as_ptr());
```

## API Reference

### Macros

| Macro | Returns | Description |
|-------|---------|-------------|
| `li_fn!("Name")` | `LazyFunction<HASH>` | Resolves the named export across all loaded modules |
| `li_module!("NAME")` | `LazyModule<HASH>` | Resolves the named module in the PEB loader list |

### `LazyFunction<OHP>`

| Method | Returns | Follows forwarded? | Uses cache? |
|--------|---------|--------------------|-------------|
| `.address()` | `Option<NonNull<c_void>>` | Yes | If `.cached()` |
| `.raw_address()` | `Option<NonNull<c_void>>` | No | Never |
| `.get::<F>()` | `Option<F>` | Yes | If `.cached()` |
| `.address_in(m)` | `Option<NonNull<c_void>>` | Yes | Never |
| `.raw_address_in(m)` | `Option<NonNull<c_void>>` | No | Never |
| `.get_in::<F>(m)` | `Option<F>` | Yes | Never |
| `.cached()` | `Self` | — | Enables global cache |

### `LazyModule<OHP>`

| Method | Returns | Uses cache? |
|--------|---------|-------------|
| `.get()` | `Option<ModuleHandle>` | If `.cached()` |
| `.cached()` | `Self` | Enables global cache |

### `ModuleHandle`

| Method | Returns |
|--------|---------|
| `.from_ptr(p)` | `Option<Self>` |
| `.as_ptr()` | `*mut c_void` |

## How It Works

```text
┌─────────────────────────────────────────────────┐
│                 Your Process                      │
│                                                   │
│  li_fn!("VirtualAlloc").get::<F>()                │
│  ├─ hash("VirtualAlloc") at compile time          │
│  ├─ walk PEB → InLoadOrderModuleList              │
│  │   ├─ kernel32.dll → parse PE export directory  │
│  │   ├─ ntdll.dll    → parse PE export directory  │
│  │   └─ ...                                       │
│  ├─ compare export name hashes                    │
│  ├─ follow forwarded exports (up to 32 hops)      │
│  ├─ resolve API-set contracts (api-ms-win-*)      │
│  └─ return function pointer                       │
│                                                   │
│  • No LoadLibrary / GetProcAddress calls          │
│  • No entries in the binary's import table        │
│  • Compile-time hash offsets (const-random)        │
│    randomize the FNV-1a seed per call site        │
└─────────────────────────────────────────────────┘
```

## Platform Support

| Architecture | Status |
|-------------|--------|
| `x86_64-pc-windows-msvc` | ✅ Full support |
| `i686-pc-windows-msvc`   | ✅ Full support |
| `aarch64-pc-windows-msvc`| ✅ Full support |
| Non-Windows              | ❌ Compile-time error |

Requires Rust **1.85+** (edition 2024).

## Building

Release builds enable **LTO** (`lto = true`) so that resolution code is
inlined directly into call sites — no separate function shows up in the
disassembly for `li_fn!` or `li_module!` lookups.

```sh
cargo build --release
```

Set `CONST_RANDOM_SEED` for deterministic (reproducible) builds:

```sh
CONST_RANDOM_SEED=0xDEAD_BEEF cargo build --release
```

## Testing

```sh
# Unit tests (requires Windows)
cargo test

# With case-insensitive feature
cargo test --features case-insensitive --test '*' --lib
```

## License

Apache 2.0 — see [LICENSE](LICENSE).
