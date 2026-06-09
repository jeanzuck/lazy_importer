use core::ffi::c_void;
use core::ptr::NonNull;

use crate::hash;
use crate::types::{ImageDosHeader, ImageExportDirectory, ImageNtHeaders};

const IMAGE_DOS_SIGNATURE: u16 = 0x5a4d;
const IMAGE_NT_SIGNATURE: u32 = 0x0000_4550;
const IMAGE_DIRECTORY_ENTRY_EXPORT: usize = 0;

pub(crate) struct ExportsDirectory {
    base: NonNull<u8>,
    directory: *const ImageExportDirectory,
    directory_size: u32,
}

impl ExportsDirectory {
    #[inline(always)]
    pub(crate) unsafe fn new(base: NonNull<c_void>) -> Option<Self> {
        let base = base.cast::<u8>();
        let dos = unsafe { &*base.as_ptr().cast::<ImageDosHeader>() };

        if dos.e_magic != IMAGE_DOS_SIGNATURE || dos.e_lfanew < 0 {
            return None;
        }

        let nt = unsafe {
            &*base
                .as_ptr()
                .add(dos.e_lfanew as usize)
                .cast::<ImageNtHeaders>()
        };

        if nt.signature != IMAGE_NT_SIGNATURE {
            return None;
        }

        let directory = nt.optional_header.data_directory[IMAGE_DIRECTORY_ENTRY_EXPORT];
        if directory.virtual_address == 0 || directory.size == 0 {
            return None;
        }

        Some(Self {
            base,
            directory: unsafe {
                base.as_ptr()
                    .add(directory.virtual_address as usize)
                    .cast::<ImageExportDirectory>()
            },
            directory_size: directory.size,
        })
    }

    #[inline]
    pub(crate) unsafe fn find_by_hash(
        &self,
        target_hash: u32,
        offset: u32,
    ) -> Option<NonNull<c_void>> {
        let mut index = 0;
        let size = self.size();

        while index < size {
            let name = unsafe { self.name(index)? };
            if unsafe { hash::hash_c_str(name, offset) } == target_hash {
                return unsafe { self.address(index) };
            }

            index += 1;
        }

        None
    }

    #[inline]
    pub(crate) fn is_forwarded(&self, address: NonNull<c_void>) -> bool {
        let address = address.as_ptr() as usize;
        let start = self.directory as usize;
        let end = start.saturating_add(self.directory_size as usize);

        address >= start && address < end
    }

    #[inline]
    fn size(&self) -> u32 {
        unsafe { (*self.directory).number_of_names }
    }

    #[inline]
    unsafe fn name(&self, index: u32) -> Option<*const u8> {
        if index >= self.size() {
            return None;
        }

        let names = unsafe {
            self.base
                .as_ptr()
                .add((*self.directory).address_of_names as usize)
                .cast::<u32>()
        };
        let name_rva = unsafe { *names.add(index as usize) };

        Some(unsafe { self.base.as_ptr().add(name_rva as usize).cast::<u8>() })
    }

    #[inline]
    unsafe fn address(&self, index: u32) -> Option<NonNull<c_void>> {
        if index >= self.size() {
            return None;
        }

        let directory = unsafe { &*self.directory };
        let ordinals = unsafe {
            self.base
                .as_ptr()
                .add(directory.address_of_name_ordinals as usize)
                .cast::<u16>()
        };
        let functions = unsafe {
            self.base
                .as_ptr()
                .add(directory.address_of_functions as usize)
                .cast::<u32>()
        };

        let ordinal = unsafe { *ordinals.add(index as usize) } as u32;
        if ordinal >= directory.number_of_functions {
            return None;
        }

        let function_rva = unsafe { *functions.add(ordinal as usize) };
        NonNull::new(unsafe {
            self.base
                .as_ptr()
                .add(function_rva as usize)
                .cast::<c_void>()
        })
    }
}
