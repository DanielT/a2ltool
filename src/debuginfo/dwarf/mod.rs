use crate::debuginfo::{DbgDataType, DebugData, TypeInfo, VarInfo};
use gimli::{Abbreviations, DebuggingInformationEntry, Dwarf, UnitHeader};
use gimli::{EndianSlice, RunTimeEndian};
use indexmap::IndexMap;
use object::read::ObjectSection;
use object::{Endianness, Object};
use std::ffi::OsStr;
use std::ops::Index;
use std::{collections::HashMap, fs::File};

type SliceType<'a> = EndianSlice<'a, RunTimeEndian>;

mod attributes;
use attributes::{
    get_abstract_origin_attribute, get_location_attribute, get_name_attribute,
    get_specification_attribute, get_typeref_attribute,
};
mod typereader;

pub(crate) struct UnitList<'a> {
    list: Vec<(UnitHeader<SliceType<'a>>, gimli::Abbreviations)>,
}

struct DebugDataReader<'elffile> {
    dwarf: Dwarf<EndianSlice<'elffile, RunTimeEndian>>,
    verbose: bool,
    units: UnitList<'elffile>,
    unit_names: Vec<Option<String>>,
    endian: Endianness,
    sections: HashMap<String, (u64, u64)>,
}

// load the debug info from an elf file
pub(crate) fn load_dwarf(filename: &OsStr, verbose: bool) -> Result<DebugData, String> {
    let filedata = load_filedata(filename)?;
    let elffile = load_elf_file(&filename.to_string_lossy(), &filedata)?;

    if !elffile
        .sections()
        .any(|section| section.name() == Ok(".debug_info"))
    {
        return Err(format!(
            "Error: {} does not contain DWARF2+ debug info. The section .debug_info is missing.",
            filename.to_string_lossy()
        ));
    }

    let dwarf = load_dwarf_sections(&elffile)?;

    if !verify_dwarf_compile_units(&dwarf) {
        return Err(format!(
            "Error: {} does not contain DWARF2+ debug info - zero compile units contain debug info.",
            filename.to_string_lossy()
        ));
    }

    let sections = get_elf_sections(&elffile);

    let dbg_reader = DebugDataReader {
        dwarf,
        verbose,
        units: UnitList::new(),
        unit_names: Vec::new(),
        endian: elffile.endianness(),
        sections,
    };

    Ok(dbg_reader.read_debug_info_entries())
}

