use super::attributes::*;
use super::{TypeInfo, UnitList, VarInfo};
use gimli::{EndianSlice, EntriesTreeNode, RunTimeEndian};
use std::collections::HashMap;

// load all the types referenced by variables in given HashMap
pub(crate) fn load_types(
    variables: &HashMap<String, VarInfo>,
    typedefs: &HashMap<usize, String>,
    units: &UnitList,
    dwarf: &gimli::Dwarf<EndianSlice<RunTimeEndian>>,
    verbose: bool,
) -> HashMap<usize, TypeInfo> {
    let mut types = HashMap::<usize, TypeInfo>::new();
    // for each variable
    for (name, VarInfo { typeref, .. }) in variables {
        // check if the type was already loaded
        if types.get(typeref).is_none() {
            if let Some(unit_idx) = units.get_unit(*typeref) {
                // create an entries_tree iterator that makes it possible to read the DIEs of this type
                let (unit, abbrev) = &units[unit_idx];
                let dbginfo_offset = gimli::DebugInfoOffset(*typeref);
                let unit_offset = dbginfo_offset.to_unit_offset(unit).unwrap();
                let mut entries_tree = unit.entries_tree(abbrev, Some(unit_offset)).unwrap();

                // load one type and add it to the collection (always succeeds for correctly structured DWARF debug info)
                match get_type(
                    units,
                    unit_idx,
                    entries_tree.root().unwrap(),
                    None,
                    typedefs,
                    dwarf,
                ) {
                    Ok(vartype) => {
                        types.insert(*typeref, vartype);
                    }
                    Err(errmsg) => {
                        if verbose {
                            println!("Error loading type info for variable {name}: {errmsg}");
                        }
                    }
                }
            }
        }
    }

    types
}

