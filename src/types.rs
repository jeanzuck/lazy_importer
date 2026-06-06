use core::ffi::c_void;

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct ListEntry {
    pub flink: *const ListEntry,
    pub blink: *const ListEntry,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct UnicodeString {
    pub length: u16,
    pub maximum_length: u16,
    pub buffer: *const u16,
}

impl UnicodeString {
    #[inline]
    pub(crate) const fn len_u16(self) -> usize {
        (self.length as usize) / core::mem::size_of::<u16>()
    }
}

#[repr(C)]
pub(crate) struct PebLdrData {
    pub length: u32,
    pub initialized: u32,
    pub ss_handle: *const c_void,
    pub in_load_order_module_list: ListEntry,
}

#[repr(C)]
pub(crate) struct Peb {
    pub reserved1: [u8; 2],
    pub being_debugged: u8,
    pub reserved2: [u8; 1],
    pub reserved3: [*const c_void; 2],
    pub ldr: *const PebLdrData,
    pub process_parameters: *const c_void,
    pub sub_system_data: *const c_void,
    pub process_heap: *const c_void,
    pub fast_peb_lock: *const c_void,
    pub ifeo_key: *const c_void,
    pub atl_thunk_slist_ptr: *const c_void,
    pub cross_process_flags: u32,
    pub reserved4: *const c_void,
    pub system_reserved: u32,
    pub atl_thunk_slist_ptr32: u32,
    pub api_set_map: *const ApiSetNamespace,
}

#[repr(C)]
pub(crate) struct LdrDataTableEntry {
    pub in_load_order_links: ListEntry,
    pub in_memory_order_links: ListEntry,
    pub in_initialization_order_links: ListEntry,
    pub dll_base: *mut c_void,
    pub entry_point: *mut c_void,
    pub size_of_image: u32,
    pub full_dll_name: UnicodeString,
    pub base_dll_name: UnicodeString,
}

#[repr(C)]
pub(crate) struct ApiSetNamespace {
    pub version: u32,
    pub size: u32,
    pub flags: u32,
    pub count: u32,
    pub entry_offset: u32,
    pub hash_offset: u32,
    pub hash_factor: u32,
}

#[repr(C)]
pub(crate) struct ApiSetNamespaceEntry {
    pub flags: u32,
    pub name_offset: u32,
    pub name_length: u32,
    pub hashed_length: u32,
    pub value_offset: u32,
    pub value_count: u32,
}

#[repr(C)]
pub(crate) struct ApiSetValueEntry {
    pub flags: u32,
    pub name_offset: u32,
    pub name_length: u32,
    pub value_offset: u32,
    pub value_length: u32,
}

#[repr(C)]
pub(crate) struct ImageDosHeader {
    pub e_magic: u16,
    pub e_cblp: u16,
    pub e_cp: u16,
    pub e_crlc: u16,
    pub e_cparhdr: u16,
    pub e_minalloc: u16,
    pub e_maxalloc: u16,
    pub e_ss: u16,
    pub e_sp: u16,
    pub e_csum: u16,
    pub e_ip: u16,
    pub e_cs: u16,
    pub e_lfarlc: u16,
    pub e_ovno: u16,
    pub e_res: [u16; 4],
    pub e_oemid: u16,
    pub e_oeminfo: u16,
    pub e_res2: [u16; 10],
    pub e_lfanew: i32,
}

#[repr(C)]
pub(crate) struct ImageFileHeader {
    pub machine: u16,
    pub number_of_sections: u16,
    pub time_date_stamp: u32,
    pub pointer_to_symbol_table: u32,
    pub number_of_symbols: u32,
    pub size_of_optional_header: u16,
    pub characteristics: u16,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct ImageDataDirectory {
    pub virtual_address: u32,
    pub size: u32,
}

#[repr(C)]
#[cfg(target_pointer_width = "64")]
pub(crate) struct ImageOptionalHeader64 {
    pub magic: u16,
    pub major_linker_version: u8,
    pub minor_linker_version: u8,
    pub size_of_code: u32,
    pub size_of_initialized_data: u32,
    pub size_of_uninitialized_data: u32,
    pub address_of_entry_point: u32,
    pub base_of_code: u32,
    pub image_base: u64,
    pub section_alignment: u32,
    pub file_alignment: u32,
    pub major_operating_system_version: u16,
    pub minor_operating_system_version: u16,
    pub major_image_version: u16,
    pub minor_image_version: u16,
    pub major_subsystem_version: u16,
    pub minor_subsystem_version: u16,
    pub win32_version_value: u32,
    pub size_of_image: u32,
    pub size_of_headers: u32,
    pub check_sum: u32,
    pub subsystem: u16,
    pub dll_characteristics: u16,
    pub size_of_stack_reserve: u64,
    pub size_of_stack_commit: u64,
    pub size_of_heap_reserve: u64,
    pub size_of_heap_commit: u64,
    pub loader_flags: u32,
    pub number_of_rva_and_sizes: u32,
    pub data_directory: [ImageDataDirectory; 16],
}

#[repr(C)]
#[cfg(target_pointer_width = "32")]
pub(crate) struct ImageOptionalHeader32 {
    pub magic: u16,
    pub major_linker_version: u8,
    pub minor_linker_version: u8,
    pub size_of_code: u32,
    pub size_of_initialized_data: u32,
    pub size_of_uninitialized_data: u32,
    pub address_of_entry_point: u32,
    pub base_of_code: u32,
    pub base_of_data: u32,
    pub image_base: u32,
    pub section_alignment: u32,
    pub file_alignment: u32,
    pub major_operating_system_version: u16,
    pub minor_operating_system_version: u16,
    pub major_image_version: u16,
    pub minor_image_version: u16,
    pub major_subsystem_version: u16,
    pub minor_subsystem_version: u16,
    pub win32_version_value: u32,
    pub size_of_image: u32,
    pub size_of_headers: u32,
    pub check_sum: u32,
    pub subsystem: u16,
    pub dll_characteristics: u16,
    pub size_of_stack_reserve: u32,
    pub size_of_stack_commit: u32,
    pub size_of_heap_reserve: u32,
    pub size_of_heap_commit: u32,
    pub loader_flags: u32,
    pub number_of_rva_and_sizes: u32,
    pub data_directory: [ImageDataDirectory; 16],
}

#[cfg(target_pointer_width = "64")]
pub(crate) type ImageOptionalHeader = ImageOptionalHeader64;

#[cfg(target_pointer_width = "32")]
pub(crate) type ImageOptionalHeader = ImageOptionalHeader32;

#[repr(C)]
pub(crate) struct ImageNtHeaders {
    pub signature: u32,
    pub file_header: ImageFileHeader,
    pub optional_header: ImageOptionalHeader,
}

#[repr(C)]
pub(crate) struct ImageExportDirectory {
    pub characteristics: u32,
    pub time_date_stamp: u32,
    pub major_version: u16,
    pub minor_version: u16,
    pub name: u32,
    pub base: u32,
    pub number_of_functions: u32,
    pub number_of_names: u32,
    pub address_of_functions: u32,
    pub address_of_names: u32,
    pub address_of_name_ordinals: u32,
}
