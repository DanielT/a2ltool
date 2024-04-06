use gimli::{Abbreviations, DebugInfoOffset, DebuggingInformationEntry, Dwarf, UnitHeader};
use gimli::{EndianSlice, RunTimeEndian};
use indexmap::IndexMap;
use object::read::ObjectSection;
use object::{Endianness, Object};
use std::ffi::OsStr;
use std::fmt::Display;
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
pub(crate) struct TypeInfo {
    pub(crate) name: Option<String>, // not all types have a name
    pub(crate) unit_idx: usize,
    pub(crate) datatype: DwarfDataType,
    pub(crate) dbginfo_offset: usize,
}

#[derive(Debug, Clone)]
pub(crate) enum DwarfDataType {
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
    Pointer(u64, DebugInfoOffset),
    Struct {
        size: u64,
        members: IndexMap<String, (TypeInfo, u64)>,
    },
    Class {
        size: u64,
        inheritance: IndexMap<String, (TypeInfo, u64)>,
        members: IndexMap<String, (TypeInfo, u64)>,
    },
    Union {
        size: u64,
        members: IndexMap<String, (TypeInfo, u64)>,
    },
    Enum {
        size: u64,
        enumerators: Vec<(String, i64)>,
    },
    Array {
        size: u64,
        dim: Vec<u64>,
        stride: u64,
        arraytype: Box<TypeInfo>,
    },
    TypeRef(usize, u64),
    FuncPtr(u64),
    Other(u64),
}

pub(crate) struct UnitList<'a> {
    list: Vec<(UnitHeader<SliceType<'a>>, gimli::Abbreviations)>,
}

#[derive(Debug)]
pub(crate) struct DebugData {
    pub(crate) variables: IndexMap<String, VarInfo>,
    pub(crate) types: HashMap<usize, TypeInfo>,
    pub(crate) typenames: HashMap<String, Vec<usize>>,
    pub(crate) demangled_names: HashMap<String, String>,
    pub(crate) unit_names: Vec<Option<String>>,
}