// get one type from the debug info
fn get_type(
    unit_list: &UnitList,
    current_unit: usize,
    entries_tree_node: EntriesTreeNode<EndianSlice<RunTimeEndian>>,
    typedef_name: Option<String>,
    typedefs: &HashMap<usize, String>,
    dwarf: &gimli::Dwarf<EndianSlice<RunTimeEndian>>,
) -> Result<TypeInfo, String> {
    let entry = entries_tree_node.entry();
    match entry.tag() {
        gimli::constants::DW_TAG_base_type => get_base_type(entry, &unit_list[current_unit].0),
        gimli::constants::DW_TAG_pointer_type => {
            let (unit, _) = &unit_list[current_unit];
            Ok(TypeInfo::Pointer(u64::from(unit.encoding().address_size)))
        }
        gimli::constants::DW_TAG_array_type => {
            let maybe_size = get_byte_size_attribute(entry);
            let (new_cur_unit, mut new_entries_tree) =
                get_entries_tree_from_attribute(entry, unit_list, current_unit)?;
            let arraytype = get_type(
                unit_list,
                new_cur_unit,
                new_entries_tree.root().unwrap(),
                None,
                typedefs,
                dwarf,
            )?;

            let mut dim = Vec::<u64>::new();

            // If the stride of the array is different from the size of each element, then the stride must be given as an attribute
            let stride = if let Some(stride) = get_byte_stride_attribute(entry) {
                stride
            } else {
                // this is the usual case
                arraytype.get_size()
            };

            let default_ubound = maybe_size.map(|s| s / stride - 1); // subtract 1, because ubound is the last element, not the size

            // the child entries of the DW_TAG_array_type entry give the array dimensions
            let mut iter = entries_tree_node.children();
            while let Ok(Some(child_node)) = iter.next() {
                let child_entry = child_node.entry();
                if child_entry.tag() == gimli::constants::DW_TAG_subrange_type {
                    let ubound = get_upper_bound_attribute(child_entry)
                        .or(default_ubound)
                        .ok_or_else(|| {
                            "error decoding array info: neither size nor ubound available".to_string()
                        })?;
                    dim.push(ubound + 1);
                }
            }

            // Use parsed size or determine the size from the array dimensions
            let size = maybe_size.unwrap_or_else(|| dim.iter().fold(1, |acc, num| acc * num));
            Ok(TypeInfo::Array {
                dim,
                arraytype: Box::new(arraytype),
                size,
                stride,
            })
        }
        gimli::constants::DW_TAG_enumeration_type => {
            let size = get_byte_size_attribute(entry)
                .ok_or_else(|| "missing enum byte size attribute".to_string())?;
            let mut enumerators = Vec::new();
            let (unit, _) = &unit_list[current_unit];
            let dioffset = entry.offset().to_debug_info_offset(unit).unwrap().0;

            let typename = if let Some(name) = typedef_name {
                // enum referenced by a typedef: the compiler generated debuginfo that had e.g.
                //   variable -> typedef -> (named or anonymous) enum
                name
            } else if let Ok(name_from_attr) = get_name_attribute(entry, dwarf) {
                // named enum that is not directly referenced by a typedef. It might still have been typedef'd in the original code.
                name_from_attr
            } else if let Some(name) = typedefs.get(&dioffset) {
                // anonymous enum, with a typedef name recovered from the global list
                // the compiler had the typedef info at compile time, but didn't refer to it in the debug info
                name.to_owned()
            } else {
                // a truly anonymous enum. This can happen if someone writes C code that looks like this:
                // enum { ... } varname;
                format!("anonymous_enum_{dioffset}")
            };

            let mut iter = entries_tree_node.children();
            // get all the enumerators
            while let Ok(Some(child_node)) = iter.next() {
                let child_entry = child_node.entry();
                if child_entry.tag() == gimli::constants::DW_TAG_enumerator {
                    let name = get_name_attribute(child_entry, dwarf)
                        .map_err(|_| "missing enum item name".to_string())?;
                    let value = get_const_value_attribute(child_entry)
                        .ok_or_else(|| "missing enum item value".to_string())?;
                    enumerators.push((name, value));
                }
            }
            Ok(TypeInfo::Enum {
                typename,
                size,
                enumerators,
            })
        }
        gimli::constants::DW_TAG_structure_type => {
            let size = get_byte_size_attribute(entry)
                .ok_or_else(|| "missing struct byte size attribute".to_string())?;
            let members = get_struct_or_union_members(
                entries_tree_node,
                unit_list,
                current_unit,
                typedefs,
                dwarf,
            )?;
            Ok(TypeInfo::Struct { size, members })
        }
        gimli::constants::DW_TAG_class_type => {
            let size = get_byte_size_attribute(entry)
                .ok_or_else(|| "missing class byte size attribute".to_string())?;

            // construct a second entries_tree_node, since both get_class_inheritance and get_struct_or_union_members need to own it
            let (unit, abbrev) = &unit_list[current_unit];
            let mut entries_tree2 = unit
                .entries_tree(abbrev, Some(entries_tree_node.entry().offset()))
                .unwrap();
            let entries_tree_node2 = entries_tree2.root().unwrap();

            // get the inheritance, i.e. the list of base classes
            let inheritance = get_class_inheritance(
                entries_tree_node2,
                unit_list,
                current_unit,
                typedefs,
                dwarf,
            )?;
            // get the list of data members
            let mut members = get_struct_or_union_members(
                entries_tree_node,
                unit_list,
                current_unit,
                typedefs,
                dwarf,
            )?;
            // make inherited members visible directly
            for (baseclass_type, baseclass_offset) in inheritance.values() {
                if let TypeInfo::Class {
                    members: baseclass_members,
                    ..
                } = baseclass_type
                {
                    for (name, (m_type, m_offset)) in baseclass_members {
                        members.insert(
                            name.to_owned(),
                            (m_type.clone(), m_offset + baseclass_offset),
                        );
                    }
                }
            }
            Ok(TypeInfo::Class {
                size,
                inheritance,
                members,
            })
        }
        gimli::constants::DW_TAG_union_type => {
            let size = get_byte_size_attribute(entry)
                .ok_or_else(|| "missing union byte size attribute".to_string())?;
            let members = get_struct_or_union_members(
                entries_tree_node,
                unit_list,
                current_unit,
                typedefs,
                dwarf,
            )?;
            Ok(TypeInfo::Union { size, members })
        }
        gimli::constants::DW_TAG_typedef => {
            let name = get_name_attribute(entry, dwarf)?;
            let (new_cur_unit, mut new_entries_tree) =
                get_entries_tree_from_attribute(entry, unit_list, current_unit)?;
            get_type(
                unit_list,
                new_cur_unit,
                new_entries_tree.root().unwrap(),
                Some(name),
                typedefs,
                dwarf,
            )
        }
        gimli::constants::DW_TAG_const_type | gimli::constants::DW_TAG_volatile_type => {
            let (new_cur_unit, mut new_entries_tree) =
                get_entries_tree_from_attribute(entry, unit_list, current_unit)?;
            get_type(
                unit_list,
                new_cur_unit,
                new_entries_tree.root().unwrap(),
                typedef_name,
                typedefs,
                dwarf,
            )
        }
        other_tag => Err(format!(
            "unexpected DWARF tag {other_tag} in type definition"
        )),
    }
}

