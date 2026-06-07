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
- Resolve global module and function lookups fresh by default, with opt-in
  global caching by name via `cached`.
- Follow forwarded exports by default in `address`, `get`, `address_in`, and
  `get_in`.
- Resolve API-set forwarded exports such as `api-ms-win-*` and `ext-ms-*`
  contracts through the process API set map.
- Expose `raw_address` and `raw_address_in` for lookups that do not follow
  forwarded exports.
- Fold ASCII case for module/export hashing and cache keys with the
  `case-insensitive` Cargo feature.
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

By default, every `get` call resolves the module again. Use `cached` when you
want all `li_module!` calls for the same module name to reuse the first
successfully resolved module handle. With the `case-insensitive` feature, module
cache keys fold ASCII case too:

```rust,no_run
let kernel32 = lazy_importer::li_module!("KERNEL32.DLL")
    .cached()
    .get()
    .expect("kernel32 should be loaded");
```

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

`address` and `get` follow forwarded exports by default. By default, each call
resolves the function again. Use `cached` when you want all `li_fn!` calls for
the same export name to reuse the first successfully resolved global address.
With the `case-insensitive` feature, export cache keys fold ASCII case too:

```rust,no_run
type GetCurrentProcessId = unsafe extern "system" fn() -> u32;

let function = unsafe {
    lazy_importer::li_fn!(GetCurrentProcessId)
        .cached()
        .get::<GetCurrentProcessId>()
        .expect("export should resolve")
};
```

Use `raw_address` when you need the raw export address without following
forwarded exports. `raw_address` always resolves fresh; `cached` only affects
the forwarded global lookup used by `address` and `get`.

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
raw in-module export address. In-module lookups are not stored in the global
cache because they depend on the module handle passed to the call.

## Caching

Calling `cached` opts into a process-wide cache shared by all macro call sites
with the same module or export name. Modules and functions use separate caches,
and only successful `LazyModule::get`, `LazyFunction::address`, and
`LazyFunction::get` lookups are stored.

Cache keys are derived from the module or export name at compile time and follow
the crate's active case-sensitivity setting. The cache stores keys and resolved
pointers, not the original strings.

The cache is a fixed-size `no_std` table. If the table has no available slot, the
lookup still returns the freshly resolved value, but that value is not stored for
future calls.

## Case Sensitivity

By default, hashing is case-sensitive. Enable `case-insensitive` if you want
module and export hashing to fold ASCII case:

```toml
lazy_importer = { version = "0.1", features = ["case-insensitive"] }
```

## Platform Behavior

On supported Windows targets, resolution walks the PEB loader list and parses PE
export directories directly. Non-Windows targets fail at compile time.
