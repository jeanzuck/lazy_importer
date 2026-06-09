use core::ffi::c_void;

fn main() {
    // ── Step 1: resolve LoadLibraryA from kernel32 (always loaded) ──
    //
    // kernel32.dll is loaded into every Windows process, so we can resolve
    // LoadLibraryA directly via lazy_importer without any extra steps.
    type LoadLibraryAFn = unsafe extern "system" fn(name: *const u8) -> *mut c_void;
    let load_library: LoadLibraryAFn = unsafe {
        lazy_importer::li_fn!("LoadLibraryA")
            .get::<LoadLibraryAFn>()
            .expect("LoadLibraryA should resolve")
    };

    // ── Step 2: load user32.dll into the process ──
    //
    // lazy_importer walks the PEB module list — it only finds modules that
    // are *already* loaded.  A plain console app (like this example) does
    // not load user32.dll by default, so MessageBoxA would not be found.
    //
    // We use LoadLibraryA to bring user32.dll into the process first.
    // After this call, user32 appears in the PEB and lazy_importer can
    // resolve its exports.
    let user32 = unsafe { load_library(b"user32.dll\0".as_ptr()) };
    assert!(!user32.is_null(), "failed to load user32.dll");

    // ── Step 3: now resolve MessageBoxA from the freshly loaded module ──
    type MessageBoxAFn = unsafe extern "system" fn(
        hwnd: *mut c_void,
        text: *const u8,
        caption: *const u8,
        utype: u32,
    ) -> i32;

    let msgbox: MessageBoxAFn = unsafe {
        lazy_importer::li_fn!("MessageBoxA")
            .get::<MessageBoxAFn>()
            .expect("MessageBoxA should resolve")
    };

    unsafe {
        msgbox(
            core::ptr::null_mut(),
            b"Hello from lazy_importer!\0".as_ptr(),
            b"basic example\0".as_ptr(),
            0,
        );
    }
}