fn get_base_type(
    entry: &gimli::DebuggingInformationEntry<EndianSlice<RunTimeEndian>, usize>,
    unit: &gimli::UnitHeader<EndianSlice<RunTimeEndian>>,
) -> Result<TypeInfo, String> {
    let byte_size = get_byte_size_attribute(entry).unwrap_or(1u64);
    let encoding = get_encoding_attribute(entry).unwrap_or(gimli::constants::DW_ATE_unsigned);
    Ok(match encoding {
        gimli::constants::DW_ATE_address => {
            TypeInfo::Pointer(u64::from(unit.encoding().address_size))
        }
        gimli::constants::DW_ATE_float => {
            if byte_size == 8 {
                TypeInfo::Double
            } else {
                TypeInfo::Float
            }
        }
        gimli::constants::DW_ATE_signed | gimli::constants::DW_ATE_signed_char => match byte_size {
            1 => TypeInfo::Sint8,
            2 => TypeInfo::Sint16,
            4 => TypeInfo::Sint32,
            8 => TypeInfo::Sint64,
            val => {
                return Err(format!("error loading data type: signed int of size {val}"));
            }
        },
        gimli::constants::DW_ATE_boolean
        | gimli::constants::DW_ATE_unsigned
        | gimli::constants::DW_ATE_unsigned_char => match byte_size {
            1 => TypeInfo::Uint8,
            2 => TypeInfo::Uint16,
            4 => TypeInfo::Uint32,
            8 => TypeInfo::Uint64,
            val => {
                return Err(format!(
                    "error loading data type: unsigned int of size {val}"
                ));
            }
        },
        _other => TypeInfo::Other(byte_size),
    })
}

// get all the members of a struct or union or class
fn get_struct_or_union_members(
    entries_tree: EntriesTreeNode<EndianSlice<RunTimeEndian>>,
    unit_list: &UnitList,
    current_unit: usize,
    typedefs: &HashMap<usize, String>,
    dwarf: &gimli::Dwarf<EndianSlice<RunTimeEndian>>,
) -> Result<HashMap<String, (TypeInfo, u64)>, String> {
    let (unit, _) = &unit_list[current_unit];
    let mut members = HashMap::<String, (TypeInfo, u64)>::new();
    let mut iter = entries_tree.children();
    while let Ok(Some(child_node)) = iter.next() {
        let child_entry = child_node.entry();
        if child_entry.tag() == gimli::constants::DW_TAG_member {
            let name = get_name_attribute(child_entry, dwarf)
                .map_err(|_| "missing struct/union member name".to_string())?;
            let offset = get_data_member_location_attribute(child_entry, unit.encoding())
                .ok_or_else(|| "missing byte offset for struct/union member".to_string())?;
            let bitsize = get_bit_size_attribute(child_entry);
            let bitoffset = get_bit_offset_attribute(child_entry);
            let (new_cur_unit, mut new_entries_tree) =
                get_entries_tree_from_attribute(child_entry, unit_list, current_unit)?;
            let mut membertype = get_type(
                unit_list,
                new_cur_unit,
                new_entries_tree.root().unwrap(),
                None,
                typedefs,
                dwarf,
            )?;

            // wrap bitfield members in a TypeInfo::Bitfield to store bit_size and bit_offset
            if let Some(bit_size) = bitsize {
                if let Some(bit_offset) = bitoffset {
                    membertype = TypeInfo::Bitfield {
                        basetype: Box::new(membertype),
                        bit_size: bit_size as u16,
                        bit_offset: bit_offset as u16,
                    };
                }
            }
            members.insert(name, (membertype, offset));
        }
    }
    Ok(members)
}

// get all the members of a struct or union or class
fn get_class_inheritance(
    entries_tree: EntriesTreeNode<EndianSlice<RunTimeEndian>>,
    unit_list: &UnitList,
    current_unit: usize,
    typedefs: &HashMap<usize, String>,
    dwarf: &gimli::Dwarf<EndianSlice<RunTimeEndian>>,
) -> Result<HashMap<String, (TypeInfo, u64)>, String> {
    let (unit, _) = &unit_list[current_unit];
    let mut inheritance = HashMap::<String, (TypeInfo, u64)>::new();
    let mut iter = entries_tree.children();
    while let Ok(Some(child_node)) = iter.next() {
        let child_entry = child_node.entry();
        if child_entry.tag() == gimli::constants::DW_TAG_inheritance {
            let offset = get_data_member_location_attribute(child_entry, unit.encoding())
                .ok_or_else(|| "missing byte offset for inherited class".to_string())?;
            let (new_cur_unit, mut baseclass_tree) =
                get_entries_tree_from_attribute(child_entry, unit_list, current_unit)?;

            let baseclass_tree_node = baseclass_tree.root().unwrap();
            let baseclass_entry = baseclass_tree_node.entry();
            let baseclass_name = get_name_attribute(baseclass_entry, dwarf)?;

            let baseclass_type = get_type(
                unit_list,
                new_cur_unit,
                baseclass_tree.root().unwrap(),
                None,
                typedefs,
                dwarf,
            )?;

            inheritance.insert(baseclass_name, (baseclass_type, offset));
        }
    }
    Ok(inheritance)
}
