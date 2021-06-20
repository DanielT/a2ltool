use std::{collections::HashMap, fs::File};
use object::Object;
use object::read::ObjectSection;
use gimli::{Abbreviations, UnitHeader, DebuggingInformationEntry};
use gimli::{RunTimeEndian, EndianSlice};
use std::ops::Index;


type SliceType<'a> = EndianSlice<'a, RunTimeEndian>;

mod attributes;
use attributes::*;
mod typereader;
use typereader::load_types;


#[derive(Debug)]
pub(crate) struct VarInfo {
    pub(crate) address: u64,
    pub(crate) typeref: usize
}


#[derive(Debug)]
pub(crate) enum TypeInfo {
    Uint8,
    Uint16,
    Uint32,
    Uint64,
    Sint8,
    Sint16,
    Sint32,
    Sint64,
    Float,
    Double,
    Bitfield {
        basetype: Box<TypeInfo>,
        bit_offset: u16,
        bit_size: u16
    },
    Pointer(u64),
    Other(u64),
    Struct {
        typename: String,
        size: u64,
        members: HashMap<String, (TypeInfo, u64)>
    },
    Class {
        typename: String,
        size: u64,
        members: HashMap<String, (TypeInfo, u64)>
    },
    Union {
        size: u64,
        members: HashMap<String, (TypeInfo, u64)>
    },
    Enum {
        typename: String,
        size: u64,
        enumerators: Vec<(String, i64)>
    },
    Array {
        size: u64,
        dim: Vec<u64>,
        stride: u64,
        arraytype: Box<TypeInfo>
    }
}

pub(crate) struct UnitList<'a> {
    list: Vec<(UnitHeader<SliceType<'a>>, gimli::Abbreviations)>
}

#[derive(Debug)]
pub(crate) struct DebugData {
    pub(crate) variables: HashMap<String, VarInfo>,
    pub(crate) types: HashMap<usize, TypeInfo>
}


// load the debug info from an elf file
pub(crate) fn load_debuginfo(filename: &str) -> Result<DebugData, String> {
    let filedata = load_filedata(filename)?;
    let elffile = load_elf_file(filename, &*filedata)?;
    let dwarf = load_dwarf(&elffile)?;
    
    Ok(read_debug_info_entries(&dwarf))
}


// open a file and mmap its content
fn load_filedata(filename: &str) -> Result<memmap::Mmap, String> {
    let file = match File::open(filename) {
        Ok(file) => file,
        Err(error) => return Err(format!("Error: could not open file {}: {}", filename, error))
    };

    match unsafe { memmap::Mmap::map(&file) } {
        Ok(mmap) => Ok(mmap),
        Err(err) => {
            return Err(format!("Error: Failed to map file '{}': {}", filename, err));
        }
    }
}


// read the headers and sections of an elf/object file
fn load_elf_file<'data>(filename: &str, filedata: &'data [u8]) -> Result<object::read::File<'data>, String> {
    match object::File::parse(&*filedata) {
        Ok(file) => Ok(file),
        Err(err) => {
            Err(format!("Error: Failed to parse file '{}': {}", filename, err))
        }
    }
}


// load the SWARF debug info from the .debug_<xyz> sections
fn load_dwarf<'data>(elffile: &object::read::File<'data>) -> Result<gimli::Dwarf<SliceType<'data>>, String> {
    // Dwarf::load takes two closures / functions and uses them to load all the required debug sections
    let loader = |section: gimli::SectionId| { get_file_section_reader(elffile, section.name()) };
    let sup_loader = |section: gimli::SectionId| { get_sup_file_section_reader(elffile, section.name()) };
    gimli::Dwarf::load(loader, sup_loader)
}


// get a section from the elf file.
// returns a slice referencing the section data if it exists, or an empty slice otherwise
fn get_file_section_reader<'data>(elffile: &object::read::File<'data>, section_name: &str) -> Result<SliceType<'data>, String> {
    if let Some(dbginfo) = elffile.section_by_name(section_name) {
        match dbginfo.data() {
            Ok(val) => Ok(EndianSlice::new(val, get_endian(elffile))),
            Err(e) => Err(e.to_string())
        }
    } else {
        Ok(EndianSlice::new(&[], get_endian(elffile)))
    }
}


// required by Dwarf::load: get a section from a supplementary file.
// Supplementary files are not supported, so the function always returns an empty slice
fn get_sup_file_section_reader<'data>(elffile: &object::read::File<'data>, _section_name: &str) -> Result<SliceType<'data>, String> {
    Ok(EndianSlice::new(&[], get_endian(elffile)))
}


// get the endianity of the elf file
fn get_endian(elffile: &object::read::File) -> RunTimeEndian {
    if elffile.is_little_endian() {
        RunTimeEndian::Little
    } else {
        RunTimeEndian::Big
    }
}