struct DebugDataReader<'elffile> {
    dwarf: Dwarf<EndianSlice<'elffile, RunTimeEndian>>,
    verbose: bool,
    units: UnitList<'elffile>,
    unit_names: Vec<Option<String>>,
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
            units: UnitList::new(),
            unit_names: Vec::new(),
            endian: elffile.endianness(),
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
        let (types, typenames) = self.load_types(&variables);
        let varname_list: Vec<&String> = variables.keys().collect();
        let demangled_names = demangle_cpp_varnames(&varname_list);

        let mut unit_names = Vec::new();
        std::mem::swap(&mut unit_names, &mut self.unit_names);

        DebugData {
            variables,
            types,
            typenames,
            demangled_names,
            unit_names,
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
            if let Ok(Some((_, entry))) = entries_cursor.next_dfs() {
                if entry.tag() == gimli::constants::DW_TAG_compile_unit {
                    self.unit_names
                        .push(get_name_attribute(entry, &self.dwarf, unit).ok());
                }
            }

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
    const MAX_RECURSION_DEPTH: usize = 5;

    pub(crate) fn get_size(&self) -> u64 {
        match &self.datatype {
            DwarfDataType::Uint8 => 1,
            DwarfDataType::Uint16 => 2,
            DwarfDataType::Uint32 => 4,
            DwarfDataType::Uint64 => 8,
            DwarfDataType::Sint8 => 1,
            DwarfDataType::Sint16 => 2,
            DwarfDataType::Sint32 => 4,
            DwarfDataType::Sint64 => 8,
            DwarfDataType::Float => 4,
            DwarfDataType::Double => 8,
            DwarfDataType::Bitfield { basetype, .. } => basetype.get_size(),
            DwarfDataType::Pointer(size, _)
            | DwarfDataType::Other(size)
            | DwarfDataType::Struct { size, .. }
            | DwarfDataType::Class { size, .. }
            | DwarfDataType::Union { size, .. }
            | DwarfDataType::Enum { size, .. }
            | DwarfDataType::Array { size, .. }
            | DwarfDataType::FuncPtr(size)
            | DwarfDataType::TypeRef(_, size) => *size,
        }
    }

    pub(crate) fn get_members(&self) -> Option<&IndexMap<String, (TypeInfo, u64)>> {
        match &self.datatype {
            DwarfDataType::Struct { members, .. }
            | DwarfDataType::Class { members, .. }
            | DwarfDataType::Union { members, .. } => Some(members),

            _ => None,
        }
    }

    pub(crate) fn get_pointer<'a>(
        &self,
        types: &'a HashMap<usize, TypeInfo>,
    ) -> Option<(u64, &'a TypeInfo)> {
        if let DwarfDataType::Pointer(pt_size, pt_ref) = &self.datatype {
            let typeinfo = types.get(&pt_ref.0)?;
            Some((*pt_size, typeinfo))
        } else {
            None
        }
    }

    pub(crate) fn get_arraytype(&self) -> Option<&TypeInfo> {
        if let DwarfDataType::Array { arraytype, .. } = &self.datatype {
            Some(arraytype)
        } else {
            None
        }
    }

    pub(crate) fn get_reference<'a>(&'a self, types: &'a HashMap<usize, TypeInfo>) -> &'a Self {
        if let DwarfDataType::TypeRef(dbginfo_offset, _) = &self.datatype {
            types.get(dbginfo_offset).unwrap_or(self)
        } else {
            self
        }
    }

    // not using PartialEq, because not all fields are considered for this comparison
    pub(crate) fn compare(&self, other: &TypeInfo, types: &HashMap<usize, TypeInfo>) -> bool {
        self.compare_internal(other, types, 0)
    }

    fn compare_internal(
        &self,
        other: &TypeInfo,
        types: &HashMap<usize, TypeInfo>,
        depth: usize,
    ) -> bool {
        let type_1 = self.get_reference(types);
        let type_2 = other.get_reference(types);

        type_1.dbginfo_offset == type_2.dbginfo_offset
            || (type_1.name == type_2.name
                && match (&type_1.datatype, &type_2.datatype) {
                    (DwarfDataType::Uint8, DwarfDataType::Uint8)
                    | (DwarfDataType::Uint16, DwarfDataType::Uint16)
                    | (DwarfDataType::Uint32, DwarfDataType::Uint32)
                    | (DwarfDataType::Uint64, DwarfDataType::Uint64)
                    | (DwarfDataType::Sint8, DwarfDataType::Sint8)
                    | (DwarfDataType::Sint16, DwarfDataType::Sint16)
                    | (DwarfDataType::Sint32, DwarfDataType::Sint32)
                    | (DwarfDataType::Sint64, DwarfDataType::Sint64)
                    | (DwarfDataType::Float, DwarfDataType::Float)
                    | (DwarfDataType::Double, DwarfDataType::Double) => true,
                    (
                        DwarfDataType::Enum { size, enumerators },
                        DwarfDataType::Enum {
                            size: size2,
                            enumerators: enumerators2,
                        },
                    ) => size == size2 && enumerators == enumerators2,
                    (
                        DwarfDataType::Array {
                            size,
                            dim,
                            stride,
                            arraytype,
                        },
                        DwarfDataType::Array {
                            size: size2,
                            dim: dim2,
                            stride: stride2,
                            arraytype: arraytype2,
                        },
                    ) => {
                        size == size2
                            && dim == dim2
                            && stride == stride2
                            && arraytype.compare_internal(arraytype2, types, depth + 1)
                    }
                    (
                        DwarfDataType::Pointer(size1, dest_offset1),
                        DwarfDataType::Pointer(size2, dest_offset2),
                    ) => {
                        size1 == size2
                            && if dest_offset1.0 == dest_offset2.0 {
                                true
                            } else if let (Some(dest_type1), Some(dest_type2)) =
                                (types.get(&dest_offset1.0), types.get(&dest_offset2.0))
                            {
                                // can't always call ref1.compare(&ref2) here, because this could result in infinite recursion
                                if depth < Self::MAX_RECURSION_DEPTH {
                                    dest_type1.compare_internal(dest_type2, types, depth + 1)
                                } else {
                                    // when we're not using compare(), we need to follow TypeRef (if any) to the referenced type
                                    let dest1_deref = dest_type1.get_reference(types);
                                    let dest2_deref = dest_type2.get_reference(types);
                                    dest1_deref.name == dest2_deref.name
                                        && std::mem::discriminant(&dest1_deref.datatype)
                                            == std::mem::discriminant(&dest2_deref.datatype)
                                        && dest1_deref.get_size() == dest2_deref.get_size()
                                }
                            } else {
                                false
                            }
                    }
                    (DwarfDataType::Other(size1), DwarfDataType::Other(size2)) => size1 == size2,
                    (
                        DwarfDataType::Bitfield {
                            basetype,
                            bit_offset,
                            bit_size,
                        },
                        DwarfDataType::Bitfield {
                            basetype: basetype2,
                            bit_offset: bit_offset2,
                            bit_size: bit_size2,
                        },
                    ) => {
                        bit_offset == bit_offset2
                            && bit_size == bit_size2
                            && basetype.compare_internal(basetype2, types, depth + 1)
                    }
                    (
                        DwarfDataType::Struct { size, members },
                        DwarfDataType::Struct {
                            size: size2,
                            members: members2,
                        },
                    ) => size == size2 && Self::compare_members(members, members2, types, depth),
                    (
                        DwarfDataType::Union { size, members },
                        DwarfDataType::Union {
                            size: size2,
                            members: members2,
                        },
                    ) => size == size2 && Self::compare_members(members, members2, types, depth),
                    (
                        DwarfDataType::Class {
                            size,
                            members,
                            inheritance,
                        },
                        DwarfDataType::Class {
                            size: size2,
                            members: members2,
                            inheritance: inheritance2,
                        },
                    ) => {
                        size == size2
                            && Self::compare_members(members, members2, types, depth)
                            && Self::compare_members(inheritance, inheritance2, types, depth)
                    }
                    (DwarfDataType::FuncPtr(size1), DwarfDataType::FuncPtr(size2)) => {
                        size1 == size2
                    }
                    _ => false,
                })
    }

    fn compare_members(
        members1: &IndexMap<String, (TypeInfo, u64)>,
        members2: &IndexMap<String, (TypeInfo, u64)>,
        types: &HashMap<usize, TypeInfo>,
        depth: usize,
    ) -> bool {
        if members1.len() != members2.len() {
            return false;
        }
        for (member1_name, (member1_type, member1_offset)) in members1 {
            let Some((member2_type, member2_offset)) = members2.get(member1_name) else {
                return false;
            };
            if member1_offset != member2_offset {
                return false;
            }
            if depth < Self::MAX_RECURSION_DEPTH {
                if !member1_type.compare_internal(member2_type, types, depth + 1) {
                    return false;
                }
            } else {
                let member1_deref = member1_type.get_reference(types);
                let member2_deref = member2_type.get_reference(types);
                if std::mem::discriminant(&member1_deref.datatype)
                    != std::mem::discriminant(&member2_deref.datatype)
                    || member1_deref.name != member2_deref.name
                {
                    return false;
                }
            }
        }
        true
    }
}

