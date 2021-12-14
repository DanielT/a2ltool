use std::collections::HashMap;
use gimli::{EndianSlice, EntriesTreeNode, RunTimeEndian};
use super::{UnitList, TypeInfo, VarInfo};
use super::attributes::*;


// load all the types referenced by variables in given HashMap
pub(crate) fn load_types(
    variables: &HashMap<String, VarInfo>,
    units: UnitList,
    dwarf: &gimli::Dwarf<EndianSlice<RunTimeEndian>>,
    verbose: bool
) -> HashMap<usize, TypeInfo> {
    let mut types = HashMap::<usize, TypeInfo>::new();
    // for each variable
    for (_name, VarInfo { typeref, ..}) in variables {
        // check if the type was already loaded
        if types.get(typeref).is_none() {
            if let Some(unit_idx) = units.get_unit(*typeref) {
                // create an entries_tree iterator that makes it possible to read the DIEs of this type
                let (unit, abbrev) = &units[unit_idx];
                let dbginfo_offset = gimli::DebugInfoOffset(*typeref);
                let unit_offset = dbginfo_offset.to_unit_offset(unit).unwrap();
                let mut entries_tree = unit.entries_tree(&abbrev, Some(unit_offset)).unwrap();

                // load one type and add it to the collection (always succeeds for correctly structured DWARF debug info)
                match get_type(&units, unit_idx, entries_tree.root().unwrap(), None, dwarf) {
                    Ok(vartype) => {
                        types.insert(*typeref, vartype);
                    }
                    Err(errmsg) => {
                        if verbose {
                            println!("Error loading type info for variable {}: {}", _name, errmsg);
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
    entries_tree: EntriesTreeNode<EndianSlice<RunTimeEndian>>,
    typedef_name: Option<String>,
    dwarf: &gimli::Dwarf<EndianSlice<RunTimeEndian>>
) -> Result<TypeInfo, String> {
    let entry = entries_tree.entry();
    match entry.tag() {
        gimli::constants::DW_TAG_base_type => {
            let byte_size = get_byte_size_attribute(entry).unwrap_or(1u64);
            let encoding = get_encoding_attribute(entry).unwrap_or(gimli::constants::DW_ATE_unsigned);
            Ok(match encoding {
                gimli::constants::DW_ATE_address => {
                    let (unit, _) = &unit_list[current_unit];
                    TypeInfo::Pointer(unit.encoding().address_size as u64)
                }
                gimli::constants::DW_ATE_float => {
                    if byte_size == 8 {
                        TypeInfo::Double
                    } else {
                        TypeInfo::Float
                    }
                }
                gimli::constants::DW_ATE_signed |
                gimli::constants::DW_ATE_signed_char => {
                    match byte_size {
                        1 => TypeInfo::Sint8,
                        2 => TypeInfo::Sint16,
                        4 => TypeInfo::Sint32,
                        8 => TypeInfo::Sint64,
                        val => {
                            return Err(format!("error loading data type: signed int of size {}", val));
                        }
                    }
                }
                gimli::constants::DW_ATE_boolean |
                gimli::constants::DW_ATE_unsigned |
                gimli::constants::DW_ATE_unsigned_char => {
                    match byte_size {
                        1 => TypeInfo::Uint8,
                        2 => TypeInfo::Uint16,
                        4 => TypeInfo::Uint32,
                        8 => TypeInfo::Uint64,
                        val => {
                            return Err(format!("error loading data type: unsigned int of size {}", val));
                        }
                    }
                }
                _other => {
                    TypeInfo::Other(byte_size)
                }
            })
        }
        gimli::constants::DW_TAG_pointer_type => {
            let (unit, _) = &unit_list[current_unit];
            Ok(TypeInfo::Pointer(unit.encoding().address_size as u64))
        }
        gimli::constants::DW_TAG_array_type => {
            let size = get_byte_size_attribute(entry)
                .ok_or_else(|| "error decoding array info: missing size attribute".to_string())?;
            let (new_cur_unit, mut new_entries_tree) =
                get_entries_tree_from_attribute(entry, unit_list, current_unit)?;
            let arraytype = get_type(unit_list,
                new_cur_unit,
                new_entries_tree.root().unwrap(),
                None,
                dwarf)?;

            let mut dim = Vec::<u64>::new();

            // If the stride of the array is different from the size of each element, then the stride must be given as an attribute
            let stride = if let Some(stride) = get_byte_stride_attribute(entry) {
                stride
            } else {
                // this is the usual case
                arraytype.get_size()
            };

            let default_ubound = (size / stride) - 1; // subtract 1, because ubound is the last element, not the size

            // the child entries of the DW_TAG_array_type entry give the array dimensions
            let mut iter = entries_tree.children();
            while let Ok(Some(child_node)) = iter.next() {
                let child_entry = child_node.entry();
                if child_entry.tag() ==  gimli::constants::DW_TAG_subrange_type {
                    let ubound = get_upper_bound_attribute(child_entry).unwrap_or(default_ubound);
                    dim.push(ubound + 1);
                }
            }
            Ok(TypeInfo::Array { dim, arraytype: Box::new(arraytype), size, stride })
        }
        gimli::constants::DW_TAG_enumeration_type => {
            let size = get_byte_size_attribute(entry)
                .ok_or_else(|| "missing enum byte size attribute".to_string())?;
            let mut enumerators = Vec::new();
            let typename = if let Some(name) = typedef_name {
                name
            } else {
                if let Ok(name_from_attr) = get_name_attribute(entry, dwarf) {
                    name_from_attr
                } else {
                    let (unit, _) = &unit_list[current_unit];
                    format!("anonymous_enum_{}", entry.offset().to_debug_info_offset(unit).unwrap().0)
                }
            };
            let mut iter = entries_tree.children();
            // get all the enumerators
            while let Ok(Some(child_node)) = iter.next() {
                let child_entry = child_node.entry();
                if child_entry.tag() ==  gimli::constants::DW_TAG_enumerator {
                    let name = get_name_attribute(child_entry, dwarf)
                        .map_err(|_| "missing enum item name".to_string() )?;
                    let value = get_const_value_attribute(child_entry)
                        .ok_or_else(|| "missing enum item value".to_string())?;
                    enumerators.push((name, value));
                }
            }
            Ok(TypeInfo::Enum { typename, size, enumerators })
        }
        gimli::constants::DW_TAG_structure_type => {
            let size = get_byte_size_attribute(entry)
                .ok_or_else(|| "missing struct byte size attribute".to_string())?;
            let members = get_struct_or_union_members(entries_tree, unit_list, current_unit, dwarf)?;
            Ok(TypeInfo::Struct {size, members})
        }
        gimli::constants::DW_TAG_class_type => {
            let size = get_byte_size_attribute(entry)
                .ok_or_else(|| "missing class byte size attribute".to_string())?;
            let members = get_struct_or_union_members(entries_tree, unit_list, current_unit, dwarf)?;
            Ok(TypeInfo::Class {size, members})
        }
        gimli::constants::DW_TAG_union_type => {
            let size = get_byte_size_attribute(entry)
                .ok_or_else(|| "missing union byte size attribute".to_string())?;
            let members = get_struct_or_union_members(entries_tree, unit_list, current_unit, dwarf)?;
            Ok(TypeInfo::Union {size, members})
        }
        gimli::constants::DW_TAG_typedef => {
            let name = get_name_attribute(entry, dwarf)?;
            let (new_cur_unit, mut new_entries_tree) = get_entries_tree_from_attribute(entry, unit_list, current_unit)?;
            get_type(
                unit_list,
                new_cur_unit,
                new_entries_tree.root().unwrap(),
                Some(name),
                dwarf
            )
        }
        gimli::constants::DW_TAG_const_type |
        gimli::constants::DW_TAG_volatile_type => {
            let (new_cur_unit, mut new_entries_tree) = get_entries_tree_from_attribute(entry, unit_list, current_unit)?;
            get_type(
                unit_list,
                new_cur_unit,
                new_entries_tree.root().unwrap(),
                typedef_name,
                dwarf
            )
        }
        other_tag => {
            Err(format!("unexpected DWARF tag {} in type definition", other_tag))
        }
    }
}


// get all the members of a struct or union or class
fn get_struct_or_union_members(
    entries_tree: EntriesTreeNode<EndianSlice<RunTimeEndian>>,
    unit_list: &UnitList,
    current_unit: usize,
    dwarf: &gimli::Dwarf<EndianSlice<RunTimeEndian>>
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
                dwarf
            )?;

            // wrap bitfield members in a TypeInfo::Bitfield to store bit_size and bit_offset
            if let Some(bit_size) = bitsize {
                if let Some(bit_offset) = bitoffset {
                    membertype = TypeInfo::Bitfield {
                        basetype: Box::new(membertype),
                        bit_size: bit_size as u16,
                        bit_offset: bit_offset as u16
                    };
                }
            }
            members.insert(name, (membertype, offset));
        }
    }
    Ok(members)
}
