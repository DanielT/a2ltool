use super::dwarf::{DebugData, TypeInfo};

#[cfg(test)]
use std::collections::HashMap;

// find a symbol in the elf_info data structure that was derived from the DWARF debug info in the elf file
pub(crate) fn find_symbol<'a>(
    varname: &str,
    debug_data: &'a DebugData,
) -> Result<(String, u64, &'a TypeInfo), String> {
    // split the a2l symbol name: e.g. "motortune.param._0_" -> ["motortune", "param", "_0_"]
    let components = split_symbol_components(varname);

    // find the symbol in the symbol table
    match find_symbol_from_components(&components, debug_data) {
        Ok((addr, typeinfo)) => {
            Ok((varname.to_owned(), addr, typeinfo))
        }
        Err(find_err) => {
            // it was not found using the given varname; if this is name has a mangled form then try that instead
            if let Some(mangled) = debug_data.demangled_names.get(components[0]) {
                let mut components_mangled = components.clone();
                components_mangled[0] = mangled;
                if let Ok((addr, typeinfo)) = find_symbol_from_components(&components_mangled, debug_data) {
                    let mangled_varname = mangled.to_owned() + varname.strip_prefix(components[0]).unwrap();
                    return Ok((mangled_varname, addr, typeinfo))
                }
            }

            Err(find_err)
        }
    }
}

fn find_symbol_from_components<'a>(
    components: &[&str],
    debug_data: &'a DebugData,
) -> Result<(u64, &'a TypeInfo), String> {
    // the first component of the symbol name is the name of the global variable.
    if let Some(varinfo) = debug_data.variables.get(components[0]) {
        // we also need the type in order to resolve struct members, etc.
        if let Some(vartype) = debug_data.types.get(&varinfo.typeref) {
            // all further components of the symbol name are struct/union members or array indices
            find_membertype(vartype, components, 1, varinfo.address)
        } else {
            // this exists for completeness, but shouldn't happen with a correctly generated elffile
            // if the variable is present in the elffile, then the type should also be present
            if components.len() == 1 {
                Ok((varinfo.address, &TypeInfo::Uint8))
            } else {
                Err(format!(
                    "Remaining portion \"{}\" of \"{}\" could not be matched",
                    components[1..].join("."),
                    components.join(".")
                ))
            }
        }
    } else {
        Err(format!("Symbol \"{}\" does not exist", components[0]))
    }
}

// split the symbol into components
// e.g. "my_struct.array_field[5][6]" -> [ "my_struct", "array_field", "[5]", "[6]" ]
fn split_symbol_components(varname: &str) -> Vec<&str> {
    let mut components: Vec<&str> = Vec::new();

    for component in varname.split('.') {
        if let Some(idx) = component.find('[') {
            // "array_field[5][6]" -> "array_field", "[5][6]"
            let (name, indexstring) = component.split_at(idx);
            components.push(name);
            components.extend(indexstring.split_inclusive(']'));
        } else {
            components.push(component);
        }
    }

    components
}

// find the address and type of the current component of a symbol name
fn find_membertype<'a>(
    typeinfo: &'a TypeInfo,
    components: &[&str],
    component_index: usize,
    address: u64,
) -> Result<(u64, &'a TypeInfo), String> {
    if component_index >= components.len() {
        Ok((address, typeinfo))
    } else {
        match typeinfo {
            TypeInfo::Class { members, .. }
            | TypeInfo::Struct { members, .. }
            | TypeInfo::Union { members, .. } => {
                if let Some((membertype, offset)) = members.get(components[component_index]) {
                    find_membertype(
                        membertype,
                        components,
                        component_index + 1,
                        address + offset,
                    )
                } else {
                    Err(format!(
                        "There is no member \"{}\" in \"{}\"",
                        components[component_index],
                        components[..component_index].join(".")
                    ))
                }
            }
            TypeInfo::Array {
                dim,
                stride,
                arraytype,
                ..
            } => {
                let mut multi_index = 0;
                for (idx_pos, current_dim) in dim.iter().enumerate() {
                    let arraycomponent =
                        components.get(component_index + idx_pos).unwrap_or(&"_0_"); // default to first element if no more components are specified
                    let indexval = get_index(arraycomponent).ok_or_else(|| {
                        format!(
                            "could not interpret \"{}\" as an array index",
                            arraycomponent
                        )
                    })?;
                    if indexval >= *current_dim as usize {
                        return Err(format!("requested array index {} in expression \"{}\", but the array only has {} elements",
                            indexval, components.join("."), current_dim));
                    }
                    multi_index = multi_index * (*current_dim) as usize + indexval;
                }

                let elementaddr = address + (multi_index as u64 * stride);
                find_membertype(
                    arraytype,
                    components,
                    component_index + dim.len(),
                    elementaddr,
                )
            }
            _ => {
                if component_index >= components.len() {
                    Ok((address, typeinfo))
                } else {
                    // could not descend further to match additional symbol name components
                    Err(format!(
                        "Remaining portion \"{}\" of \"{}\" could not be matched",
                        components[component_index..].join("."),
                        components.join(".")
                    ))
                }
            }
        }
    }
}