// read the debug information entries in the DWAF data to get all the global variables and their types
fn read_debug_info_entries(dwarf: &gimli::Dwarf<SliceType>) -> DebugData {
    let (variables, units) = load_variables(dwarf);
    let types = load_types(&variables, units, dwarf);

    DebugData {
        variables,
        types
    }
}


// load all global variables from the dwarf data
fn load_variables<'a>(dwarf: &gimli::Dwarf<EndianSlice<'a, RunTimeEndian>>) -> (HashMap<String, VarInfo>, UnitList<'a>) {
    let mut variables = HashMap::<String, VarInfo>::new();
    let mut unit_list = UnitList::new();

    let mut iter = dwarf.debug_info.units();
    while let Ok(Some(unit)) = iter.next() {
        let abbreviations = unit.abbreviations(&dwarf.debug_abbrev).unwrap();

        // the root of the tree inside of a unit is always a DW_TAG_compile_unit
        // the global variables are among the immediate children of the DW_TAG_compile_unit
        // static variable in functions are hidden further down inside of DW_TAG_subprogram[/DW_TAG_lexical_block]*
        // we can easily find all of them by using depth-first traversal of the tree
        let mut entries_cursor = unit.entries(&abbreviations);
        while let Ok(Some((_depth_delta, entry))) = entries_cursor.next_dfs() {
            if entry.tag() == gimli::constants::DW_TAG_variable {
                if let Some((name, typeref, address)) = get_global_variable(entry, &unit, &&abbreviations, dwarf) {
                    variables.insert(name, VarInfo{address, typeref});
                }
            }
        }

        unit_list.add(unit, abbreviations);
    }

    (variables, unit_list)
}


// an entry of the type DW_TAG_variable only describes a global variable if there is a name, a type and an address
// this function tries to get all three and returns them
fn get_global_variable(
    entry: &DebuggingInformationEntry<SliceType, usize>,
    unit: &UnitHeader<SliceType>,
    abbrev: &gimli::Abbreviations,
    dwarf: &gimli::Dwarf<EndianSlice<RunTimeEndian>>
) -> Option<(String, usize, u64)> {
    let address = get_location_attribute(entry, unit.encoding())?;
    if let Some(specification_entry) = get_specification_attribute(entry, unit, abbrev) {
        // the entry refers to a specification, which contains the name and type reference
        let name = get_name_attribute(&specification_entry, dwarf)?;
        let typeref = get_typeref_attribute(&specification_entry, &unit)?;

        Some((name, typeref, address))
    } else {
        // there is no specification and all info is part of this entry
        let name = get_name_attribute(entry, dwarf)?;
        let typeref = get_typeref_attribute(entry, &unit)?;

        Some((name, typeref, address))
    }
}


// UnitList holds a list of all UnitHeaders in the Dwarf data for convenient access
impl<'a> UnitList<'a> {
    fn new() -> Self {
        Self {
            list: Vec::new()
        }
    }

    fn add(&mut self, unit: UnitHeader<SliceType<'a>>, abbrev: Abbreviations) {
        self.list.push((unit, abbrev));
    }

    fn get_unit<'b>(&'b self, itemoffset: usize) -> Option<usize> {
        for (idx, (unit, _)) in self.list.iter().enumerate() {
            let unitoffset = unit.offset().as_debug_info_offset().unwrap().0;
            if unitoffset < itemoffset && unitoffset + unit.length_including_self() > itemoffset {
                return Some(idx);
            }
        }

        None
    }
}

impl<'a> Index<usize> for UnitList<'a> {
    type Output = (UnitHeader<SliceType<'a>>, gimli::Abbreviations);

    fn index(&self, idx: usize) -> &Self::Output {
        &self.list[idx]
    }
}


impl TypeInfo {
    pub(crate) fn get_size(&self) -> u64 {
        match self {
            TypeInfo::Uint8 => 1,
            TypeInfo::Uint16 => 2,
            TypeInfo::Uint32 => 4,
            TypeInfo::Uint64 => 8,
            TypeInfo::Sint8 => 1,
            TypeInfo::Sint16 => 2,
            TypeInfo::Sint32 => 4,
            TypeInfo::Sint64 => 8,
            TypeInfo::Float => 4,
            TypeInfo::Double => 8,
            TypeInfo::Bitfield {basetype, ..} => basetype.get_size(),
            TypeInfo::Pointer(size) |
            TypeInfo::Other(size) |
            TypeInfo::Struct { size, .. } |
            TypeInfo::Class { size, .. } |
            TypeInfo::Union { size, .. } |
            TypeInfo::Enum { size, .. } |
            TypeInfo::Array { size, .. } => *size
        }
    }
}