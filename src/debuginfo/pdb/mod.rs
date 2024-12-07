use crate::debuginfo::{DbgDataType, DebugData, VarInfo};
use indexmap::IndexMap;
use pdb2::{AddressMap, FallibleIterator, RawString, SymbolData, PDB};
use std::{collections::HashMap, ffi::OsStr, fs::File, vec};
use typereader::TypeReaderData;

use super::TypeInfo;

mod typereader;

struct ModuleVars {
    static_variables: IndexMap<String, Vec<VarInfo>>,
    unit_list: Vec<Option<String>>,
}

pub(crate) fn load_pdb(filename: &OsStr, _verbose: bool) -> Result<DebugData, String> {
    let file = File::open(filename).map_err(|ioerr| ioerr.to_string())?;
    let pdb = match PDB::open(file) {
        Ok(pdb) => pdb,
        Err(pdb2::Error::UnimplementedFeature(feat)) => {
            return Err(format!("PDB feature not implemented: {feat}"));
        }
        Err(pdb2::Error::IoError(ioerr)) => {
            return Err(ioerr.to_string());
        }
        Err(pdb2::Error::UnrecognizedFileFormat) => {
            return Err(format!(
                "Input file {} is not in PDB format",
                filename.to_string_lossy()
            ));
        }
        Err(pdb2::Error::PageReferenceOutOfRange(_) | pdb2::Error::InvalidPageSize(_)) => {
            return Err(format!(
                "Input file {} is corrupted",
                filename.to_string_lossy()
            ));
        }
        Err(err) => {
            return Err(format!(
                "Unknown error reading PDB file {}: {err}",
                filename.to_string_lossy()
            ));
        }
    };

    read_pdb(pdb).map_err(|pdberr| format!("PDB error: {pdberr:?}"))
}

fn read_pdb(mut pdb: PDB<'_, File>) -> Result<DebugData, pdb2::Error> {
    let address_map = pdb.address_map().unwrap();
    let global_variables = read_global_variables(&mut pdb, &address_map)?;
    let ModuleVars {
        static_variables,
        unit_list,
    } = read_static_variables(&mut pdb, &address_map)?;
    let mut variables = global_variables
        .into_iter()
        .chain(static_variables)
        .collect();

    let TypeReaderData {
        types, typenames, ..
    } = typereader::read_all_types(&mut pdb, &variables)?;

    filter_extern_variables(&mut variables, &types);

    // names in PDB debug info are not mangled, so we don't need to demangle them
    let demangled_names = HashMap::new();

    let mut sections = HashMap::new();
    if let Some(sections_list) = pdb.sections()? {
        for section in sections_list {
            let name = section.name().to_string();
            let virt_addr = section.virtual_address as u64;
            let length = section.virtual_size as u64;
            sections.insert(name, (virt_addr, virt_addr + length));
        }
    }

    Ok(DebugData {
        variables,
        types,
        typenames,
        demangled_names,
        unit_names: unit_list,
        sections,
    })
}

fn read_global_variables(
    pdb: &mut PDB<'_, File>,
    address_map: &AddressMap<'_>,
) -> Result<IndexMap<String, Vec<VarInfo>>, pdb2::Error> {
    let mut global_variables: IndexMap<String, Vec<VarInfo>> = IndexMap::new();

    let symbol_table = pdb.global_symbols()?;
    let mut symbols_iter = symbol_table.iter();
    while let Some(symbol) = symbols_iter.next()? {
        if let Ok(SymbolData::Data(data_symbol)) = symbol.parse() {
            let sym_full_name = data_symbol.name.to_string();
            let mut ns_components = sym_full_name
                .split("::")
                .map(|s| s.to_string())
                .collect::<Vec<_>>();
            let symbol_name = ns_components.pop();
            let virt_addr = data_symbol.offset.to_rva(address_map);
            if let (Some(symbol_name), Some(virt_addr)) = (symbol_name, virt_addr) {
                global_variables
                    .entry(symbol_name)
                    .or_default()
                    .push(VarInfo {
                        address: virt_addr.0 as u64,
                        typeref: data_symbol.type_index.0 as usize,
                        unit_idx: 0,
                        function: None,
                        namespaces: ns_components,
                    });
            }
        }
    }

    Ok(global_variables)
}

