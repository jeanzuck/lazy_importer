fn main() {
    type GetCurrentProcessId = unsafe extern "system" fn() -> u32;

    let get_current_process_id = unsafe {
        lazy_importer::li_fn!(GetCurrentProcessId)
            .get::<GetCurrentProcessId>()
            .expect("GetCurrentProcessId should resolve")
    };

    let pid = unsafe { get_current_process_id() };
    println!("{pid}");
}
