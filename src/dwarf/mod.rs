use gimli::{Abbreviations, DebuggingInformationEntry, Dwarf, UnitHeader};
use gimli::{EndianSlice, RunTimeEndian};
use indexmap::IndexMap;
use object::read::ObjectSection;
use object::{Object, Endianness};
use std::ffi::OsStr;
use std::ops::Index;
use std::{collections::HashMap, fs::File};

type SliceType<'a> = EndianSlice<'a, RunTimeEndian>;

mod attributes;
use attributes::{
    get_abstract_origin_attribute, get_location_attribute, get_name_attribute,
    get_specification_attribute, get_typeref_attribute,
};
mod iter;
mod typereader;

#[derive(Debug)]
pub(crate) struct VarInfo {
    pub(crate) address: u64,
    pub(crate) typeref: usize,
}

#[derive(Debug, Clone)]
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
        bit_size: u16,
    },
    Pointer(u64),
    Other(u64),
    Struct {
        // typename: String,
        size: u64,
        members: IndexMap<String, (TypeInfo, u64)>,
    },
    Class {
        // typename: String,
        size: u64,
        inheritance: IndexMap<String, (TypeInfo, u64)>,
        members: IndexMap<String, (TypeInfo, u64)>,
    },
    Union {
        size: u64,
        members: IndexMap<String, (TypeInfo, u64)>,
    },
    Enum {
        typename: String,
        size: u64,
        enumerators: Vec<(String, i64)>,
    },
    Array {
        size: u64,
        dim: Vec<u64>,
        stride: u64,
        arraytype: Box<TypeInfo>,
    },
}

pub(crate) struct UnitList<'a> {
    list: Vec<(UnitHeader<SliceType<'a>>, gimli::Abbreviations)>,
}

#[derive(Debug)]
pub(crate) struct DebugData {
    pub(crate) variables: IndexMap<String, VarInfo>,
    pub(crate) types: HashMap<usize, TypeInfo>,
    pub(crate) demangled_names: HashMap<String, String>,
}

struct DebugDataReader<'elffile> {
    dwarf: Dwarf<EndianSlice<'elffile, RunTimeEndian>>,
    verbose: bool,
    typedefs: HashMap<usize, String>,
    units: UnitList<'elffile>,
    endian: Endianness,
}

impl DebugData {
    // load the debug info from an elf file
    pub(crate) fn load(filename: &OsStr, verbose: bool) -> Result<Self, String> {
        let filedata = load_filedata(filename)?;
        let elffile = load_elf_file(&filename.to_string_lossy(), &filedata)?;
        let dwarf = load_dwarf(&elffile)?;

        let mut dbg_reader = DebugDataReader {
            dwarf,
            verbose,
            typedefs: HashMap::new(),
            units: UnitList::new(),
            endian: elffile.endianness()
        };

        Ok(dbg_reader.read_debug_info_entries())
    }

    pub(crate) fn iter(&self, use_new_arrays: bool) -> iter::VariablesIterator {
        iter::VariablesIterator::new(self, use_new_arrays)
    }
}

// open a file and mmap its content
fn load_filedata(filename: &OsStr) -> Result<memmap::Mmap, String> {
    let file = match File::open(filename) {
        Ok(file) => file,
        Err(error) => {
            return Err(format!(
                "Error: could not open file {}: {}",
                filename.to_string_lossy(),
                error
            ))
        }
    };

    match unsafe { memmap::Mmap::map(&file) } {
        Ok(mmap) => Ok(mmap),
        Err(err) => Err(format!(
            "Error: Failed to map file '{}': {}",
            filename.to_string_lossy(),
            err
        )),
    }
}

// read the headers and sections of an elf/object file
fn load_elf_file<'data>(
    filename: &str,
    filedata: &'data [u8],
) -> Result<object::read::File<'data>, String> {
    match object::File::parse(filedata) {
        Ok(file) => Ok(file),
        Err(err) => Err(format!("Error: Failed to parse file '{filename}': {err}")),
    }
}

