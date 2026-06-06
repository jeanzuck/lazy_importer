# lazy_importer

[![License](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](https://www.apache.org/licenses/LICENSE-2.0)
[![crates.io](https://img.shields.io/crates/v/lazy_importer.svg)](https://crates.io/crates/lazy_importer)
[![docs.rs](https://docs.rs/lazy_importer/badge.svg)](https://docs.rs/lazy_importer)

`lazy_importer` is a `no_std` Rust port of Justas Masiulis's original C++
lazy_importer. It resolves already-loaded Windows modules and exports by
hashing names and walking the process PEB directly, keeping Windows internals in
small unsafe modules and exposing a typed API at the edge.

## Features

- Resolve loaded modules by hash with `li_module!`.
- Resolve exports by hash across loaded modules with `li_fn!`.
- Resolve exports inside a known module with `address_in`, `raw_address_in`, or
  `get_in`.
- Cache global module and function resolution per macro call site by default.
- Follow forwarded exports by default in `address`, `get`, `address_in`, and
  `get_in`.
- Resolve API-set forwarded exports such as `api-ms-win-*` and `ext-ms-*`
  contracts through the process API set map.
- Expose `raw_address` and `raw_address_in` for lookups that do not follow
  forwarded exports.
- Fold ASCII case for module and export hashing with the `case-insensitive`
  Cargo feature.
- Generate compile-time-random hash offsets with `const-random`; set
  `CONST_RANDOM_SEED` when deterministic build output is required.
- Fail at compile time on non-Windows targets.

Resolution is implemented for Windows x86, x86_64, and aarch64.

## Basic Usage

```rust,no_run
type GetCurrentProcessId = unsafe extern "system" fn() -> u32;

let get_current_process_id = unsafe {
    lazy_importer::li_fn!(GetCurrentProcessId)
        .get::<GetCurrentProcessId>()
        .expect("GetCurrentProcessId should be loaded")
};

let pid = unsafe { get_current_process_id() };
```

`li_fn!` accepts either an identifier or a string literal:

```rust,no_run
type GetCurrentProcessId = unsafe extern "system" fn() -> u32;

let function = unsafe {
    lazy_importer::li_fn!("GetCurrentProcessId")
        .get::<GetCurrentProcessId>()
        .expect("GetCurrentProcessId should resolve")
};
```

## Modules

Resolve a loaded module by name:

```rust,no_run
let kernel32 = lazy_importer::li_module!("KERNEL32.DLL")
    .get()
    .expect("kernel32 should be loaded");
```

Each macro invocation has its own static cache. Calling `get` again from the
same `li_module!` call site reuses the cached module handle after the first
successful lookup. There is no separate `cached` method because global module
lookups are cached by default.

## Functions

Resolve a function across all loaded modules:

```rust,no_run
type GetCurrentProcessId = unsafe extern "system" fn() -> u32;

let function = lazy_importer::li_fn!(GetCurrentProcessId);
let address = function.address().expect("export should resolve");
let typed = unsafe {
    function
        .get::<GetCurrentProcessId>()
        .expect("export should resolve")
};
```

`address` and `get` follow forwarded exports by default and cache the resolved
address per macro call site. Use `raw_address` when you need the raw export
address without following forwarded exports. There is no separate `cached`
method because global function lookups are cached by default.

## Known Modules

Resolve inside a module handle you already have:

```rust,no_run
type GetCurrentProcessId = unsafe extern "system" fn() -> u32;

let kernel32 = lazy_importer::li_module!("KERNEL32.DLL")
    .get()
    .expect("kernel32 should be loaded");

let address = lazy_importer::li_fn!("GetCurrentProcessId")
    .address_in(kernel32)
    .expect("export should resolve in kernel32");

let function = unsafe {
    lazy_importer::li_fn!("GetCurrentProcessId")
        .get_in::<GetCurrentProcessId>(kernel32)
        .expect("export should resolve in kernel32")
};
```

`address_in` and `get_in` follow forwarded exports. Use `raw_address_in` for the
raw in-module export address. In-module lookups are not cached by the macro call
site cache.

## Case Sensitivity

By default, hashing is case-sensitive. Enable `case-insensitive` if you want
module and export hashing to fold ASCII case:

```toml
lazy_importer = { version = "0.1", features = ["case-insensitive"] }
```

## Platform Behavior

On supported Windows targets, resolution walks the PEB loader list and parses PE
export directories directly. Non-Windows targets fail at compile time.