// before ASAP2 1.7 array indices in symbol names could not written as [x], but only as _x_
// this function will get the numerical index for either representation
fn get_index(idxstr: &str) -> Option<usize> {
    if (idxstr.starts_with('_') && idxstr.ends_with('_'))
        || (idxstr.starts_with('[') && idxstr.ends_with(']'))
    {
        let idxstrlen = idxstr.len();
        match idxstr[1..(idxstrlen - 1)].parse() {
            Ok(val) => Some(val),
            Err(_) => None,
        }
    } else {
        None
    }
}

#[test]
fn test_split_symbol_components() {
    let result = split_symbol_components("my_struct.array_field[5][1]");
    assert_eq!(result.len(), 4);
    assert_eq!(result[0], "my_struct");
    assert_eq!(result[1], "array_field");
    assert_eq!(result[2], "[5]");
    assert_eq!(result[3], "[1]");

    let result2 = split_symbol_components("my_struct.array_field._5_._1_");
    assert_eq!(result2.len(), 4);
    assert_eq!(result2[0], "my_struct");
    assert_eq!(result2[1], "array_field");
    assert_eq!(result2[2], "_5_");
    assert_eq!(result2[3], "_1_");
}

#[test]
fn test_find_symbol_of_array() {
    let mut dbgdata = DebugData {
        types: HashMap::new(),
        variables: HashMap::new(),
        demangled_names: HashMap::new(),
    };
    // global variable: uint32_t my_array[2]
    dbgdata.variables.insert(
        "my_array".to_string(),
        crate::dwarf::VarInfo {
            address: 0x1234,
            typeref: 1,
        },
    );
    dbgdata.types.insert(
        1,
        TypeInfo::Array {
            arraytype: Box::new(TypeInfo::Uint32),
            dim: vec![2],
            size: 8, // total size of the array
            stride: 4,
        },
    );

    // try the different array indexing notations
    let result1 = find_symbol("my_array._0_", &dbgdata);
    assert!(result1.is_ok());
    // C-style notation is only allowed starting with ASAP2 version 1.7, before that the '[' and ']' are not allowed in names
    let result2 = find_symbol("my_array[0]", &dbgdata);
    assert!(result2.is_ok());

    // it should also be possible to get a typeref for the entire array
    let result3 = find_symbol("my_array", &dbgdata);
    assert!(result3.is_ok());

    // there should not be a result if the symbol name contains extra unmatched components
    let result4 = find_symbol("my_array._0_.lalala", &dbgdata);
    assert!(result4.is_err());
    // going past the end of the array is also not permitted
    let result5 = find_symbol("my_array._2_", &dbgdata);
    assert!(result5.is_err());
}

#[test]
fn test_find_symbol_of_array_in_struct() {
    let mut dbgdata = DebugData {
        types: HashMap::new(),
        variables: HashMap::new(),
        demangled_names: HashMap::new(),
    };
    // global variable defined in C like this:
    // struct {
    //        uint32_t array_item[2];
    // } my_struct;
    let mut structmembers: HashMap<String, (TypeInfo, u64)> = HashMap::new();
    structmembers.insert(
        "array_item".to_string(),
        (
            TypeInfo::Array {
                arraytype: Box::new(TypeInfo::Uint32),
                dim: vec![2],
                size: 8,
                stride: 4,
            },
            0,
        ),
    );
    dbgdata.variables.insert(
        "my_struct".to_string(),
        crate::dwarf::VarInfo {
            address: 0xcafe00,
            typeref: 2,
        },
    );
    dbgdata.types.insert(
        2,
        TypeInfo::Struct {
            members: structmembers,
            size: 4,
        },
    );

    // try the different array indexing notations
    let result1 = find_symbol("my_struct.array_item._0_", &dbgdata);
    assert!(result1.is_ok());
    // C-style notation is only allowed starting with ASAP2 version 1.7, before that the '[' and ']' are not allowed in names
    let result2 = find_symbol("my_struct.array_item[0]", &dbgdata);
    assert!(result2.is_ok());

    // theres should not be a result if the symbol name contains extra unmatched components
    let result3 = find_symbol("my_struct.array_item._0_.extra.unused", &dbgdata);
    assert!(result3.is_err());
}