impl Display for TypeInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.datatype {
            DwarfDataType::Uint8 => f.write_str("Uint8"),
            DwarfDataType::Uint16 => f.write_str("Uint16"),
            DwarfDataType::Uint32 => f.write_str("Uint32"),
            DwarfDataType::Uint64 => f.write_str("Uint64"),
            DwarfDataType::Sint8 => f.write_str("Sint8"),
            DwarfDataType::Sint16 => f.write_str("Sint16"),
            DwarfDataType::Sint32 => f.write_str("Sint32"),
            DwarfDataType::Sint64 => f.write_str("Sint64"),
            DwarfDataType::Float => f.write_str("Float"),
            DwarfDataType::Double => f.write_str("Double"),
            DwarfDataType::Bitfield { .. } => f.write_str("Bitfield"),
            DwarfDataType::Pointer(_, _) => write!(f, "Pointer(...)"),
            DwarfDataType::Other(osize) => write!(f, "Other({osize})"),
            DwarfDataType::FuncPtr(osize) => write!(f, "function pointer({osize})"),
            DwarfDataType::Struct { members, .. } => {
                if let Some(name) = &self.name {
                    write!(f, "Struct {name}({} members)", members.len())
                } else {
                    write!(f, "Struct <anonymous>({} members)", members.len())
                }
            }
            DwarfDataType::Class { members, .. } => {
                if let Some(name) = &self.name {
                    write!(f, "Class {name}({} members)", members.len())
                } else {
                    write!(f, "Class <anonymous>({} members)", members.len())
                }
            }
            DwarfDataType::Union { members, .. } => {
                if let Some(name) = &self.name {
                    write!(f, "Union {name}({} members)", members.len())
                } else {
                    write!(f, "Union <anonymous>({} members)", members.len())
                }
            }
            DwarfDataType::Enum { enumerators, .. } => {
                if let Some(name) = &self.name {
                    write!(f, "Enum {name}({} enumerators)", enumerators.len())
                } else {
                    write!(f, "Enum <anonymous>({} enumerators)", enumerators.len())
                }
            }
            DwarfDataType::Array { dim, arraytype, .. } => {
                write!(f, "Array({dim:?} x {arraytype})")
            }
            DwarfDataType::TypeRef(t_ref, _) => write!(f, "TypeRef({t_ref})"),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    static ELF_FILE_NAMES: [&str; 7] = [
        "tests/elffiles/debugdata_clang.elf",
        "tests/elffiles/debugdata_clang_dw4.elf",
        "tests/elffiles/debugdata_clang_dw4_dwz.elf",
        "tests/elffiles/debugdata_gcc.elf",
        "tests/elffiles/debugdata_gcc_dw3.elf",
        "tests/elffiles/debugdata_gcc_dw3_dwz.elf",
        "tests/elffiles/debugdata_gcc_dwz.elf",
    ];

