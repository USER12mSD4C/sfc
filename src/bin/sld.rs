use memmap2::{Mmap, MmapMut};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::path::{Path, PathBuf};
use std::slice;
use std::sync::Arc;

// =============================================================================
// 1. Структуры ELF64 (repr(C) для прямого чтения и записи)
// =============================================================================

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct Elf64_Ehdr {
    pub e_ident: [u8; 16],
    pub e_type: u16,
    pub e_machine: u16,
    pub e_version: u32,
    pub e_entry: u64,
    pub e_phoff: u64,
    pub e_shoff: u64,
    pub e_flags: u32,
    pub e_ehsize: u16,
    pub e_phentsize: u16,
    pub e_phnum: u16,
    pub e_shentsize: u16,
    pub e_shnum: u16,
    pub e_shstrndx: u16,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct Elf64_Shdr {
    pub sh_name: u32,
    pub sh_type: u32,
    pub sh_flags: u64,
    pub sh_addr: u64,
    pub sh_offset: u64,
    pub sh_size: u64,
    pub sh_link: u32,
    pub sh_info: u32,
    pub sh_addralign: u64,
    pub sh_entsize: u64,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct Elf64_Sym {
    pub st_name: u32,
    pub st_info: u8,
    pub st_other: u8,
    pub st_shndx: u16,
    pub st_value: u64,
    pub st_size: u64,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct Elf64_Phdr {
    pub p_type: u32,
    pub p_flags: u32,
    pub p_offset: u64,
    pub p_vaddr: u64,
    pub p_paddr: u64,
    pub p_filesz: u64,
    pub p_memsz: u64,
    pub p_align: u64,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct Elf64_Rela {
    pub r_offset: u64,
    pub r_info: u64,
    pub r_addend: i64,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct ArHeader {
    pub ar_name: [u8; 16],
    pub _ar_date: [u8; 12],
    pub _ar_uid: [u8; 6],
    pub _ar_gid: [u8; 6],
    pub _ar_mode: [u8; 8],
    pub ar_size: [u8; 10],
    pub _ar_fmag: [u8; 2],
}

// Константы ELF
const ELF_MAGIC: [u8; 4] = [0x7f, b'E', b'L', b'F'];
const SHT_SYMTAB: u32 = 2;
const SHT_STRTAB: u32 = 3;
const SHT_RELA: u32 = 4;

const SHN_UNDEF: u16 = 0;
const SHN_COMMON: u16 = 0xfff2;

const STB_GLOBAL: u8 = 1;
const STB_WEAK: u8 = 2;

const SHF_ALLOC: u64 = 0x2;
const SHF_WRITE: u64 = 0x1;
const SHF_MERGE: u64 = 0x10;
const SHF_STRINGS: u64 = 0x20;

const PT_LOAD: u32 = 1;
const PT_TLS: u32 = 7;
const PT_GNU_EH_FRAME: u32 = 0x6474e550;
const PT_GNU_STACK: u32 = 0x6474e551;

const PF_X: u32 = 0x1;
const PF_W: u32 = 0x2;
const PF_R: u32 = 0x4;

const PAGE_SIZE: u64 = 0x1000;

// Типы релокаций x86_64
const R_X86_64_NONE: u32 = 0;
const R_X86_64_64: u32 = 1;
const R_X86_64_PC32: u32 = 2;
const R_X86_64_PLT32: u32 = 4;
const R_X86_64_GOTPCREL: u32 = 9;
const R_X86_64_32: u32 = 10;
const R_X86_64_32S: u32 = 11;
const R_X86_64_TPOFF32: u32 = 18;
const R_X86_64_GOTTPOFF: u32 = 22;
const R_X86_64_PC64: u32 = 24;
const R_X86_64_GOTPCRELX: u32 = 41;
const R_X86_64_REX_GOTPCRELX: u32 = 42;

// =============================================================================
// 2. Структуры разбора фреймов исключений .eh_frame (DWARF)
// =============================================================================

#[derive(Clone)]
pub struct CieRecord {
    pub obj_idx: usize,
    pub input_offset: usize,
    pub bytes: Vec<u8>,
    pub fde_encoding: u8,
}

#[derive(Clone)]
pub struct FdeRecord {
    pub input_offset: usize,
    pub cie_input_offset: usize,
    pub bytes: Vec<u8>,
    pub pc_begin_offset: usize,
    pub obj_idx: usize,
    pub sec_idx: usize,
}

// =============================================================================
// 3. Безопасные утилиты для Zero-Copy парсинга
// =============================================================================

#[inline(always)]
pub unsafe fn read_struct<T>(bytes: &[u8], offset: usize) -> Option<T> {
    let size = std::mem::size_of::<T>();
    if offset + size <= bytes.len() {
        let ptr = bytes.as_ptr().add(offset) as *const T;
        Some(std::ptr::read_unaligned(ptr))
    } else {
        None
    }
}

#[inline(always)]
pub unsafe fn read_slice<'a, T>(bytes: &'a [u8], offset: usize, count: usize) -> Option<&'a [T]> {
    let size = std::mem::size_of::<T>() * count;
    if offset + size <= bytes.len() {
        let ptr = bytes.as_ptr().add(offset) as *const T;
        Some(slice::from_raw_parts(ptr, count))
    } else {
        None
    }
}

#[inline(always)]
fn align_up(val: u64, align: u64) -> u64 {
    if align == 0 {
        val
    } else {
        (val + align - 1) & !(align - 1)
    }
}

pub fn parse_archive_symtab(data: &[u8]) -> Option<(Vec<u32>, Vec<String>)> {
    if data.len() < 4 {
        return None;
    }
    let num_syms_be = unsafe { *(data.as_ptr() as *const u32) };
    let num_syms = u32::from_be(num_syms_be) as usize;

    let offsets_byte_len = num_syms * 4;
    if data.len() < 4 + offsets_byte_len {
        return None;
    }

    let mut offsets = Vec::with_capacity(num_syms);
    for i in 0..num_syms {
        let offset_ptr = unsafe { data.as_ptr().add(4 + i * 4) as *const u32 };
        let val_be = unsafe { std::ptr::read_unaligned(offset_ptr) };
        offsets.push(u32::from_be(val_be));
    }

    let strings_start = 4 + offsets_byte_len;
    let mut strings = Vec::with_capacity(num_syms);
    let mut curr = strings_start;

    for _ in 0..num_syms {
        if curr >= data.len() {
            break;
        }
        let mut end = curr;
        while end < data.len() && data[end] != 0 {
            end += 1;
        }
        if let Ok(s) = std::str::from_utf8(&data[curr..end]) {
            strings.push(s.to_string());
        } else {
            strings.push(String::new());
        }
        curr = end + 1;
    }

    Some((offsets, strings))
}

fn extract_strings(bytes: &[u8]) -> Vec<(u64, Vec<u8>)> {
    let mut result = Vec::new();
    let mut start = 0;
    while start < bytes.len() {
        let mut end = start;
        while end < bytes.len() && bytes[end] != 0 {
            end += 1;
        }
        if end < bytes.len() {
            let s = bytes[start..=end].to_vec();
            result.push((start as u64, s));
        }
        start = end + 1;
    }
    result
}

fn skip_uleb128(bytes: &[u8], mut p: usize, end: usize) -> usize {
    while p < end {
        let b = bytes[p];
        p += 1;
        if (b & 0x80) == 0 {
            break;
        }
    }
    p
}

fn skip_sleb128(bytes: &[u8], p: usize, end: usize) -> usize {
    skip_uleb128(bytes, p, end)
}

fn read_uleb128(bytes: &[u8], mut p: usize, end: usize) -> (usize, usize) {
    let mut result = 0;
    let mut shift = 0;
    while p < end {
        let b = bytes[p];
        p += 1;
        result |= ((b & 0x7F) as usize) << shift;
        if (b & 0x80) == 0 {
            break;
        }
        shift += 7;
    }
    (result, p)
}

fn get_encoding_size(enc: u8) -> usize {
    match enc & 0x0F {
        0x00 => 8,
        0x01 => 8,
        0x02 => 2,
        0x03 => 4,
        0x04 => 8,
        0x09 => 2,
        0x0A => 4,
        0x0B => 8,
        _ => 4,
    }
}

pub fn parse_eh_frame_section(
    bytes: &[u8],
    sec_offset: usize,
    sec_size: usize,
    obj_idx: usize,
    sec_idx: usize,
) -> (Vec<CieRecord>, Vec<FdeRecord>) {
    let mut cies = Vec::new();
    let mut fdes = Vec::new();
    let mut offset = sec_offset;
    let sec_end = sec_offset + sec_size;

    while offset < sec_end {
        if offset + 4 > sec_end {
            break;
        }
        let length =
            unsafe { std::ptr::read_unaligned(bytes.as_ptr().add(offset) as *const u32) } as usize;
        if length == 0 {
            offset += 4;
            continue;
        }
        let record_end = offset + 4 + length;
        if record_end > sec_end {
            break;
        }

        let id = unsafe { std::ptr::read_unaligned(bytes.as_ptr().add(offset + 4) as *const u32) };
        if id == 0 {
            let aug_str_offset = offset + 8;
            let mut end = aug_str_offset;
            while end < record_end && bytes[end] != 0 {
                end += 1;
            }
            let mut fde_encoding = 0x1b;
            if let Ok(aug_str) = std::str::from_utf8(&bytes[aug_str_offset..end]) {
                let mut p = end + 1;
                p = skip_uleb128(bytes, p, record_end);
                p = skip_sleb128(bytes, p, record_end);
                p = skip_uleb128(bytes, p, record_end);

                if aug_str.starts_with('z') {
                    let (aug_len, mut aug_ptr) = read_uleb128(bytes, p, record_end);
                    let aug_end = aug_ptr + aug_len;
                    for c in aug_str.chars().skip(1) {
                        if c == 'R' {
                            if aug_ptr < aug_end {
                                fde_encoding = bytes[aug_ptr];
                            }
                            break;
                        }
                        if c == 'L' || c == 'P' {
                            if c == 'P' && aug_ptr < aug_end {
                                let p_enc = bytes[aug_ptr];
                                aug_ptr += 1;
                                aug_ptr += get_encoding_size(p_enc);
                            } else if c == 'L' {
                                aug_ptr += 1;
                            }
                        }
                    }
                }
            }
            cies.push(CieRecord {
                obj_idx,
                input_offset: offset - sec_offset,
                bytes: bytes[offset..record_end].to_vec(),
                fde_encoding,
            });
        } else {
            let cie_input_offset = (offset + 4) - id as usize - sec_offset;
            let pc_begin_offset = offset + 8;
            fdes.push(FdeRecord {
                input_offset: offset - sec_offset,
                cie_input_offset,
                bytes: bytes[offset..record_end].to_vec(),
                pc_begin_offset: pc_begin_offset - offset,
                obj_idx,
                sec_idx,
            });
        }
        offset = record_end;
    }
    (cies, fdes)
}

// =============================================================================
// 4. Представление входных источников и объектов
// =============================================================================

pub enum InputSource {
    File(Mmap),
    ArchiveMember {
        archive_mmap: Arc<Mmap>,
        offset: usize,
        size: usize,
    },
}

impl InputSource {
    pub fn as_slice(&self) -> &[u8] {
        match self {
            InputSource::File(mmap) => mmap,
            InputSource::ArchiveMember {
                archive_mmap,
                offset,
                size,
            } => &archive_mmap[*offset..*offset + *size],
        }
    }
}

pub struct InputObject {
    pub path: PathBuf,
    pub source: InputSource,
    pub header: Elf64_Ehdr,
    pub sections: Vec<Elf64_Shdr>,
}

impl InputObject {
    pub fn from_source(path: PathBuf, source: InputSource) -> Result<Self, String> {
        let bytes = source.as_slice();
        if bytes.len() < std::mem::size_of::<Elf64_Ehdr>() {
            return Err("File is too small to be a valid ELF".to_string());
        }

        let header = unsafe { read_struct::<Elf64_Ehdr>(bytes, 0) }
            .ok_or_else(|| "Failed to read ELF header".to_string())?;

        if header.e_ident[0..4] != ELF_MAGIC {
            return Err("Invalid ELF magic number".to_string());
        }

        let sh_count = header.e_shnum as usize;
        let sh_offset = header.e_shoff as usize;
        let sections_slice = unsafe { read_slice::<Elf64_Shdr>(bytes, sh_offset, sh_count) }
            .ok_or_else(|| "Failed to read section headers".to_string())?;

        let sections = sections_slice.to_vec();

        Ok(InputObject {
            path,
            source,
            header,
            sections,
        })
    }

    pub fn get_section_name(&self, sh_name_offset: u32) -> Option<&str> {
        let strtab_idx = self.header.e_shstrndx as usize;
        if strtab_idx >= self.sections.len() {
            return None;
        }
        let strtab_sec = &self.sections[strtab_idx];
        self.get_string_from_sec(strtab_sec, sh_name_offset)
    }

    pub fn get_string_from_sec(&self, strtab_sec: &Elf64_Shdr, offset: u32) -> Option<&str> {
        if strtab_sec.sh_type != SHT_STRTAB {
            return None;
        }
        let bytes = self.source.as_slice();
        let start = (strtab_sec.sh_offset + offset as u64) as usize;
        if start >= bytes.len() {
            return None;
        }
        let mut end = start;
        while end < bytes.len() && bytes[end] != 0 {
            end += 1;
        }
        std::str::from_utf8(&bytes[start..end]).ok()
    }
}

pub struct Archive {
    pub path: PathBuf,
    pub mmap: Arc<Mmap>,
    pub offsets: Vec<u32>,
    pub symbols: Vec<String>,
    pub loaded_offsets: HashSet<u32>,
}

// =============================================================================
// 5. Логические абстракции для сборки
// =============================================================================

#[derive(Debug, Clone)]
pub struct GlobalSymbol {
    pub name: String,
    pub value: u64,
    pub size: u64,
    pub shndx: u16,
    pub input_obj_idx: usize,
    pub is_weak: bool,
    pub is_synthetic: bool,
}

#[derive(Debug)]
pub struct SectionContribution {
    pub input_obj_idx: usize,
    pub input_sec_idx: usize,
    pub offset_in_output: u64,
    pub size: u64,
}

#[derive(Debug, Clone)]
pub struct MergeRange {
    pub input_start: u64,
    pub input_end: u64,
    pub output_start: u64,
}

#[derive(Debug, Default)]
pub struct OutputSection {
    pub name: String,
    pub sh_type: u32,
    pub sh_flags: u64,
    pub sh_addr: u64,
    pub sh_offset: u64,
    pub sh_size: u64,
    pub sh_addralign: u64,
    pub contributions: Vec<SectionContribution>,

    pub is_merge: bool,
    pub string_pool: Vec<u8>,
    pub string_merge_ranges: HashMap<(usize, usize), Vec<MergeRange>>,
}

#[derive(Debug, Clone)]
pub struct Segment {
    pub p_type: u32,
    pub p_flags: u32,
    pub p_offset: u64,
    pub p_vaddr: u64,
    pub p_paddr: u64,
    pub p_filesz: u64,
    pub p_memsz: u64,
    pub p_align: u64,
}

// =============================================================================
// 6. Контекст линкера
// =============================================================================

pub struct Linker {
    inputs: Vec<InputObject>,
    archives: Vec<Archive>,
    global_symbols: HashMap<String, GlobalSymbol>,
    undefined_symbols: Vec<(String, usize, bool)>,
    output_sections: Vec<OutputSection>,
    segments: Vec<Segment>,
    marked_sections: HashSet<(usize, usize)>,
    got_offset_map: HashMap<String, u64>,
    tls_got_symbols: HashSet<String>,
    eh_frame_cies: Vec<CieRecord>,
    eh_frame_fdes: Vec<FdeRecord>,
}

impl Linker {
    pub fn new() -> Self {
        Linker {
            inputs: Vec::new(),
            archives: Vec::new(),
            global_symbols: HashMap::new(),
            undefined_symbols: Vec::new(),
            output_sections: Vec::new(),
            segments: Vec::new(),
            marked_sections: HashSet::new(),
            got_offset_map: HashMap::new(),
            tls_got_symbols: HashSet::new(),
            eh_frame_cies: Vec::new(),
            eh_frame_fdes: Vec::new(),
        }
    }

    pub fn add_file<P: AsRef<Path>>(&mut self, path: P) -> Result<(), String> {
        let file = File::open(&path).map_err(|e| format!("Failed to open file: {e}"))?;
        let mmap = unsafe { Mmap::map(&file).map_err(|e| format!("Mmap failed: {e}"))? };

        if mmap.starts_with(b"!<arch>\n") {
            println!("Loading archive {:?}", path.as_ref());
            let mmap_arc = Arc::new(mmap);

            if mmap_arc.len() < 8 + 60 {
                return Err("Archive is too small".to_string());
            }

            let hdr = unsafe { read_struct::<ArHeader>(&mmap_arc, 8) }
                .ok_or_else(|| "Failed to read first archive header".to_string())?;

            let size_str = std::str::from_utf8(&hdr.ar_size)
                .map_err(|_| "Invalid ASCII in ar_size")?
                .trim();
            let size = size_str
                .parse::<usize>()
                .map_err(|e| format!("Failed to parse ar_size: {e}"))?;

            if hdr.ar_name.starts_with(b"/               ") {
                let data_start = 8 + 60;
                let symtab_data = &mmap_arc[data_start..data_start + size];
                if let Some((offsets, symbols)) = parse_archive_symtab(symtab_data) {
                    self.archives.push(Archive {
                        path: path.as_ref().to_path_buf(),
                        mmap: mmap_arc,
                        offsets,
                        symbols,
                        loaded_offsets: HashSet::new(),
                    });
                } else {
                    return Err("Failed to parse archive symbol table".to_string());
                }
            } else {
                return Err("First archive member is not a SVR4 symbol table (/)".to_string());
            }
        } else {
            let obj =
                InputObject::from_source(path.as_ref().to_path_buf(), InputSource::File(mmap))?;
            self.inputs.push(obj);
        }
        Ok(())
    }

    fn load_archive_member(&self, arc_idx: usize, offset: usize) -> Result<InputObject, String> {
        let archive = &self.archives[arc_idx];

        if offset + 60 > archive.mmap.len() {
            return Err(format!("Archive member offset {} out of bounds", offset));
        }

        let hdr = unsafe { read_struct::<ArHeader>(&archive.mmap, offset) }
            .ok_or_else(|| "Failed to read member archive header".to_string())?;

        let size_str = std::str::from_utf8(&hdr.ar_size)
            .map_err(|_| "Invalid ASCII in ar_size")?
            .trim();
        let size = size_str
            .parse::<usize>()
            .map_err(|e| format!("Failed to parse ar_size `{size_str}`: {e}"))?;

        if offset + 60 + size > archive.mmap.len() {
            return Err(format!(
                "Archive member size out of bounds at offset {}",
                offset
            ));
        }

        let mut name_str = std::str::from_utf8(&hdr.ar_name)
            .map_err(|_| "Invalid ASCII in ar_name")?
            .trim()
            .to_string();

        if let Some(slash_idx) = name_str.find('/') {
            name_str.truncate(slash_idx);
        }

        let path = archive.path.join(name_str);

        let source = InputSource::ArchiveMember {
            archive_mmap: Arc::clone(&archive.mmap),
            offset: offset + 60,
            size,
        };

        InputObject::from_source(path, source)
    }

    fn resolve_symbols_internal(&mut self) -> Result<(), String> {
        self.global_symbols.clear();
        self.undefined_symbols.clear();

        for (obj_idx, input) in self.inputs.iter().enumerate() {
            let bytes = input.source.as_slice();
            for sec in &input.sections {
                if sec.sh_type == SHT_SYMTAB {
                    let count = (sec.sh_size / sec.sh_entsize) as usize;
                    let syms =
                        unsafe { read_slice::<Elf64_Sym>(bytes, sec.sh_offset as usize, count) }
                            .ok_or("Failed to read symbol slice")?;

                    let strtab_idx = sec.sh_link as usize;
                    if strtab_idx >= input.sections.len() {
                        return Err("Invalid symtab sh_link index".to_string());
                    }
                    let strtab_sec = &input.sections[strtab_idx];

                    for sym in syms {
                        let bind = sym.st_info >> 4;
                        let is_weak = bind == STB_WEAK;

                        if bind == STB_GLOBAL || is_weak {
                            let name = input
                                .get_string_from_sec(strtab_sec, sym.st_name)
                                .unwrap_or("")
                                .to_string();

                            if name.is_empty() {
                                continue;
                            }

                            if sym.st_shndx == SHN_UNDEF {
                                self.undefined_symbols.push((name, obj_idx, is_weak));
                            } else {
                                let should_override = match self.global_symbols.get(&name) {
                                    None => true,
                                    Some(existing) => existing.is_weak && !is_weak,
                                };

                                if should_override {
                                    self.global_symbols.insert(
                                        name.clone(),
                                        GlobalSymbol {
                                            name,
                                            value: sym.st_value,
                                            size: sym.st_size,
                                            shndx: sym.st_shndx,
                                            input_obj_idx: obj_idx,
                                            is_weak,
                                            is_synthetic: false,
                                        },
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub fn resolve_symbols(&mut self) -> Result<(), String> {
        self.resolve_symbols_internal()?;

        loop {
            let mut newly_loaded = false;

            let mut active_undefined = HashSet::new();
            for (undef, _, is_weak) in &self.undefined_symbols {
                if !self.global_symbols.contains_key(undef) && !is_weak {
                    active_undefined.insert(undef.clone());
                }
            }

            if active_undefined.is_empty() {
                break;
            }

            for arc_idx in 0..self.archives.len() {
                let mut to_load_offsets = Vec::new();
                {
                    let archive = &self.archives[arc_idx];
                    for (i, sym_name) in archive.symbols.iter().enumerate() {
                        if active_undefined.contains(sym_name) {
                            let offset = archive.offsets[i];
                            if !archive.loaded_offsets.contains(&offset) {
                                to_load_offsets.push(offset);
                            }
                        }
                    }
                }

                for offset in to_load_offsets {
                    println!(
                        "  Extracting member from archive {:?} at offset {} to satisfy dependency",
                        self.archives[arc_idx].path, offset
                    );
                    let obj = self.load_archive_member(arc_idx, offset as usize)?;
                    self.inputs.push(obj);
                    self.archives[arc_idx].loaded_offsets.insert(offset);
                    newly_loaded = true;
                }
            }

            if !newly_loaded {
                break;
            }

            self.resolve_symbols_internal()?;
        }

        let mut missing = Vec::new();
        for (undef_name, obj_idx, is_weak) in &self.undefined_symbols {
            if !self.global_symbols.contains_key(undef_name) && !is_weak {
                missing.push(format!(
                    "Undefined reference to `{}` in file {:?}",
                    undef_name, self.inputs[*obj_idx].path
                ));
            }
        }

        if !missing.is_empty() {
            return Err(missing.join("\n"));
        }

        println!("  All global symbols successfully resolved.");
        Ok(())
    }

    fn get_symbol_name(&self, obj_idx: usize, sym_idx: usize) -> Option<String> {
        let input = &self.inputs[obj_idx];
        let symtab_sec = input.sections.iter().find(|s| s.sh_type == SHT_SYMTAB)?;
        let syms = unsafe {
            read_slice::<Elf64_Sym>(
                input.source.as_slice(),
                symtab_sec.sh_offset as usize,
                (symtab_sec.sh_size / symtab_sec.sh_entsize) as usize,
            )
        }?;
        let sym = syms.get(sym_idx)?;
        let strtab_sec = &input.sections[symtab_sec.sh_link as usize];
        input
            .get_string_from_sec(strtab_sec, sym.st_name)
            .map(|s| s.to_string())
    }

    pub fn gc_sections(&mut self) -> Result<(), String> {
        println!("Performing section garbage collection (--gc-sections)...");

        let mut marked = HashSet::new();
        let mut queue = Vec::new();

        if let Some(entry_sym) = self.global_symbols.get("_start") {
            let root = (entry_sym.input_obj_idx, entry_sym.shndx as usize);
            marked.insert(root);
            queue.push(root);
        }

        for (obj_idx, input) in self.inputs.iter().enumerate() {
            for (sec_idx, sec) in input.sections.iter().enumerate() {
                if let Some(name) = input.get_section_name(sec.sh_name) {
                    if name == ".init_array" || name == ".fini_array" || name == ".preinit_array" {
                        let root = (obj_idx, sec_idx);
                        if marked.insert(root) {
                            queue.push(root);
                        }
                    }
                }
            }
        }

        while let Some((obj_idx, sec_idx)) = queue.pop() {
            let input = &self.inputs[obj_idx];
            let bytes = input.source.as_slice();

            for rel_sec in &input.sections {
                if rel_sec.sh_type == SHT_RELA && rel_sec.sh_info as usize == sec_idx {
                    let count = (rel_sec.sh_size / rel_sec.sh_entsize) as usize;
                    if let Some(relas) = unsafe {
                        read_slice::<Elf64_Rela>(bytes, rel_sec.sh_offset as usize, count)
                    } {
                        for rela in relas {
                            let sym_idx = (rela.r_info >> 32) as usize;
                            if let Some(def_sec) =
                                self.resolve_symbol_definition_section(obj_idx, sym_idx)
                            {
                                if marked.insert(def_sec) {
                                    queue.push(def_sec);
                                }
                            }
                        }
                    }
                }
            }
        }

        self.marked_sections = marked;
        println!(
            "  GC complete. Kept {} sections.",
            self.marked_sections.len()
        );
        Ok(())
    }

    fn resolve_symbol_definition_section(
        &self,
        obj_idx: usize,
        sym_idx: usize,
    ) -> Option<(usize, usize)> {
        let input = &self.inputs[obj_idx];
        let bytes = input.source.as_slice();
        let symtab_sec = input.sections.iter().find(|s| s.sh_type == SHT_SYMTAB)?;
        let syms = unsafe {
            read_slice::<Elf64_Sym>(
                bytes,
                symtab_sec.sh_offset as usize,
                (symtab_sec.sh_size / symtab_sec.sh_entsize) as usize,
            )
        }?;
        let sym = syms.get(sym_idx)?;

        let bind = sym.st_info >> 4;
        if bind == STB_GLOBAL || bind == STB_WEAK {
            let strtab_sec = &input.sections[symtab_sec.sh_link as usize];
            let name = input.get_string_from_sec(strtab_sec, sym.st_name)?;
            if let Some(global_sym) = self.global_symbols.get(name) {
                if global_sym.shndx != SHN_UNDEF && global_sym.shndx != SHN_COMMON {
                    return Some((global_sym.input_obj_idx, global_sym.shndx as usize));
                }
            }
        }

        if sym.st_shndx != SHN_UNDEF && sym.st_shndx != SHN_COMMON {
            return Some((obj_idx, sym.st_shndx as usize));
        }
        None
    }

    fn define_linker_symbols(&mut self) {
        let init_array_sec = self
            .output_sections
            .iter()
            .find(|s| s.name == ".init_array");
        let fini_array_sec = self
            .output_sections
            .iter()
            .find(|s| s.name == ".fini_array");

        let mut add_sym = |name: &str, addr: u64| {
            self.global_symbols.insert(
                name.to_string(),
                GlobalSymbol {
                    name: name.to_string(),
                    value: addr,
                    size: 0,
                    shndx: 0,
                    input_obj_idx: 0,
                    is_weak: false,
                    is_synthetic: true,
                },
            );
        };

        if let Some(sec) = init_array_sec {
            add_sym("__init_array_start", sec.sh_addr);
            add_sym("__init_array_end", sec.sh_addr + sec.sh_size);
        } else {
            add_sym("__init_array_start", 0);
            add_sym("__init_array_end", 0);
        }

        if let Some(sec) = fini_array_sec {
            add_sym("__fini_array_start", sec.sh_addr);
            add_sym("__fini_array_end", sec.sh_addr + sec.sh_size);
        } else {
            add_sym("__fini_array_start", 0);
            add_sym("__fini_array_end", 0);
        }

        let rw_seg = self.segments.iter().find(|seg| (seg.p_flags & PF_W) != 0);
        if let Some(seg) = rw_seg {
            add_sym("_edata", seg.p_vaddr + seg.p_filesz);
            add_sym("_end", seg.p_vaddr + seg.p_memsz);
        } else {
            add_sym("_edata", 0);
            add_sym("_end", 0);
        }
    }

    fn resolve_fde_func_address(&self, fde: &FdeRecord) -> Option<(u64, usize, usize)> {
        let input = &self.inputs[fde.obj_idx];
        let rela_sec = input
            .sections
            .iter()
            .find(|s| s.sh_type == SHT_RELA && s.sh_info as usize == fde.sec_idx)?;
        let count = (rela_sec.sh_size / rela_sec.sh_entsize) as usize;
        let relas = unsafe {
            read_slice::<Elf64_Rela>(input.source.as_slice(), rela_sec.sh_offset as usize, count)
        }?;

        let target_offset = (fde.input_offset + fde.pc_begin_offset) as u64;
        let rela = relas.iter().find(|r| r.r_offset == target_offset)?;
        let sym_idx = (rela.r_info >> 32) as usize;

        let s = self.resolve_symbol_address(fde.obj_idx, sym_idx, rela.r_addend)?;
        let def_sec = self.resolve_symbol_definition_section(fde.obj_idx, sym_idx)?;

        Some((s, def_sec.0, def_sec.1))
    }

    /// Фаза 2: Вычисление макета выходного файла (Layout)
    pub fn compute_layout(&mut self) -> Result<(), String> {
        self.gc_sections()?;

        println!("Computing output sections layout...");

        // 1. Сборка CIE/FDE записей из .eh_frame
        let mut cies = Vec::new();
        let mut fdes = Vec::new();
        for (obj_idx, input) in self.inputs.iter().enumerate() {
            for (sec_idx, sec) in input.sections.iter().enumerate() {
                if let Some(name) = input.get_section_name(sec.sh_name) {
                    if name == ".eh_frame" {
                        let (obj_cies, obj_fdes) = parse_eh_frame_section(
                            input.source.as_slice(),
                            sec.sh_offset as usize,
                            sec.sh_size as usize,
                            obj_idx,
                            sec_idx,
                        );
                        cies.extend(obj_cies);
                        fdes.extend(obj_fdes);
                    }
                }
            }
        }

        let mut kept_fdes = Vec::new();
        let mut kept_cies = HashSet::new();
        for fde in fdes {
            if let Some((_func_addr, obj_idx, sec_idx)) = self.resolve_fde_func_address(&fde) {
                if self.marked_sections.contains(&(obj_idx, sec_idx)) {
                    kept_fdes.push(fde.clone());
                    kept_cies.insert((fde.obj_idx, fde.cie_input_offset));
                }
            }
        }

        let mut kept_cies_list = Vec::new();
        for cie in cies {
            if kept_cies.contains(&(cie.obj_idx, cie.input_offset)) {
                kept_cies_list.push(cie);
            }
        }

        self.eh_frame_cies = kept_cies_list;
        self.eh_frame_fdes = kept_fdes;

        let eh_frame_size = self
            .eh_frame_cies
            .iter()
            .map(|c| c.bytes.len())
            .sum::<usize>()
            + self
                .eh_frame_fdes
                .iter()
                .map(|f| f.bytes.len())
                .sum::<usize>()
            + 4;
        let eh_frame_hdr_size = 12 + self.eh_frame_fdes.len() * 8;

        // 2. Аллокация SHN_COMMON символов (неинициализированные переменные C)
        let mut common_symbols = Vec::new();
        for sym in self.global_symbols.values() {
            if sym.shndx == SHN_COMMON {
                common_symbols.push(sym.clone());
            }
        }

        let mut common_size: u64 = 0;
        let mut common_align: u64 = 1;
        let mut common_offsets = HashMap::new();

        for sym in &common_symbols {
            let align = sym.value;
            if align > common_align {
                common_align = align;
            }
            common_size = align_up(common_size, align);
            common_offsets.insert(sym.name.clone(), common_size);
            common_size += sym.size;
        }

        // 3. Сканирование релокаций для резервирования GOT и TLS-слотов
        let mut got_symbols = HashSet::new();
        let mut tls_got_symbols = HashSet::new();
        for (obj_idx, input) in self.inputs.iter().enumerate() {
            for sec in &input.sections {
                if sec.sh_type == SHT_RELA {
                    let target_sec_idx = sec.sh_info as usize;
                    if !self.marked_sections.contains(&(obj_idx, target_sec_idx)) {
                        continue;
                    }
                    let count = (sec.sh_size / sec.sh_entsize) as usize;
                    if let Some(relas) = unsafe {
                        read_slice::<Elf64_Rela>(
                            input.source.as_slice(),
                            sec.sh_offset as usize,
                            count,
                        )
                    } {
                        for rela in relas {
                            let rel_type = (rela.r_info & 0xFFFFFFFF) as u32;
                            if rel_type == R_X86_64_GOTPCREL
                                || rel_type == R_X86_64_GOTPCRELX
                                || rel_type == R_X86_64_REX_GOTPCRELX
                                || rel_type == R_X86_64_GOTTPOFF
                            {
                                let sym_idx = (rela.r_info >> 32) as usize;
                                if let Some(name) = self.get_symbol_name(obj_idx, sym_idx) {
                                    got_symbols.insert(name.clone());
                                    if rel_type == R_X86_64_GOTTPOFF {
                                        tls_got_symbols.insert(name);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        self.tls_got_symbols = tls_got_symbols;

        let mut grouped_sections: HashMap<String, (u32, u64, u64)> = HashMap::new();

        // Регистрируем классические секции
        for (obj_idx, input) in self.inputs.iter().enumerate() {
            for (sec_idx, sec) in input.sections.iter().enumerate() {
                if !self.marked_sections.contains(&(obj_idx, sec_idx)) {
                    continue;
                }
                if let Some(name) = input.get_section_name(sec.sh_name) {
                    if name == ".eh_frame" {
                        continue;
                    }
                    if (sec.sh_flags & SHF_ALLOC) != 0 {
                        let entry = grouped_sections.entry(name.to_string()).or_insert((
                            sec.sh_type,
                            sec.sh_flags,
                            1,
                        ));
                        if sec.sh_addralign > entry.2 {
                            entry.2 = sec.sh_addralign;
                        }
                    }
                }
            }
        }

        if common_size > 0 {
            let entry =
                grouped_sections
                    .entry(".bss".to_string())
                    .or_insert((8, SHF_ALLOC | SHF_WRITE, 1));
            if common_align > entry.2 {
                entry.2 = common_align;
            }
        }

        self.output_sections.clear();

        let mut common_start_in_bss: u64 = 0;

        for (sec_name, (sh_type, sh_flags, max_align)) in grouped_sections {
            let mut out_sec = OutputSection {
                name: sec_name.clone(),
                sh_type,
                sh_flags,
                sh_addr: 0,
                sh_offset: 0,
                sh_size: 0,
                sh_addralign: max_align,
                contributions: Vec::new(),
                is_merge: false,
                string_pool: Vec::new(),
                string_merge_ranges: HashMap::new(),
            };

            let is_merge = (sh_flags & SHF_MERGE) != 0 && (sh_flags & SHF_STRINGS) != 0;

            if is_merge {
                out_sec.is_merge = true;
            } else {
                let mut sec_size: u64 = 0;
                for (obj_idx, input) in self.inputs.iter().enumerate() {
                    for (sec_idx, sec) in input.sections.iter().enumerate() {
                        if !self.marked_sections.contains(&(obj_idx, sec_idx)) {
                            continue;
                        }
                        if (sec.sh_flags & SHF_ALLOC) != 0 {
                            if let Some(name) = input.get_section_name(sec.sh_name) {
                                if name == sec_name {
                                    let align = sec.sh_addralign;
                                    if align > 1 {
                                        sec_size = align_up(sec_size, align);
                                    }

                                    out_sec.contributions.push(SectionContribution {
                                        input_obj_idx: obj_idx,
                                        input_sec_idx: sec_idx,
                                        offset_in_output: sec_size,
                                        size: sec.sh_size,
                                    });

                                    sec_size += sec.sh_size;
                                }
                            }
                        }
                    }
                }

                if sec_name == ".bss" && common_size > 0 {
                    let common_start = align_up(sec_size, common_align);
                    sec_size = common_start + common_size;
                    common_start_in_bss = common_start;
                }

                out_sec.sh_size = sec_size;
            }
            self.output_sections.push(out_sec);
        }

        // Слияние и дедупликация строк
        for out_sec in &mut self.output_sections {
            if out_sec.is_merge {
                let mut pool = Vec::new();
                let mut string_to_offset: HashMap<Vec<u8>, u64> = HashMap::new();

                for (obj_idx, input) in self.inputs.iter().enumerate() {
                    for (sec_idx, sec) in input.sections.iter().enumerate() {
                        if !self.marked_sections.contains(&(obj_idx, sec_idx)) {
                            continue;
                        }
                        if (sec.sh_flags & SHF_MERGE) != 0 && (sec.sh_flags & SHF_STRINGS) != 0 {
                            if let Some(name) = input.get_section_name(sec.sh_name) {
                                if name == out_sec.name {
                                    let sec_bytes = &input.source.as_slice()[sec.sh_offset as usize
                                        ..(sec.sh_offset + sec.sh_size) as usize];
                                    let strings = extract_strings(sec_bytes);
                                    for (input_offset, s) in strings {
                                        let out_offset = if let Some(&existing_offset) =
                                            string_to_offset.get(&s)
                                        {
                                            existing_offset
                                        } else {
                                            let new_offset = pool.len() as u64;
                                            pool.extend_from_slice(&s);
                                            string_to_offset.insert(s.clone(), new_offset);
                                            new_offset
                                        };
                                        ranges_push(
                                            &mut out_sec.string_merge_ranges,
                                            obj_idx,
                                            sec_idx,
                                            MergeRange {
                                                input_start: input_offset,
                                                input_end: input_offset + s.len() as u64,
                                                output_start: out_offset,
                                            },
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
                out_sec.string_pool = pool;
                out_sec.sh_size = out_sec.string_pool.len() as u64;
            }
        }

        if eh_frame_size > 4 {
            let eh_frame_sec = OutputSection {
                name: ".eh_frame".to_string(),
                sh_type: 1,
                sh_flags: SHF_ALLOC,
                sh_addr: 0,
                sh_offset: 0,
                sh_size: eh_frame_size as u64,
                sh_addralign: 8,
                contributions: Vec::new(),
                is_merge: false,
                string_pool: Vec::new(),
                string_merge_ranges: HashMap::new(),
            };
            self.output_sections.push(eh_frame_sec);

            let eh_frame_hdr_sec = OutputSection {
                name: ".eh_frame_hdr".to_string(),
                sh_type: 1,
                sh_flags: SHF_ALLOC,
                sh_addr: 0,
                sh_offset: 0,
                sh_size: eh_frame_hdr_size as u64,
                sh_addralign: 4,
                contributions: Vec::new(),
                is_merge: false,
                string_pool: Vec::new(),
                string_merge_ranges: HashMap::new(),
            };
            self.output_sections.push(eh_frame_hdr_sec);
        }

        let mut got_offset_map = HashMap::new();
        if !got_symbols.is_empty() {
            let got_sec = OutputSection {
                name: ".got".to_string(),
                sh_type: 1,
                sh_flags: SHF_ALLOC | SHF_WRITE,
                sh_addr: 0,
                sh_offset: 0,
                sh_size: (got_symbols.len() * 8) as u64,
                sh_addralign: 8,
                contributions: Vec::new(),
                is_merge: false,
                string_pool: Vec::new(),
                string_merge_ranges: HashMap::new(),
            };

            for (idx, name) in got_symbols.iter().enumerate() {
                got_offset_map.insert(name.clone(), (idx * 8) as u64);
            }

            self.output_sections.push(got_sec);
        }
        self.got_offset_map = got_offset_map;

        self.output_sections
            .sort_by_key(|s| (s.sh_flags & SHF_WRITE) != 0);

        let mut current_file_offset: u64 = PAGE_SIZE;
        let mut current_vaddr: u64 = 0x400000 + current_file_offset;

        let mut rx_end_offset = current_file_offset;
        let mut rx_end_vaddr = current_vaddr;

        let mut rw_start_offset = 0;
        let mut rw_start_vaddr = 0;
        let mut rw_end_offset = 0;
        let mut rw_end_vaddr = 0;

        let mut transitioned_to_rw = false;

        for out_sec in &mut self.output_sections {
            if out_sec.sh_size == 0 {
                continue;
            }

            let is_rw = (out_sec.sh_flags & SHF_WRITE) != 0;

            if is_rw && !transitioned_to_rw {
                rx_end_offset = current_file_offset;
                rx_end_vaddr = current_vaddr;

                current_file_offset = align_up(current_file_offset, PAGE_SIZE);
                current_vaddr = align_up(current_vaddr, PAGE_SIZE);

                rw_start_offset = current_file_offset;
                rw_start_vaddr = current_vaddr;
                transitioned_to_rw = true;
            }

            let align = out_sec.sh_addralign;
            if align > 1 {
                current_file_offset = align_up(current_file_offset, align);
                current_vaddr = align_up(current_vaddr, align);
            }

            out_sec.sh_offset = current_file_offset;
            out_sec.sh_addr = current_vaddr;

            if out_sec.sh_type == 8 {
                current_vaddr += out_sec.sh_size;
            } else {
                current_file_offset += out_sec.sh_size;
                current_vaddr += out_sec.sh_size;
            }

            println!(
                "  Section {:<15}: offset=0x{:08x}, vaddr=0x{:08x}, size={:>6} bytes",
                out_sec.name, out_sec.sh_offset, out_sec.sh_addr, out_sec.sh_size
            );
        }

        if !transitioned_to_rw {
            rx_end_offset = current_file_offset;
            rx_end_vaddr = current_vaddr;
        } else {
            rw_end_offset = current_file_offset;
            rw_end_vaddr = current_vaddr;
        }

        self.segments.clear();

        self.segments.push(Segment {
            p_type: PT_LOAD,
            p_flags: PF_R | PF_X,
            p_offset: 0,
            p_vaddr: 0x400000,
            p_paddr: 0x400000,
            p_filesz: rx_end_offset,
            p_memsz: rx_end_vaddr - 0x400000,
            p_align: PAGE_SIZE,
        });

        if transitioned_to_rw {
            self.segments.push(Segment {
                p_type: PT_LOAD,
                p_flags: PF_R | PF_W,
                p_offset: rw_start_offset,
                p_vaddr: rw_start_vaddr,
                p_paddr: rw_start_vaddr,
                p_filesz: rw_end_offset - rw_start_offset,
                p_memsz: rw_end_vaddr - rw_start_vaddr,
                p_align: PAGE_SIZE,
            });
        }

        let tdata_sec = self.output_sections.iter().find(|s| s.name == ".tdata");
        let tbss_sec = self.output_sections.iter().find(|s| s.name == ".tbss");
        if tdata_sec.is_some() || tbss_sec.is_some() {
            let mut tls_offset = 0;
            let mut tls_vaddr = 0;
            let mut tls_filesz = 0;
            let mut tls_memsz = 0;
            let mut tls_align = 1;

            if let Some(sec) = tdata_sec {
                tls_offset = sec.sh_offset;
                tls_vaddr = sec.sh_addr;
                tls_filesz = sec.sh_size;
                tls_memsz = sec.sh_size;
                tls_align = sec.sh_addralign;
            }
            if let Some(sec) = tbss_sec {
                if tls_vaddr == 0 {
                    tls_offset = sec.sh_offset;
                    tls_vaddr = sec.sh_addr;
                }
                tls_memsz += sec.sh_size;
                if sec.sh_addralign > tls_align {
                    tls_align = sec.sh_addralign;
                }
            }

            self.segments.push(Segment {
                p_type: PT_TLS,
                p_flags: PF_R,
                p_offset: tls_offset,
                p_vaddr: tls_vaddr,
                p_paddr: tls_vaddr,
                p_filesz: tls_filesz,
                p_memsz: tls_memsz,
                p_align: tls_align,
            });
        }

        if let Some(eh_hdr) = self
            .output_sections
            .iter()
            .find(|s| s.name == ".eh_frame_hdr")
        {
            self.segments.push(Segment {
                p_type: PT_GNU_EH_FRAME,
                p_flags: PF_R,
                p_offset: eh_hdr.sh_offset,
                p_vaddr: eh_hdr.sh_addr,
                p_paddr: eh_hdr.sh_addr,
                p_filesz: eh_hdr.sh_size,
                p_memsz: eh_hdr.sh_size,
                p_align: eh_hdr.sh_addralign,
            });
        }

        self.segments.push(Segment {
            p_type: PT_GNU_STACK,
            p_flags: PF_R | PF_W,
            p_offset: 0,
            p_vaddr: 0,
            p_paddr: 0,
            p_filesz: 0,
            p_memsz: 0,
            p_align: 8,
        });

        if common_size > 0 {
            if let Some(bss_sec) = self.output_sections.iter().find(|s| s.name == ".bss") {
                let base_addr = bss_sec.sh_addr + common_start_in_bss;
                for sym_name in common_offsets.keys() {
                    if let Some(sym) = self.global_symbols.get_mut(sym_name) {
                        let offset = common_offsets[sym_name];
                        sym.value = base_addr + offset;
                        sym.is_synthetic = true;
                    }
                }
            }
        }

        self.define_linker_symbols();

        Ok(())
    }

    /// Фаза 3: Генерация выходного бинаря и применение релокаций
    pub fn write_output<P: AsRef<Path>>(&self, output_path: P) -> Result<(), String> {
        println!("Writing output file and applying relocations...");

        let total_file_size = self
            .segments
            .iter()
            .map(|seg| seg.p_offset + seg.p_filesz)
            .max()
            .unwrap_or(PAGE_SIZE) as usize;

        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&output_path)
            .map_err(|e| format!("Failed to create output file: {e}"))?;
        file.set_len(total_file_size as u64)
            .map_err(|e| format!("Failed to set output file size: {e}"))?;

        let mut mmap =
            unsafe { MmapMut::map_mut(&file).map_err(|e| format!("MmapMut failed: {e}"))? };

        // 1. Запись ELF Header
        let mut ehdr = Elf64_Ehdr {
            e_ident: [0; 16],
            e_type: 2,
            e_machine: 0x3E,
            e_version: 1,
            e_entry: 0,
            e_phoff: 64,
            e_shoff: 0,
            e_flags: 0,
            e_ehsize: 64,
            e_phentsize: 56,
            e_phnum: self.segments.len() as u16,
            e_shentsize: 64,
            e_shnum: 0,
            e_shstrndx: 0,
        };
        ehdr.e_ident[0..4].copy_from_slice(&ELF_MAGIC);
        ehdr.e_ident[4] = 2;
        ehdr.e_ident[5] = 1;
        ehdr.e_ident[6] = 1;
        ehdr.e_ident[7] = 0;

        if let Some(entry_sym) = self.global_symbols.get("_start") {
            if let Some(entry_vaddr) = self.get_global_symbol_vaddr(entry_sym) {
                ehdr.e_entry = entry_vaddr;
            }
        }

        if ehdr.e_entry == 0 {
            if let Some(text_sec) = self.output_sections.iter().find(|s| s.name == ".text") {
                ehdr.e_entry = text_sec.sh_addr;
            } else {
                return Err("Could not find `_start` or `.text` to set entry point".to_string());
            }
        }

        unsafe {
            std::ptr::write_unaligned(mmap.as_mut_ptr() as *mut Elf64_Ehdr, ehdr);
        }

        // 2. Запись Program Headers
        for (i, seg) in self.segments.iter().enumerate() {
            let phdr = Elf64_Phdr {
                p_type: seg.p_type,
                p_flags: seg.p_flags,
                p_offset: seg.p_offset,
                p_vaddr: seg.p_vaddr,
                p_paddr: seg.p_paddr,
                p_filesz: seg.p_filesz,
                p_memsz: seg.p_memsz,
                p_align: seg.p_align,
            };
            let phdr_offset = 64 + i * 56;
            unsafe {
                std::ptr::write_unaligned(
                    mmap.as_mut_ptr().add(phdr_offset) as *mut Elf64_Phdr,
                    phdr,
                );
            }
        }

        // 3. Копирование сырых данных секций
        for out_sec in &self.output_sections {
            if out_sec.sh_size == 0 {
                continue;
            }
            if out_sec.sh_type == 8 {
                continue;
            }
            if out_sec.name == ".eh_frame" || out_sec.name == ".eh_frame_hdr" {
                continue;
            }
            if out_sec.is_merge {
                let dest_offset = out_sec.sh_offset as usize;
                let size = out_sec.string_pool.len();
                mmap[dest_offset..dest_offset + size].copy_from_slice(&out_sec.string_pool);
            } else {
                for contrib in &out_sec.contributions {
                    let input = &self.inputs[contrib.input_obj_idx];
                    let input_sec = &input.sections[contrib.input_sec_idx];
                    let src_offset = input_sec.sh_offset as usize;
                    let dest_offset = (out_sec.sh_offset + contrib.offset_in_output) as usize;
                    let size = contrib.size as usize;

                    let src_bytes = input.source.as_slice();
                    mmap[dest_offset..dest_offset + size]
                        .copy_from_slice(&src_bytes[src_offset..src_offset + size]);
                }
            }
        }

        // 4. Компиляция и запись дедуплицированной .eh_frame и генерация индексной .eh_frame_hdr
        if let Some(eh_sec) = self.output_sections.iter().find(|s| s.name == ".eh_frame") {
            if let Some(eh_hdr_sec) = self
                .output_sections
                .iter()
                .find(|s| s.name == ".eh_frame_hdr")
            {
                let mut eh_bytes = Vec::new();
                let mut cie_map = HashMap::new();

                for cie in &self.eh_frame_cies {
                    let out_offset = eh_bytes.len();
                    cie_map.insert((cie.obj_idx, cie.input_offset), out_offset);
                    eh_bytes.extend_from_slice(&cie.bytes);
                }

                let mut fde_table = Vec::new();

                for fde in &self.eh_frame_fdes {
                    let fde_out_offset = eh_bytes.len();
                    let mut fde_bytes = fde.bytes.clone();

                    let func_addr = self.resolve_fde_func_address(fde).map(|x| x.0).unwrap_or(0);
                    let cie_out_offset = *cie_map
                        .get(&(fde.obj_idx, fde.cie_input_offset))
                        .unwrap_or(&0);

                    let new_cie_ptr = (fde_out_offset + 4 - cie_out_offset) as u32;
                    unsafe {
                        std::ptr::write_unaligned(
                            fde_bytes.as_mut_ptr().add(4) as *mut u32,
                            new_cie_ptr,
                        );
                    }

                    let pc_begin_vaddr =
                        eh_sec.sh_addr + fde_out_offset as u64 + fde.pc_begin_offset as u64;
                    let cie = self.eh_frame_cies.iter().find(|c| {
                        c.obj_idx == fde.obj_idx && c.input_offset == fde.cie_input_offset
                    });
                    let encoding = cie.map(|c| c.fde_encoding).unwrap_or(0x1b);

                    let rel_val = func_addr as i64 - pc_begin_vaddr as i64;

                    unsafe {
                        let ptr_ptr = fde_bytes.as_mut_ptr().add(fde.pc_begin_offset);
                        match encoding & 0x0F {
                            0x02 => {
                                std::ptr::write_unaligned(ptr_ptr as *mut i16, rel_val as i16);
                            }
                            0x03 | 0x0A => {
                                std::ptr::write_unaligned(ptr_ptr as *mut i32, rel_val as i32);
                            }
                            0x04 | 0x0B => {
                                std::ptr::write_unaligned(ptr_ptr as *mut i64, rel_val as i64);
                            }
                            _ => {
                                std::ptr::write_unaligned(ptr_ptr as *mut i32, rel_val as i32);
                            }
                        }
                    }

                    fde_table.push((func_addr, eh_sec.sh_addr + fde_out_offset as u64));
                    eh_bytes.extend_from_slice(&fde_bytes);
                }

                eh_bytes.extend_from_slice(&[0, 0, 0, 0]);

                mmap[eh_sec.sh_offset as usize..eh_sec.sh_offset as usize + eh_bytes.len()]
                    .copy_from_slice(&eh_bytes);

                let mut hdr_bytes = vec![0u8; 12];
                hdr_bytes[0] = 1;
                hdr_bytes[1] = 0x1b;
                hdr_bytes[2] = 0x03;
                hdr_bytes[3] = 0x3b;

                let eh_frame_ptr_val = eh_sec.sh_addr as i64 - eh_hdr_sec.sh_addr as i64;
                unsafe {
                    std::ptr::write_unaligned(
                        hdr_bytes.as_mut_ptr().add(4) as *mut i32,
                        eh_frame_ptr_val as i32,
                    );
                    std::ptr::write_unaligned(
                        hdr_bytes.as_mut_ptr().add(8) as *mut u32,
                        fde_table.len() as u32,
                    );
                }

                fde_table.sort_by_key(|x| x.0);

                for (func_vaddr, fde_vaddr) in fde_table {
                    let init_loc = func_vaddr as i64 - eh_hdr_sec.sh_addr as i64;
                    let fde_ptr = fde_vaddr as i64 - eh_hdr_sec.sh_addr as i64;

                    let mut entry = [0u8; 8];
                    unsafe {
                        std::ptr::write_unaligned(entry.as_mut_ptr() as *mut i32, init_loc as i32);
                        std::ptr::write_unaligned(
                            entry.as_mut_ptr().add(4) as *mut i32,
                            fde_ptr as i32,
                        );
                    }
                    hdr_bytes.extend_from_slice(&entry);
                }

                mmap[eh_hdr_sec.sh_offset as usize
                    ..eh_hdr_sec.sh_offset as usize + hdr_bytes.len()]
                    .copy_from_slice(&hdr_bytes);
            }
        }

        // 5. Заполнение GOT-таблицы (включая TPOFF-вычисления для Thread Local)
        if let Some(got_sec) = self.output_sections.iter().find(|s| s.name == ".got") {
            for (name, got_offset) in &self.got_offset_map {
                let resolved_addr = if let Some(sym) = self.global_symbols.get(name) {
                    self.get_global_symbol_vaddr(sym).unwrap_or(0)
                } else {
                    0
                };

                let val = if self.tls_got_symbols.contains(name) {
                    if let Some(tls_seg) = self.segments.iter().find(|seg| seg.p_type == PT_TLS) {
                        let tls_end = tls_seg.p_vaddr + tls_seg.p_memsz;
                        (resolved_addr as i64 - tls_end as i64) as u64
                    } else {
                        resolved_addr
                    }
                } else {
                    resolved_addr
                };

                let dest_offset = (got_sec.sh_offset + got_offset) as usize;
                unsafe {
                    let ptr = mmap.as_mut_ptr().add(dest_offset) as *mut u64;
                    std::ptr::write_unaligned(ptr, val);
                }
            }
        }

        // 6. Применение релокаций
        for (obj_idx, input) in self.inputs.iter().enumerate() {
            let bytes = input.source.as_slice();
            for sec in &input.sections {
                if sec.sh_type == SHT_RELA {
                    let count = (sec.sh_size / sec.sh_entsize) as usize;
                    let relas =
                        unsafe { read_slice::<Elf64_Rela>(bytes, sec.sh_offset as usize, count) }
                            .ok_or("Failed to read relocations")?;

                    let target_sec_idx = sec.sh_info as usize;

                    if let Some(target_vaddr_start) =
                        self.get_input_section_vaddr(obj_idx, target_sec_idx, 0)
                    {
                        let out_sec = self
                            .find_output_section_for_input(obj_idx, target_sec_idx)
                            .ok_or("Failed to find output section for contribution")?;
                        let contrib_offset = self
                            .find_contribution_offset(obj_idx, target_sec_idx)
                            .ok_or("Failed to find contribution offset")?;

                        for rela in relas {
                            let rel_type = (rela.r_info & 0xFFFFFFFF) as u32;
                            let sym_idx = (rela.r_info >> 32) as usize;

                            let p = target_vaddr_start + rela.r_offset;
                            let dest_file_offset =
                                (out_sec.sh_offset + contrib_offset + rela.r_offset) as usize;

                            unsafe {
                                let ptr = mmap.as_mut_ptr().add(dest_file_offset);
                                match rel_type {
                                    R_X86_64_NONE => {}
                                    R_X86_64_64 => {
                                        if let Some(s) = self.resolve_symbol_address(
                                            obj_idx,
                                            sym_idx,
                                            rela.r_addend,
                                        ) {
                                            std::ptr::write_unaligned(ptr as *mut u64, s);
                                        }
                                    }
                                    R_X86_64_PC32 | R_X86_64_PLT32 => {
                                        if let Some(s) = self.resolve_symbol_address(
                                            obj_idx,
                                            sym_idx,
                                            rela.r_addend,
                                        ) {
                                            let val = (s as i64 - p as i64) as i32;
                                            std::ptr::write_unaligned(ptr as *mut i32, val);
                                        }
                                    }
                                    R_X86_64_32 => {
                                        if let Some(s) = self.resolve_symbol_address(
                                            obj_idx,
                                            sym_idx,
                                            rela.r_addend,
                                        ) {
                                            std::ptr::write_unaligned(ptr as *mut u32, s as u32);
                                        }
                                    }
                                    R_X86_64_32S => {
                                        if let Some(s) = self.resolve_symbol_address(
                                            obj_idx,
                                            sym_idx,
                                            rela.r_addend,
                                        ) {
                                            std::ptr::write_unaligned(ptr as *mut i32, s as i32);
                                        }
                                    }
                                    R_X86_64_PC64 => {
                                        if let Some(s) = self.resolve_symbol_address(
                                            obj_idx,
                                            sym_idx,
                                            rela.r_addend,
                                        ) {
                                            let val = (s as i64 - p as i64) as u64;
                                            std::ptr::write_unaligned(ptr as *mut u64, val);
                                        }
                                    }
                                    R_X86_64_TPOFF32 => {
                                        if let Some(s) = self.resolve_symbol_address(
                                            obj_idx,
                                            sym_idx,
                                            rela.r_addend,
                                        ) {
                                            if let Some(tls_seg) = self
                                                .segments
                                                .iter()
                                                .find(|seg| seg.p_type == PT_TLS)
                                            {
                                                let tls_end = tls_seg.p_vaddr + tls_seg.p_memsz;
                                                let val = (s as i64 - tls_end as i64) as i32;
                                                std::ptr::write_unaligned(ptr as *mut i32, val);
                                            }
                                        }
                                    }
                                    R_X86_64_GOTPCREL
                                    | R_X86_64_GOTPCRELX
                                    | R_X86_64_REX_GOTPCRELX
                                    | R_X86_64_GOTTPOFF => {
                                        if let Some(sym_name) =
                                            self.get_symbol_name(obj_idx, sym_idx)
                                        {
                                            if let Some(&got_offset) =
                                                self.got_offset_map.get(&sym_name)
                                            {
                                                if let Some(got_sec) = self
                                                    .output_sections
                                                    .iter()
                                                    .find(|s| s.name == ".got")
                                                {
                                                    let g = got_sec.sh_addr + got_offset;
                                                    let val = (g as i64 + rela.r_addend - p as i64)
                                                        as i32;
                                                    std::ptr::write_unaligned(ptr as *mut i32, val);
                                                }
                                            }
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
            }
        }

        mmap.flush()
            .map_err(|e| format!("Failed to flush mmap: {e}"))?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&output_path)
                .map_err(|e| format!("Failed to read metadata: {e}"))?
                .permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&output_path, perms)
                .map_err(|e| format!("Failed to set permissions: {e}"))?;
        }

        println!("Successfully wrote fully functional, optimized minimal binary!");
        Ok(())
    }

    fn get_global_symbol_vaddr(&self, global_sym: &GlobalSymbol) -> Option<u64> {
        if global_sym.is_synthetic {
            Some(global_sym.value)
        } else {
            self.get_input_section_vaddr(
                global_sym.input_obj_idx,
                global_sym.shndx as usize,
                global_sym.value,
            )
        }
    }

    fn get_input_section_vaddr(&self, obj_idx: usize, sec_idx: usize, offset: u64) -> Option<u64> {
        for out_sec in &self.output_sections {
            if out_sec.is_merge {
                if let Some(ranges) = out_sec.string_merge_ranges.get(&(obj_idx, sec_idx)) {
                    for range in ranges {
                        if offset >= range.input_start && offset < range.input_end {
                            let delta = offset - range.input_start;
                            return Some(out_sec.sh_addr + range.output_start + delta);
                        }
                    }
                }
            } else {
                for contrib in &out_sec.contributions {
                    if contrib.input_obj_idx == obj_idx && contrib.input_sec_idx == sec_idx {
                        return Some(out_sec.sh_addr + contrib.offset_in_output + offset);
                    }
                }
            }
        }
        None
    }

    fn resolve_symbol_address(&self, obj_idx: usize, sym_idx: usize, addend: i64) -> Option<u64> {
        let input = &self.inputs[obj_idx];
        let bytes = input.source.as_slice();
        let symtab_sec = input.sections.iter().find(|s| s.sh_type == SHT_SYMTAB)?;
        let syms = unsafe {
            read_slice::<Elf64_Sym>(
                bytes,
                symtab_sec.sh_offset as usize,
                (symtab_sec.sh_size / symtab_sec.sh_entsize) as usize,
            )
        }?;
        let sym = syms.get(sym_idx)?;

        let bind = sym.st_info >> 4;
        if bind == STB_GLOBAL || bind == STB_WEAK {
            let strtab_sec = &input.sections[symtab_sec.sh_link as usize];
            let name = input.get_string_from_sec(strtab_sec, sym.st_name)?;
            if let Some(global_sym) = self.global_symbols.get(name) {
                if global_sym.is_synthetic {
                    return Some(global_sym.value);
                } else {
                    let final_offset = (global_sym.value as i64 + addend) as u64;
                    return self.get_input_section_vaddr(
                        global_sym.input_obj_idx,
                        global_sym.shndx as usize,
                        final_offset,
                    );
                }
            }

            if bind == STB_WEAK {
                return Some(0);
            }
        }

        if sym.st_shndx == SHN_UNDEF {
            return None;
        }

        let final_offset = (sym.st_value as i64 + addend) as u64;
        self.get_input_section_vaddr(obj_idx, sym.st_shndx as usize, final_offset)
    }

    fn find_output_section_for_input(
        &self,
        obj_idx: usize,
        sec_idx: usize,
    ) -> Option<&OutputSection> {
        for out_sec in &self.output_sections {
            for contrib in &out_sec.contributions {
                if contrib.input_obj_idx == obj_idx && contrib.input_sec_idx == sec_idx {
                    return Some(out_sec);
                }
            }
        }
        None
    }

    fn find_contribution_offset(&self, obj_idx: usize, sec_idx: usize) -> Option<u64> {
        for out_sec in &self.output_sections {
            for contrib in &out_sec.contributions {
                if contrib.input_obj_idx == obj_idx && contrib.input_sec_idx == sec_idx {
                    return Some(contrib.offset_in_output);
                }
            }
        }
        None
    }
}

#[inline(always)]
fn ranges_push(
    map: &mut HashMap<(usize, usize), Vec<MergeRange>>,
    obj: usize,
    sec: usize,
    range: MergeRange,
) {
    map.entry((obj, sec)).or_insert_with(Vec::new).push(range);
}

// =============================================================================
// Entry Point
// =============================================================================

fn main() {
    let mut linker = Linker::new();

    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        println!("Usage: {} <object_or_archive_files...>", args[0]);
        return;
    }

    for arg in &args[1..] {
        if arg.ends_with(".o") || arg.ends_with(".a") {
            if let Err(e) = linker.add_file(arg) {
                eprintln!("Error loading {}: {}", arg, e);
                std::process::exit(1);
            }
        }
    }

    if let Err(e) = linker.resolve_symbols_and_link() {
        eprintln!("Link error: {}", e);
        std::process::exit(1);
    }
}

impl Linker {
    pub fn resolve_symbols_and_link(&mut self) -> Result<(), String> {
        self.resolve_symbols()?;
        self.compute_layout()?;
        self.write_output("a.out")?;
        println!("Linking completed successfully!");
        Ok(())
    }
}