fn read_static_variables(
    pdb: &mut PDB<'_, File>,
    address_map: &AddressMap<'_>,
) -> Result<ModuleVars, pdb2::Error> {
    let mut modvars = ModuleVars {
        static_variables: IndexMap::new(),
        unit_list: vec![None],
    };

    let dbi = pdb.debug_information()?;
    let mut modules = dbi.modules()?;
    while let Some(module) = modules.next()? {
        modvars
            .unit_list
            .push(Some(module.module_name().to_string()));
        let info = match pdb.module_info(&module)? {
            Some(info) => info,
            None => {
                continue;
            }
        };

        let mut scope_stack: Vec<(bool, Option<RawString<'_>>)> = Vec::new();

        let mut symbols = info.symbols()?;
        while let Some(symbol) = symbols.next()? {
            if symbol.ends_scope() {
                scope_stack.pop();
            } else if symbol.starts_scope() {
                // immediately push the scope so that the symbol stack remains consistent even if parsing fails
                scope_stack.push((false, None));
            }

            if let Ok(symbol_data) = symbol.parse() {
                if symbol.starts_scope() {
                    // fix the info for the last symbol
                    scope_stack.last_mut().unwrap().1 = symbol_data.name();
                }

                if let SymbolData::Procedure(_) = symbol_data {
                    scope_stack.last_mut().unwrap().0 = true;
                } else if let SymbolData::Data(data_symbol) = symbol_data {
                    // skip unnamed symbols
                    if data_symbol.name.is_empty() {
                        continue;
                    }
                    let function_name: Option<String> = scope_stack
                        .iter()
                        .rfind(|(is_func, _)| *is_func)
                        .and_then(|(_, name)| *name)
                        .map(|name| name.to_string().into());
                    let sym_name: String = data_symbol.name.to_string().into();

                    let virt_addr = data_symbol.offset.to_rva(address_map);
                    if let Some(virt_addr) = virt_addr {
                        modvars
                            .static_variables
                            .entry(sym_name)
                            .or_default()
                            .push(VarInfo {
                                address: virt_addr.0 as u64,
                                typeref: data_symbol.type_index.0 as usize,
                                unit_idx: modvars.unit_list.len() - 1,
                                function: function_name,
                                namespaces: vec![],
                            });
                    }
                }
            }
        }
    }

    Ok(modvars)
}

// extern declarations cause duplicated variables to be created in the variables list
fn filter_extern_variables(
    variables: &mut IndexMap<String, Vec<VarInfo>>,
    types: &HashMap<usize, TypeInfo>,
) {
    for varinfo in variables.values_mut() {
        if varinfo.len() > 1 {
            // retain all elements which are not TypeRefs
            // in this context, a TypeRef indicates an extern delcaration
            varinfo.retain(|var| {
                if let Some(typeinfo) = types.get(&var.typeref) {
                    !matches!(&typeinfo.datatype, DbgDataType::TypeRef(_, _))
                } else {
                    true
                }
            });
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    static PDB_FILE_NAMES: [&str; 2] = [
        "fixtures/bin/debugdata_cl.pdb",
        "fixtures/bin/debugdata_clang.pdb",
    ];

    #[test]
    fn test_load_data() {
        for filename in PDB_FILE_NAMES {
            let debugdata = DebugData::load_pdb(OsStr::new(filename), true).unwrap();
            // unlike the ELF test, we can't check the exact number of variables
            // The elf files are built for bare-metal ARM, while the PDB files are built for Windows
            // Building form windows causes system libraries to be linked in, which creates a lot of extra variables
            assert!(debugdata.variables.len() >= 25);

            assert!(debugdata.variables.get("class1").is_some());
            assert!(debugdata.variables.get("class2").is_some());
            assert!(debugdata.variables.get("class3").is_some());
            assert!(debugdata.variables.get("class4").is_some());
            assert!(debugdata.variables.get("staticvar").is_some());
            // structvar, despite being present in the source and in the dwarf debug info is not present in the PDB
            // llvm-pdbinfo does not show it either, so maybe it got optimized away
            //assert!(debugdata.variables.get("structvar").is_some());
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
}