// load the SWARF debug info from the .debug_<xyz> sections
fn load_dwarf<'data>(
    elffile: &object::read::File<'data>,
) -> Result<gimli::Dwarf<SliceType<'data>>, String> {
    // Dwarf::load takes two closures / functions and uses them to load all the required debug sections
    let loader = |section: gimli::SectionId| get_file_section_reader(elffile, section.name());
    gimli::Dwarf::load(loader)
}

// get a section from the elf file.
// returns a slice referencing the section data if it exists, or an empty slice otherwise
fn get_file_section_reader<'data>(
    elffile: &object::read::File<'data>,
    section_name: &str,
) -> Result<SliceType<'data>, String> {
    if let Some(dbginfo) = elffile.section_by_name(section_name) {
        match dbginfo.data() {
            Ok(val) => Ok(EndianSlice::new(val, get_endian(elffile))),
            Err(e) => Err(e.to_string()),
        }
    } else {
        Ok(EndianSlice::new(&[], get_endian(elffile)))
    }
}

// get the endianity of the elf file
fn get_endian(elffile: &object::read::File) -> RunTimeEndian {
    if elffile.is_little_endian() {
        RunTimeEndian::Little
    } else {
        RunTimeEndian::Big
    }
}

impl<'elffile> DebugDataReader<'elffile> {
    // read the debug information entries in the DWAF data to get all the global variables and their types
    fn read_debug_info_entries(&mut self) -> DebugData {
        let variables = self.load_variables();
        let types = self.load_types(&variables);
        let varname_list: Vec<&String> = variables.keys().collect();
        let demangled_names = demangle_cpp_varnames(&varname_list);

        DebugData {
            variables,
            types,
            demangled_names,
        }
    }

    // load all global variables from the dwarf data
    fn load_variables(&mut self) -> IndexMap<String, VarInfo> {
        let mut variables = IndexMap::<String, VarInfo>::new();

        let mut iter = self.dwarf.debug_info.units();
        while let Ok(Some(unit)) = iter.next() {
            let abbreviations = unit.abbreviations(&self.dwarf.debug_abbrev).unwrap();
            self.units.add(unit, abbreviations);
            let (unit, abbreviations) = &self.units[self.units.list.len() - 1];

            // the root of the tree inside of a unit is always a DW_TAG_compile_unit
            // the global variables are among the immediate children of the DW_TAG_compile_unit
            // static variable in functions are hidden further down inside of DW_TAG_subprogram[/DW_TAG_lexical_block]*
            // we can easily find all of them by using depth-first traversal of the tree
            let mut entries_cursor = unit.entries(abbreviations);
            while let Ok(Some((_depth_delta, entry))) = entries_cursor.next_dfs() {
                if entry.tag() == gimli::constants::DW_TAG_variable {
                    match self.get_global_variable(entry, unit, abbreviations) {
                        Ok(Some((name, typeref, address))) => {
                            variables.insert(name, VarInfo { address, typeref });
                        }
                        Ok(None) => {
                            // unremarkable, the variable is not a global variable
                        }
                        Err(errmsg) => {
                            if self.verbose {
                                let offset = entry
                                    .offset()
                                    .to_debug_info_offset(unit)
                                    .unwrap_or(gimli::DebugInfoOffset(0))
                                    .0;
                                println!("Error loading variable @{offset:x}: {errmsg}");
                            }
                        }
                    }
                } else if entry.tag() == gimli::constants::DW_TAG_typedef {
                    // collect information about all typedefs
                    if let Ok(name) = get_name_attribute(entry, &self.dwarf, unit) {
                        if let Ok(typeref) = get_typeref_attribute(entry, unit) {
                            // build a reverse map from the referenced type to the typedef name
                            // it's possible that a type has multiple typedefs, in which case we only keep the last one
                            self.typedefs.insert(typeref, name);
                        }
                    }
                }
            }
        }

        variables
    }