// open a file and mmap its content
fn load_filedata(filename: &OsStr) -> Result<memmap2::Mmap, String> {
    let file = match File::open(filename) {
        Ok(file) => file,
        Err(error) => {
            return Err(format!(
                "Error: could not open file {}: {error}",
                filename.to_string_lossy()
            ));
        }
    };

    match unsafe { memmap2::Mmap::map(&file) } {
        Ok(mmap) => Ok(mmap),
        Err(err) => Err(format!(
            "Error: Failed to map file '{}': {err}",
            filename.to_string_lossy()
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

fn get_elf_sections(elffile: &object::read::File) -> HashMap<String, (u64, u64)> {
    let mut map = HashMap::new();

    for section in elffile.sections() {
        let addr = section.address();
        let size = section.size();
        if addr != 0
            && size != 0
            && let Ok(name) = section.name()
        {
            map.insert(name.to_string(), (addr, addr + size));
        }
    }

    map
}

// load the DWARF debug info from the .debug_<xyz> sections
fn load_dwarf_sections<'data>(
    elffile: &object::read::File<'data>,
) -> Result<gimli::Dwarf<SliceType<'data>>, String> {
    // Dwarf::load takes two closures / functions and uses them to load all the required debug sections
    let loader = |section: gimli::SectionId| get_file_section_reader(elffile, section.name());
    gimli::Dwarf::load(loader)
}

// verify that the dwarf data is valid
fn verify_dwarf_compile_units(dwarf: &gimli::Dwarf<SliceType>) -> bool {
    let mut units_iter = dwarf.debug_info.units();
    let mut units_count = 0;
    while let Ok(Some(_)) = units_iter.next() {
        units_count += 1;
    }

    units_count > 0
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

impl DebugDataReader<'_> {
    // read the debug information entries in the DWAF data to get all the global variables and their types
    fn read_debug_info_entries(mut self) -> DebugData {
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
            sections: self.sections,
        }
    }

    // load all global variables from the dwarf data
    fn load_variables(&mut self) -> IndexMap<String, Vec<VarInfo>> {
        let mut variables = IndexMap::<String, Vec<VarInfo>>::new();

        let mut iter = self.dwarf.debug_info.units();
        while let Ok(Some(unit)) = iter.next() {
            let Ok(abbreviations) = unit.abbreviations(&self.dwarf.debug_abbrev) else {
                if self.verbose {
                    let offset = unit
                        .offset()
                        .as_debug_info_offset()
                        .unwrap_or(gimli::DebugInfoOffset(0))
                        .0;
                    println!("Error: Failed to get abbreviations for unit @{offset:x}");
                }
                continue;
            };
            self.units.add(unit, abbreviations);
            let unit_idx = self.units.list.len() - 1;
            let (unit, abbreviations) = &self.units[unit_idx];

            // The root of the tree inside of a unit is always a DW_TAG_compile_unit or DW_TAG_partial_unit.
            // The global variables are among the immediate children of the unit; static variables
            // in functions are declared inside of DW_TAG_subprogram[/DW_TAG_lexical_block]*.
            // We can easily find all of them by using depth-first traversal of the tree
            let mut entries_cursor = unit.entries(abbreviations);
            if let Ok(Some((_, entry))) = entries_cursor.next_dfs()
                && (entry.tag() == gimli::constants::DW_TAG_compile_unit
                    || entry.tag() == gimli::constants::DW_TAG_partial_unit)
            {
                self.unit_names
                    .push(get_name_attribute(entry, &self.dwarf, unit).ok());
            }

            let mut depth = 0;
            let mut context: Vec<(gimli::DwTag, Option<String>)> = Vec::new();
            while let Ok(Some((depth_delta, entry))) = entries_cursor.next_dfs() {
                depth += depth_delta;
                debug_assert!(depth >= 1);
                context.truncate((depth - 1) as usize);
                let tag = entry.tag();
                // It's essential to only get those names that might actually be needed.
                // Getting all names unconditionally doubled the runtime of the program
                // as a result of countless useless string allocations and deallocations.
                if tag == gimli::constants::DW_TAG_namespace
                    || tag == gimli::constants::DW_TAG_subprogram
                {
                    context.push((tag, get_name_attribute(entry, &self.dwarf, unit).ok()));
                } else {
                    context.push((tag, None));
                }
                debug_assert_eq!(depth as usize, context.len());

                if entry.tag() == gimli::constants::DW_TAG_variable {
                    match self.get_global_variable(entry, unit, abbreviations) {
                        Ok(Some((name, typeref, address))) => {
                            let (function, namespaces) = get_varinfo_from_context(&context);
                            variables.entry(name).or_default().push(VarInfo {
                                address,
                                typeref,
                                unit_idx,
                                function,
                                namespaces,
                            });
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

fn get_varinfo_from_context(
    context: &[(gimli::DwTag, Option<String>)],
) -> (Option<String>, Vec<String>) {
    let function = context
        .iter()
        .rev()
        .find(|(tag, _)| *tag == gimli::constants::DW_TAG_subprogram)
        .and_then(|(_, name)| name.clone());
    let namespaces: Vec<String> = context
        .iter()
        .rev()
        .filter_map(|(tag, ns)| {
            (*tag == gimli::constants::DW_TAG_namespace)
                .then(|| ns.clone())
                .flatten()
        })
        .collect();
    (function, namespaces)
}

fn demangle_cpp_varnames(input: &[&String]) -> HashMap<String, String> {
    let mut demangled_symbols = HashMap::<String, String>::new();
    let demangle_opts = cpp_demangle::DemangleOptions::new()
        .no_params()
        .no_return_type();
    for varname in input {
        // some really simple strings can be processed by the demangler, e.g "c" -> "const", which is wrong here.
        // by only processing symbols that start with _Z (variables in classes/namespaces) this problem is avoided
        if varname.starts_with("_Z")
            && let Ok(sym) = cpp_demangle::Symbol::new(*varname)
        {
            // exclude useless demangled names like "typeinfo for std::type_info" or "{vtable(std::type_info)}"
            if let Ok(demangled) = sym.demangle_with_options(&demangle_opts)
                && !demangled.contains(' ')
                && !demangled.starts_with("{vtable")
            {
                demangled_symbols.insert(demangled, (*varname).clone());
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

#[cfg(test)]
mod test {
    use super::*;

    static ELF_FILE_NAMES: [&str; 4] = [
        "fixtures/bin/debugdata_clang.elf",
        "fixtures/bin/debugdata_clang_dw4.elf",
        "fixtures/bin/debugdata_gcc.elf",
        "fixtures/bin/debugdata_gcc_dw3.elf",
    ];

    #[test]
    fn test_load_data() {
        for filename in ELF_FILE_NAMES {
            let debugdata = DebugData::load_dwarf(OsStr::new(filename), true).unwrap();
            assert_eq!(debugdata.variables.len(), 28);
            assert!(debugdata.variables.get("class1").is_some());
            assert!(debugdata.variables.get("class2").is_some());
            assert!(debugdata.variables.get("class3").is_some());
            assert!(debugdata.variables.get("class4").is_some());
            assert!(debugdata.variables.get("staticvar").is_some());
            assert!(debugdata.variables.get("structvar").is_some());
            assert!(debugdata.variables.get("bitfield").is_some());

            for (_, varinfo) in &debugdata.variables {
                assert!(debugdata.types.contains_key(&varinfo[0].typeref));
            }

            let varinfo = debugdata.variables.get("class1").unwrap();
            let typeinfo = debugdata.types.get(&varinfo[0].typeref).unwrap();
            assert!(matches!(
                typeinfo,
                TypeInfo {
                    datatype: DbgDataType::Class { .. },
                    ..
                }
            ));
            if let TypeInfo {
                datatype:
                    DbgDataType::Class {
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
                            datatype: DbgDataType::Sint16,
                            ..
                        },
                        _
                    ))
                ));
                assert!(matches!(
                    members.get("base1_var"),
                    Some((
                        TypeInfo {
                            datatype: DbgDataType::Sint32,
                            ..
                        },
                        _
                    ))
                ));
                assert!(matches!(
                    members.get("base2var"),
                    Some((
                        TypeInfo {
                            datatype: DbgDataType::Sint32,
                            ..
                        },
                        _
                    ))
                ));
            }

            let varinfo = debugdata.variables.get("class2").unwrap();
            let typeinfo = debugdata.types.get(&varinfo[0].typeref).unwrap();
            assert!(matches!(
                typeinfo,
                TypeInfo {
                    datatype: DbgDataType::Class { .. },
                    ..
                }
            ));

            let varinfo = debugdata.variables.get("class3").unwrap();
            let typeinfo = debugdata.types.get(&varinfo[0].typeref).unwrap();
            assert!(matches!(
                typeinfo,
                TypeInfo {
                    datatype: DbgDataType::Class { .. },
                    ..
                }
            ));

            let varinfo = debugdata.variables.get("class4").unwrap();
            let typeinfo = debugdata.types.get(&varinfo[0].typeref).unwrap();
            assert!(matches!(
                typeinfo,
                TypeInfo {
                    datatype: DbgDataType::Class { .. },
                    ..
                }
            ));

            let varinfo = debugdata.variables.get("staticvar").unwrap();
            let typeinfo = debugdata.types.get(&varinfo[0].typeref).unwrap();
            assert!(matches!(
                typeinfo,
                TypeInfo {
                    datatype: DbgDataType::Sint32,
                    ..
                }
            ));

            let varinfo = debugdata.variables.get("structvar").unwrap();
            let typeinfo = debugdata.types.get(&varinfo[0].typeref).unwrap();
            assert!(matches!(
                typeinfo,
                TypeInfo {
                    datatype: DbgDataType::Struct { .. },
                    ..
                }
            ));

            let varinfo = debugdata.variables.get("bitfield").unwrap();
            let typeinfo = debugdata.types.get(&varinfo[0].typeref).unwrap();
            assert!(matches!(
                typeinfo,
                TypeInfo {
                    datatype: DbgDataType::Struct { .. },
                    ..
                }
            ));
            if let TypeInfo {
                datatype: DbgDataType::Struct { members, .. },
                ..
            } = typeinfo
            {
                assert!(matches!(
                    members.get("var"),
                    Some((
                        TypeInfo {
                            datatype: DbgDataType::Bitfield {
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
                            datatype: DbgDataType::Bitfield {
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
                            datatype: DbgDataType::Bitfield {
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
                            datatype: DbgDataType::Bitfield {
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
            let typeinfo = debugdata.types.get(&varinfo[0].typeref).unwrap();
            assert!(matches!(
                typeinfo,
                TypeInfo {
                    datatype: DbgDataType::Enum { .. },
                    ..
                }
            ));
            let varinfo = debugdata.variables.get("enum_var2").unwrap();
            let typeinfo = debugdata.types.get(&varinfo[0].typeref).unwrap();
            assert!(matches!(
                typeinfo,
                TypeInfo {
                    datatype: DbgDataType::Enum { .. },
                    ..
                }
            ));
            let varinfo = debugdata.variables.get("enum_var3").unwrap();
            let typeinfo = debugdata.types.get(&varinfo[0].typeref).unwrap();
            assert!(matches!(
                typeinfo,
                TypeInfo {
                    datatype: DbgDataType::Enum { .. },
                    ..
                }
            ));

            let varinfo = debugdata.variables.get("var_array").unwrap();
            let typeinfo = debugdata.types.get(&varinfo[0].typeref).unwrap();
            let DbgDataType::Array {
                size,
                dim,
                arraytype,
                ..
            } = &typeinfo.datatype
            else {
                panic!("Expected array type, got {:?}", typeinfo.datatype);
            };
            assert_eq!(*size, 33);
            assert_eq!(dim.len(), 1);
            assert_eq!(dim[0], 33);
            assert!(matches!(arraytype.datatype, DbgDataType::Uint8));

            let varinfo = debugdata.variables.get("var_multidim").unwrap();
            let typeinfo = debugdata.types.get(&varinfo[0].typeref).unwrap();
            let DbgDataType::Array { dim, arraytype, .. } = &typeinfo.datatype else {
                panic!("Expected array type, got {:?}", typeinfo.datatype);
            };
            assert_eq!(dim.len(), 3);
            assert_eq!(dim, &[10, 3, 7]);
            assert!(matches!(arraytype.datatype, DbgDataType::Float));
        }
    }

    #[test]
    fn test_load_mingw_exe() {
        // The file fixtures/bin/update_test.c was compiled with mingw64 gcc
        // (update_test.exe) as well as with gcc for arm (update_test.elf).
        // Both file contain the same debug information, though the windows exe
        // file has some additional items from the starup code.
        let debugdata_exe =
            DebugData::load_dwarf(OsStr::new("fixtures/bin/update_test.exe"), true).unwrap();
        let debugdata_elf =
            DebugData::load_dwarf(OsStr::new("fixtures/bin/update_test.elf"), true).unwrap();

        // every variable in the elf file should also be in the exe file
        for var in debugdata_elf.variables.keys() {
            assert!(debugdata_exe.variables.contains_key(var));
        }
    }

    #[test]
    fn test_load_mingw_exe2() {
        // Both file contain the same debug information, though the windows exe
        // file has some additional items from the starup code.
        let debugdata_exe =
            DebugData::load_dwarf(OsStr::new("fixtures/bin/debugdata_gcc.exe"), true).unwrap();
        let debugdata_elf =
            DebugData::load_dwarf(OsStr::new("fixtures/bin/debugdata_gcc.elf"), true).unwrap();

        // every variable in the elf file should also be in the exe file
        for var in debugdata_elf.variables.keys() {
            assert!(debugdata_exe.variables.contains_key(var));
        }
    }
}
