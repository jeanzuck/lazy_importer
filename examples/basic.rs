use core::ffi::c_void;

fn main() {
    // ╔══════════════════════════════════════════════════════════════════╗
    // ║  Example 1 — kernel32 API (always loaded, no LoadLibrary needed) ║
    // ╚══════════════════════════════════════════════════════════════════╝
    //
    // kernel32.dll is loaded into *every* Windows process by the OS
    // loader.  lazy_importer walks the PEB module list and finds it
    // automatically — no LoadLibrary / GetProcAddress required.
    //
    // Any export from kernel32 works straight away.  Here we call
    // GetCurrentProcessId, a trivial example:
    {
        type GetCurrentProcessIdFn = unsafe extern "system" fn() -> u32;
        let get_pid: GetCurrentProcessIdFn = unsafe {
            lazy_importer::li_fn!("GetCurrentProcessId")
                .get::<GetCurrentProcessIdFn>()
                .expect("GetCurrentProcessId should resolve from kernel32")
        };

        let pid = unsafe { get_pid() };
        println!("[example 1] current process id = {pid}");
    }

    // Other kernel32 APIs you could use directly:
    //   GetModuleHandleA, GetSystemInfo, IsDebuggerPresent, Sleep, …

    // ╔══════════════════════════════════════════════════════════════════╗
    // ║  Example 2 — user32 API (NOT loaded by default in console apps)  ║
    // ╚══════════════════════════════════════════════════════════════════╝
    //
    // user32.dll is **not** loaded into a typical console/headless
    // process.  If you tried to resolve MessageBoxA directly with
    // lazy_importer, it would return `None` because user32 is absent
    // from the PEB module list.
    //
    // Workflow:
    //   1. Resolve LoadLibraryA (kernel32 → always loaded).
    //   2. Call LoadLibraryA("user32.dll") to bring it into the process.
    //   3. Now user32 is in the PEB → lazy_importer can find MessageBoxA.

    // Step 1 — resolve LoadLibraryA from kernel32
    type LoadLibraryAFn = unsafe extern "system" fn(name: *const u8) -> *mut c_void;
    let load_library: LoadLibraryAFn = unsafe {
        lazy_importer::li_fn!("LoadLibraryA")
            .get::<LoadLibraryAFn>()
            .expect("LoadLibraryA should resolve")
    };

    // Step 2 — load user32.dll (only needed once)
    let user32 = unsafe { load_library(b"user32.dll\0".as_ptr()) };
    assert!(!user32.is_null(), "failed to load user32.dll");

    // Step 3 — now MessageBoxA is discoverable
    type MessageBoxAFn = unsafe extern "system" fn(
        hwnd: *mut c_void,
        text: *const u8,
        caption: *const u8,
        utype: u32,
    ) -> i32;

    let msgbox: MessageBoxAFn = unsafe {
        lazy_importer::li_fn!("MessageBoxA")
            .get::<MessageBoxAFn>()
            .expect("MessageBoxA should resolve after loading user32")
    };

    println!("[example 2] a message box should now appear — check your desktop!");

    unsafe {
        msgbox(
            core::ptr::null_mut(),
            b"Hello from lazy_importer!\0".as_ptr(),
            b"basic example\0".as_ptr(),
            0,
        );
    }
}