    // an entry of the type DW_TAG_variable only describes a global variable if there is a name, a type and an address
    // this function tries to get all three and returns them
    fn get_global_variable(
        &self,
        entry: &DebuggingInformationEntry<SliceType, usize>,
        unit: &UnitHeader<SliceType>,
        abbrev: &gimli::Abbreviations,
    ) -> Result<Option<(String, usize, u64)>, String> {
        match get_location_attribute(self, entry, unit.encoding(), &self.units.list.len() - 1) {
            Some(address) => {
                // if debugging information entry A has a DW_AT_specification or DW_AT_abstract_origin attribute
                // pointing to another debugging information entry B, any attributes of B are considered to be part of A.
                if let Some(specification_entry) = get_specification_attribute(entry, unit, abbrev)
                {
                    // the entry refers to a specification, which contains the name and type reference
                    let name = get_name_attribute(&specification_entry, &self.dwarf, unit)?;
                    let typeref = get_typeref_attribute(&specification_entry, unit)?;

                    Ok(Some((name, typeref, address)))
                } else if let Some(abstract_origin_entry) =
                    get_abstract_origin_attribute(entry, unit, abbrev)
                {
                    // the entry refers to an abstract origin, which should also be considered when getting the name and type ref
                    let name = get_name_attribute(entry, &self.dwarf, unit).or_else(|_| {
                        get_name_attribute(&abstract_origin_entry, &self.dwarf, unit)
                    })?;
                    let typeref = get_typeref_attribute(entry, unit)
                        .or_else(|_| get_typeref_attribute(&abstract_origin_entry, unit))?;

                    Ok(Some((name, typeref, address)))
                } else {
                    // usual case: there is no specification or abstract origin and all info is part of this entry
                    let name = get_name_attribute(entry, &self.dwarf, unit)?;
                    let typeref = get_typeref_attribute(entry, unit)?;

                    Ok(Some((name, typeref, address)))
                }
            }
            None => {
                // it's a local variable, no error
                Ok(None)
            }
        }
    }
}

fn demangle_cpp_varnames(input: &[&String]) -> HashMap<String, String> {
    let mut demangled_symbols = HashMap::<String, String>::new();
    let demangle_opts = cpp_demangle::DemangleOptions::new()
        .no_params()
        .no_return_type();
    for varname in input {
        // some really simple strings can be processed by the demangler, e.g "c" -> "const", which is wrong here.
        // by only processing symbols that start with _Z (variables in classes/namespaces) this problem is avoided
        if varname.starts_with("_Z") {
            if let Ok(sym) = cpp_demangle::Symbol::new(*varname) {
                // exclude useless demangled names like "typeinfo for std::type_info" or "{vtable(std::type_info)}"
                if let Ok(demangled) = sym.demangle(&demangle_opts) {
                    if !demangled.contains(' ') && !demangled.starts_with("{vtable") {
                        demangled_symbols.insert(demangled, (*varname).clone());
                    }
                }
            }
        }
    }

    demangled_symbols
}

// UnitList holds a list of all UnitHeaders in the Dwarf data for convenient access
impl<'a> UnitList<'a> {
    fn new() -> Self {
        Self { list: Vec::new() }
    }

    fn add(&mut self, unit: UnitHeader<SliceType<'a>>, abbrev: Abbreviations) {
        self.list.push((unit, abbrev));
    }

    fn get_unit(&self, itemoffset: usize) -> Option<usize> {
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
            TypeInfo::Bitfield { basetype, .. } => basetype.get_size(),
            TypeInfo::Pointer(size)
            | TypeInfo::Other(size)
            | TypeInfo::Struct { size, .. }
            | TypeInfo::Class { size, .. }
            | TypeInfo::Union { size, .. }
            | TypeInfo::Enum { size, .. }
            | TypeInfo::Array { size, .. } => *size,
        }
    }
}