    #[test]
    fn test_load_data() {
        for filename in ELF_FILE_NAMES {
            let debugdata = DebugData::load(OsStr::new(filename), true).unwrap();
            assert_eq!(debugdata.variables.len(), 21);
            assert!(debugdata.variables.get("class1").is_some());
            assert!(debugdata.variables.get("class2").is_some());
            assert!(debugdata.variables.get("class3").is_some());
            assert!(debugdata.variables.get("class4").is_some());
            assert!(debugdata.variables.get("staticvar").is_some());
            assert!(debugdata.variables.get("structvar").is_some());
            assert!(debugdata.variables.get("bitfield").is_some());

            for (_, varinfo) in &debugdata.variables {
                assert!(debugdata.types.contains_key(&varinfo.typeref));
            }

            let varinfo = debugdata.variables.get("class1").unwrap();
            let typeinfo = debugdata.types.get(&varinfo.typeref).unwrap();
            assert!(matches!(
                typeinfo,
                TypeInfo {
                    datatype: DwarfDataType::Class { .. },
                    ..
                }
            ));
            if let TypeInfo {
                datatype:
                    DwarfDataType::Class {
                        inheritance,
                        members,
                        ..
                    },
                ..
            } = typeinfo
            {
                assert!(inheritance.contains_key("base1"));
                assert!(inheritance.contains_key("base2"));
                assert!(matches!(
                    members.get("ss"),
                    Some((
                        TypeInfo {
                            datatype: DwarfDataType::Sint16,
                            ..
                        },
                        _
                    ))
                ));
                assert!(matches!(
                    members.get("base1_var"),
                    Some((
                        TypeInfo {
                            datatype: DwarfDataType::Sint32,
                            ..
                        },
                        _
                    ))
                ));
                assert!(matches!(
                    members.get("base2var"),
                    Some((
                        TypeInfo {
                            datatype: DwarfDataType::Sint32,
                            ..
                        },
                        _
                    ))
                ));
            }

            let varinfo = debugdata.variables.get("class2").unwrap();
            let typeinfo = debugdata.types.get(&varinfo.typeref).unwrap();
            assert!(matches!(
                typeinfo,
                TypeInfo {
                    datatype: DwarfDataType::Class { .. },
                    ..
                }
            ));

            let varinfo = debugdata.variables.get("class3").unwrap();
            let typeinfo = debugdata.types.get(&varinfo.typeref).unwrap();
            assert!(matches!(
                typeinfo,
                TypeInfo {
                    datatype: DwarfDataType::Class { .. },
                    ..
                }
            ));

            let varinfo = debugdata.variables.get("class4").unwrap();
            let typeinfo = debugdata.types.get(&varinfo.typeref).unwrap();
            assert!(matches!(
                typeinfo,
                TypeInfo {
                    datatype: DwarfDataType::Class { .. },
                    ..
                }
            ));

            let varinfo = debugdata.variables.get("staticvar").unwrap();
            let typeinfo = debugdata.types.get(&varinfo.typeref).unwrap();
            assert!(matches!(
                typeinfo,
                TypeInfo {
                    datatype: DwarfDataType::Sint32,
                    ..
                }
            ));

            let varinfo = debugdata.variables.get("structvar").unwrap();
            let typeinfo = debugdata.types.get(&varinfo.typeref).unwrap();
            assert!(matches!(
                typeinfo,
                TypeInfo {
                    datatype: DwarfDataType::Struct { .. },
                    ..
                }
            ));

            let varinfo = debugdata.variables.get("bitfield").unwrap();
            let typeinfo = debugdata.types.get(&varinfo.typeref).unwrap();
            assert!(matches!(
                typeinfo,
                TypeInfo {
                    datatype: DwarfDataType::Struct { .. },
                    ..
                }
            ));
            if let TypeInfo {
                datatype: DwarfDataType::Struct { members, .. },
                ..
            } = typeinfo
            {
                assert!(matches!(
                    members.get("var"),
                    Some((
                        TypeInfo {
                            datatype: DwarfDataType::Bitfield {
                                bit_offset: 0,
                                bit_size: 5,
                                ..
                            },
                            ..
                        },
                        0
                    ))
                ));
                assert!(matches!(
                    members.get("var2"),
                    Some((
                        TypeInfo {
                            datatype: DwarfDataType::Bitfield {
                                bit_offset: 5,
                                bit_size: 5,
                                ..
                            },
                            ..
                        },
                        0
                    ))
                ));
                assert!(matches!(
                    members.get("var3"),
                    Some((
                        TypeInfo {
                            datatype: DwarfDataType::Bitfield {
                                bit_offset: 0,
                                bit_size: 23,
                                ..
                            },
                            ..
                        },
                        4
                    ))
                ));
                assert!(matches!(
                    members.get("var4"),
                    Some((
                        TypeInfo {
                            datatype: DwarfDataType::Bitfield {
                                bit_offset: 23,
                                bit_size: 1,
                                ..
                            },
                            ..
                        },
                        4
                    ))
                ));
            }
            let varinfo = debugdata.variables.get("enum_var1").unwrap();
            let typeinfo = debugdata.types.get(&varinfo.typeref).unwrap();
            assert!(matches!(
                typeinfo,
                TypeInfo {
                    datatype: DwarfDataType::Enum { .. },
                    ..
                }
            ));
            let varinfo = debugdata.variables.get("enum_var2").unwrap();
            let typeinfo = debugdata.types.get(&varinfo.typeref).unwrap();
            assert!(matches!(
                typeinfo,
                TypeInfo {
                    datatype: DwarfDataType::Enum { .. },
                    ..
                }
            ));
            let varinfo = debugdata.variables.get("enum_var3").unwrap();
            let typeinfo = debugdata.types.get(&varinfo.typeref).unwrap();
            assert!(matches!(
                typeinfo,
                TypeInfo {
                    datatype: DwarfDataType::Enum { .. },
                    ..
                }
            ));
        }
    }
}
