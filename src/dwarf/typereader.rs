use super::{attributes::*, DebugDataReader};
use super::{DwarfDataType, TypeInfo, VarInfo};
use gimli::{DebugInfoOffset, DwTag, EndianSlice, EntriesTreeNode, RunTimeEndian, UnitOffset};
use indexmap::IndexMap;
use object::Endianness;
use std::collections::HashMap;

#[derive(Debug)]
struct WipItemInfo {
    offset: usize,
    name: Option<String>,
    tag: DwTag,
}

struct TypeReaderData {
    types: HashMap<usize, TypeInfo>,
    typenames: HashMap<String, Vec<usize>>,
    wip_items: Vec<WipItemInfo>,
}

impl<'elffile> DebugDataReader<'elffile> {
    // load all the types referenced by variables in given HashMap
    pub(crate) fn load_types(
        &mut self,
        variables: &IndexMap<String, Vec<VarInfo>>,
    ) -> (HashMap<usize, TypeInfo>, HashMap<String, Vec<usize>>) {
        let mut typereader_data = TypeReaderData {
            types: HashMap::<usize, TypeInfo>::new(),
            typenames: HashMap::<String, Vec<usize>>::new(),
            wip_items: Vec::new(),
        };
        // for each variable
        for (name, var_list) in variables {
            for VarInfo { typeref, .. } in var_list {
                // check if the type was already loaded
                if !typereader_data.types.contains_key(typeref) {
                    if let Some(unit_idx) = self.units.get_unit(*typeref) {
                        // create an entries_tree iterator that makes it possible to read the DIEs of this type
                        let dbginfo_offset = gimli::DebugInfoOffset(*typeref);

                        // load one type and add it to the collection (always succeeds for correctly structured DWARF debug info)
                        let result = self.get_type(unit_idx, dbginfo_offset, &mut typereader_data);
                        if let Err(errmsg) = result {
                            if self.verbose {
                                println!("Error loading type info for variable {name}: {errmsg}");
                            }
                        }
                        typereader_data.wip_items.clear();
                    }
                }
            }
        }

        (typereader_data.types, typereader_data.typenames)
    }

    fn get_type(
        &self,
        current_unit: usize,
        dbginfo_offset: DebugInfoOffset,
        typereader_data: &mut TypeReaderData,
    ) -> Result<TypeInfo, String> {
        let wip_items_orig_len = typereader_data.wip_items.len();
        match self.get_type_wrapped(current_unit, dbginfo_offset, typereader_data) {
            Ok(typeinfo) => Ok(typeinfo),
            Err(errmsg) => {
                // try to print a readable error message
                println!("Failed to read type: {errmsg}");
                for (idx, wip) in typereader_data.wip_items.iter().enumerate() {
                    print!("  {:indent$}{}", "", wip.tag, indent = idx * 2);
                    if let Some(name) = &wip.name {
                        print!(" {name}");
                    }
                    println!(" @0x{:X}", wip.offset);
                }

                // create a dummy typeinfo using DwarfDataType::Other, rather than propagate the error
                // this allows the caller to continue, which is more useful
                // for example, this could result in a struct where one member is unusable, but any others could still be OK
                typereader_data.wip_items.truncate(wip_items_orig_len);
                let replacement_type = TypeInfo {
                    datatype: DwarfDataType::Other(0),
                    name: typereader_data
                        .wip_items
                        .last()
                        .and_then(|wip| wip.name.clone()),
                    unit_idx: current_unit,
                    dbginfo_offset: dbginfo_offset.0,
                };
                typereader_data
                    .types
                    .insert(dbginfo_offset.0, replacement_type.clone());
                Ok(replacement_type)
            }
        }
    }

    // get one type from the debug info
    fn get_type_wrapped(
        &self,
        current_unit: usize,
        dbginfo_offset: DebugInfoOffset,
        typereader_data: &mut TypeReaderData,
    ) -> Result<TypeInfo, String> {
        if let Some(t) = typereader_data.types.get(&dbginfo_offset.0) {
            return Ok(t.clone());
        }

        let (unit, abbrev) = &self.units[current_unit];
        let offset = dbginfo_offset.to_unit_offset(unit).unwrap();
        let mut entries_tree = unit
            .entries_tree(abbrev, Some(offset))
            .map_err(|err| err.to_string())?;
        let entries_tree_node = entries_tree.root().map_err(|err| err.to_string())?;
        let entry = entries_tree_node.entry();
        let typename = get_name_attribute(entry, &self.dwarf, unit).ok();
        typereader_data.wip_items.push(WipItemInfo::new(
            dbginfo_offset.0,
            typename.clone(),
            entry.tag(),
        ));

        let (datatype, inner_name) = match entry.tag() {
            gimli::constants::DW_TAG_base_type => {
                let (datatype, name) = get_base_type(entry, &self.units[current_unit].0);
                (datatype, Some(name))
            }
            gimli::constants::DW_TAG_pointer_type => {
                let (unit, _) = &self.units[current_unit];
                if let Ok((new_cur_unit, ptype_offset)) =
                    get_type_attribute(entry, &self.units, current_unit)
                {
                    if let Some(idx) = typereader_data
                        .wip_items
                        .iter()
                        .position(|item| item.offset == ptype_offset.0)
                    {
                        // this is a linked list or similar self-referential data structure, and one of the callers
                        // of this function is already working to get this type
                        // Trying to recursively decode this type would result in an infinite loop
                        //
                        // Unfortunately the name in wip_items could be None: pointer names propagate backward from items
                        // e.g pointer -> const -> volatile -> typedef (name comes from here!) -> any
                        let name = typereader_data.get_pointer_name(idx);
                        (
                            DwarfDataType::Pointer(
                                u64::from(unit.encoding().address_size),
                                ptype_offset,
                            ),
                            name.clone(),
                        )
                    } else {
                        let pt_type = self.get_type(new_cur_unit, ptype_offset, typereader_data)?;
                        (
                            DwarfDataType::Pointer(
                                u64::from(unit.encoding().address_size),
                                ptype_offset,
                            ),
                            pt_type.name,
                        )
                    }
                } else {
                    // void*
                    (
                        DwarfDataType::Pointer(
                            u64::from(unit.encoding().address_size),
                            DebugInfoOffset(0),
                        ),
                        Some("void".to_string()),
                    )
                }
                //DwarfDataType::Pointer(u64::from(unit.encoding().address_size), dest_type)
            }
            gimli::constants::DW_TAG_array_type => {
                self.get_array_type(entry, current_unit, offset, typereader_data)?
            }
            gimli::constants::DW_TAG_enumeration_type => {
                (self.get_enumeration_type(current_unit, offset)?, None)
            }
            gimli::constants::DW_TAG_structure_type => {
                let size = get_byte_size_attribute(entry)
                    .ok_or_else(|| "missing struct byte size attribute".to_string())?;
                let members = self.get_struct_or_union_members(
                    entries_tree_node,
                    current_unit,
                    typereader_data,
                )?;
                (DwarfDataType::Struct { size, members }, None)
            }
            gimli::constants::DW_TAG_class_type => (
                self.get_class_type(current_unit, offset, typereader_data)?,
                None,
            ),
            gimli::constants::DW_TAG_union_type => {
                let size = get_byte_size_attribute(entry)
                    .ok_or_else(|| "missing union byte size attribute".to_string())?;
                let members = self.get_struct_or_union_members(
                    entries_tree_node,
                    current_unit,
                    typereader_data,
                )?;
                (DwarfDataType::Union { size, members }, None)
            }
            gimli::constants::DW_TAG_typedef => {
                let (new_cur_unit, dbginfo_offset) =
                    get_type_attribute(entry, &self.units, current_unit)?;
                let reftype = self.get_type(new_cur_unit, dbginfo_offset, typereader_data)?;
                (reftype.datatype, None)
            }
            gimli::constants::DW_TAG_const_type | gimli::constants::DW_TAG_volatile_type => {
                if let Ok((new_cur_unit, dbginfo_offset)) =
                    get_type_attribute(entry, &self.units, current_unit)
                {
                    let typeinfo = self.get_type(new_cur_unit, dbginfo_offset, typereader_data)?;
                    (typeinfo.datatype, typeinfo.name)
                } else {
                    // const void*
                    (
                        DwarfDataType::Other(u64::from(unit.encoding().address_size)),
                        None,
                    )
                }
            }
            gimli::constants::DW_TAG_subroutine_type => {
                // function pointer
                (
                    DwarfDataType::FuncPtr(u64::from(unit.encoding().address_size)),
                    Some("p_function".to_string()),
                )
            }
            gimli::constants::DW_TAG_unspecified_type => {
                // ?
                (
                    DwarfDataType::Other(get_byte_size_attribute(entry).unwrap_or(0)),
                    None,
                )
            }
            other_tag => {
                return Err(format!(
                    "unexpected DWARF tag {other_tag} in type definition"
                ))
            }
        };

        // use the inner name as a display name for the type if the type has no name of its own
        let display_name = typename.clone().or(inner_name);
        let typeinfo = TypeInfo {
            datatype,
            name: display_name,
            unit_idx: current_unit,
            dbginfo_offset: dbginfo_offset.0,
        };

        if let Some(name) = typename {
            // DWARF2 debugdata contains massive amounts of duplicated information. A datatype defined in a
            // header appears in the data of each compilation unit (=file) that includes that header.
            // This causes one name to potentially refer to many repetitions of the type.
            if let Some(tnvec) = typereader_data.typenames.get_mut(&name) {
                tnvec.push(dbginfo_offset.0);
            } else {
                typereader_data
                    .typenames
                    .insert(name, vec![dbginfo_offset.0]);
            }
        }
        typereader_data.wip_items.pop();

        // store the type for access-by-offset
        typereader_data
            .types
            .insert(dbginfo_offset.0, typeinfo.clone());

        Ok(typeinfo)
    }

    fn get_array_type(
        &self,
        entry: &gimli::DebuggingInformationEntry<'_, '_, EndianSlice<'_, RunTimeEndian>, usize>,
        current_unit: usize,
        offset: UnitOffset,
        typereader_data: &mut TypeReaderData,
    ) -> Result<(DwarfDataType, Option<String>), String> {
        let (unit, abbrev) = &self.units[current_unit];
        let mut entries_tree = unit
            .entries_tree(abbrev, Some(offset))
            .map_err(|err| err.to_string())?;
        let entries_tree_node = entries_tree.root().map_err(|err| err.to_string())?;

        let maybe_size = get_byte_size_attribute(entry);
        let (new_cur_unit, arraytype_offset) =
            get_type_attribute(entry, &self.units, current_unit)?;
        let arraytype = self.get_type(new_cur_unit, arraytype_offset, typereader_data)?;
        let arraytype_name = arraytype.name.clone();
        let stride = if let Some(stride) = get_byte_stride_attribute(entry) {
            stride
        } else {
            // this is the usual case
            arraytype.get_size()
        };

        // get the array dimensions
        let mut dim = Vec::<u64>::new();
        let mut iter = entries_tree_node.children();
        while let Ok(Some(child_node)) = iter.next() {
            let child_entry = child_node.entry();
            if child_entry.tag() == gimli::constants::DW_TAG_subrange_type {
                let count = if let Some(ubound) = get_upper_bound_attribute(child_entry) {
                    let lbound = get_lower_bound_attribute(child_entry).unwrap_or(0);
                    // compilers may use the bit pattern FFF.. to mean that the array size is unknown
                    // this can happen when a pointer to an array is declared
                    if ubound != u64::from(u32::MAX) && ubound != u64::MAX {
                        ubound - lbound + 1
                    } else {
                        0
                    }
                } else {
                    // clang generates DW_AT_count instead of DW_AT_ubound
                    get_count_attribute(child_entry).unwrap_or_default()
                };
                dim.push(count);
            } else if child_entry.tag() == gimli::constants::DW_TAG_enumeration_type {
                // the DWARF spec allows an array dimension to be given using an enumeration type
                // presumably this could be created by languages other than C / C++
                let mut enum_count = 0;
                let mut enum_iter = child_node.children();
                while let Ok(Some(enum_node)) = enum_iter.next() {
                    if enum_node.entry().tag() == gimli::constants::DW_TAG_enumerator {
                        enum_count += 1;
                    }
                }
                dim.push(enum_count);
            }
        }

        // try to fix the dimension of the array, if the DW_TAG_subrange_type didn't contain enough info
        if dim.len() == 1 && dim[0] == 0 && stride != 0 {
            if let Some(count) = maybe_size.map(|s: u64| s / stride) {
                dim[0] = count;
            }
        }
        let size = maybe_size.unwrap_or_else(|| dim.iter().fold(stride, |acc, num| acc * num));
        Ok((
            DwarfDataType::Array {
                dim,
                arraytype: Box::new(arraytype),
                size,
                stride,
            },
            arraytype_name,
        ))
    }

    fn get_enumeration_type(
        &self,
        current_unit: usize,
        offset: UnitOffset,
    ) -> Result<DwarfDataType, String> {
        let (unit, abbrev) = &self.units[current_unit];
        let mut entries_tree = unit
            .entries_tree(abbrev, Some(offset))
            .map_err(|err| err.to_string())?;
        let entries_tree_node = entries_tree.root().map_err(|err| err.to_string())?;
        let entry = entries_tree_node.entry();

        let size = get_byte_size_attribute(entry)
            .ok_or_else(|| "missing enum byte size attribute".to_string())?;
        let mut enumerators = Vec::new();
        let (unit, _) = &self.units[current_unit];

        let mut iter = entries_tree_node.children();
        while let Ok(Some(child_node)) = iter.next() {
            let child_entry = child_node.entry();
            if child_entry.tag() == gimli::constants::DW_TAG_enumerator {
                let name = get_name_attribute(child_entry, &self.dwarf, unit)
                    .map_err(|_| "missing enum item name".to_string())?;
                let value = get_const_value_attribute(child_entry)
                    .ok_or_else(|| "missing enum item value".to_string())?;
                enumerators.push((name, value));
            }
        }
        Ok(DwarfDataType::Enum { size, enumerators })
    }

    fn get_class_type(
        &self,
        current_unit: usize,
        offset: UnitOffset,
        typereader_data: &mut TypeReaderData,
    ) -> Result<DwarfDataType, String> {
        let (unit, abbrev) = &self.units[current_unit];
        let mut entries_tree = unit
            .entries_tree(abbrev, Some(offset))
            .map_err(|err| err.to_string())?;
        let entries_tree_node = entries_tree.root().map_err(|err| err.to_string())?;
        let entry = entries_tree_node.entry();

        let size = get_byte_size_attribute(entry)
            .ok_or_else(|| "missing class byte size attribute".to_string())?;
        let (unit, abbrev) = &self.units[current_unit];
        let mut entries_tree2 = unit
            .entries_tree(abbrev, Some(entries_tree_node.entry().offset()))
            .unwrap();
        let entries_tree_node2 = entries_tree2.root().unwrap();
        let inheritance = self
            .get_class_inheritance(entries_tree_node2, current_unit, typereader_data)
            .unwrap_or_default();
        let mut members =
            self.get_struct_or_union_members(entries_tree_node, current_unit, typereader_data)?;
        for (baseclass_type, baseclass_offset) in inheritance.values() {
            if let DwarfDataType::Class {
                members: baseclass_members,
                ..
            } = &baseclass_type.datatype
            {
                for (name, (m_type, m_offset)) in baseclass_members {
                    members.insert(
                        name.to_owned(),
                        (m_type.clone(), m_offset + baseclass_offset),
                    );
                }
            }
        }
        Ok(DwarfDataType::Class {
            size,
            inheritance,
            members,
        })
    }

    // get all the members of a struct or union or class
    fn get_struct_or_union_members(
        &self,
        entries_tree: EntriesTreeNode<EndianSlice<RunTimeEndian>>,
        current_unit: usize,
        typereader_data: &mut TypeReaderData,
    ) -> Result<IndexMap<String, (TypeInfo, u64)>, String> {
        let (unit, _) = &self.units[current_unit];
        let mut members = IndexMap::<String, (TypeInfo, u64)>::new();
        let mut iter = entries_tree.children();
        while let Ok(Some(child_node)) = iter.next() {
            let child_entry = child_node.entry();
            if child_entry.tag() == gimli::constants::DW_TAG_member {
                // the name can be missing if this struct/union contains an anonymous struct/union
                let opt_name = get_name_attribute(child_entry, &self.dwarf, unit)
                    .map_err(|_| "missing struct/union member name".to_string());

                let mut offset = get_data_member_location_attribute(
                    self,
                    child_entry,
                    unit.encoding(),
                    current_unit,
                )
                .unwrap_or(0);
                let (new_cur_unit, new_dbginfo_offset) =
                    get_type_attribute(child_entry, &self.units, current_unit)?;
                if let Ok(mut membertype) =
                    self.get_type(new_cur_unit, new_dbginfo_offset, typereader_data)
                {
                    // wrap bitfield members in a TypeInfo::Bitfield to store bit_size and bit_offset
                    if let Some(bit_size) = get_bit_size_attribute(child_entry) {
                        let dbginfo_offset =
                            child_entry.offset().to_debug_info_offset(unit).unwrap().0;
                        if let Some(bit_offset) = get_bit_offset_attribute(child_entry) {
                            // Dwarf 2 / 3
                            let type_size = membertype.get_size();
                            let type_size_bits = type_size * 8;
                            let bit_offset_le = type_size_bits - bit_offset - bit_size;
                            membertype = TypeInfo {
                                name: membertype.name.clone(),
                                unit_idx: membertype.unit_idx,
                                dbginfo_offset,
                                datatype: DwarfDataType::Bitfield {
                                    basetype: Box::new(membertype),
                                    bit_size: bit_size as u16,
                                    bit_offset: bit_offset_le as u16,
                                },
                            };
                        } else if let Some(mut data_bit_offset) =
                            get_data_bit_offset_attribute(child_entry)
                        {
                            // Dwarf 4 / 5:
                            // The data bit offset attribute is the offset in bits from the beginning of the containing storage to the beginning of the value
                            // this means the bitfield member may have type uint32, but have an offset > 32 bits
                            let type_size = membertype.get_size();
                            let type_size_bits = type_size * 8;
                            if data_bit_offset >= type_size_bits {
                                // Dwarf 4 / 5: re-calculate offset
                                offset += (data_bit_offset / type_size_bits) * type_size;
                                data_bit_offset %= type_size_bits;
                            }
                            if self.endian == Endianness::Big {
                                // reverse the mask for big endian. Example
                                // In: type_size 32, offset: 5, size 4 -> 0000_0000_0000_0000_0000_0001_1110_0000
                                // Out: offset = 32 - 5 - 4 = 23       -> 0000_0111_1000_0000_0000_0000_0000_0000
                                data_bit_offset = type_size_bits - data_bit_offset - bit_size;
                            }
                            // these values should be independent of Endianness
                            membertype = TypeInfo {
                                name: membertype.name.clone(),
                                unit_idx: membertype.unit_idx,
                                dbginfo_offset,
                                datatype: DwarfDataType::Bitfield {
                                    basetype: Box::new(membertype),
                                    bit_size: bit_size as u16,
                                    bit_offset: data_bit_offset as u16,
                                },
                            };
                        }
                    }
                    if let Ok(name) = opt_name {
                        // in bitfields it's actually possible for the name to be empty!
                        // "int :31;" is valid C!
                        if !name.is_empty() {
                            // refer to the loaded type instead of duplicating it in the members
                            if matches!(membertype.datatype, DwarfDataType::Struct { .. })
                                || matches!(membertype.datatype, DwarfDataType::Union { .. })
                                || matches!(membertype.datatype, DwarfDataType::Class { .. })
                            {
                                membertype.datatype = DwarfDataType::TypeRef(
                                    new_dbginfo_offset.0,
                                    membertype.get_size(),
                                );
                            }
                            members.insert(name, (membertype, offset));
                        }
                    } else {
                        // no name: the member is an anon struct / union
                        // In this case, the contained members are transferred
                        match membertype.datatype {
                            DwarfDataType::Class {
                                members: anon_members,
                                ..
                            }
                            | DwarfDataType::Struct {
                                members: anon_members,
                                ..
                            }
                            | DwarfDataType::Union {
                                members: anon_members,
                                ..
                            } => {
                                for (am_name, (am_type, am_offset)) in anon_members {
                                    members.insert(am_name, (am_type, offset + am_offset));
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        Ok(members)
    }

    // get all the members of a struct or union or class
    fn get_class_inheritance(
        &self,
        entries_tree: EntriesTreeNode<EndianSlice<RunTimeEndian>>,
        current_unit: usize,
        typereader_data: &mut TypeReaderData,
    ) -> Result<IndexMap<String, (TypeInfo, u64)>, String> {
        let (unit, _) = &self.units[current_unit];
        let mut inheritance = IndexMap::<String, (TypeInfo, u64)>::new();
        let mut iter = entries_tree.children();
        while let Ok(Some(child_node)) = iter.next() {
            let child_entry = child_node.entry();
            if child_entry.tag() == gimli::constants::DW_TAG_inheritance {
                let data_location = get_data_member_location_attribute(
                    self,
                    child_entry,
                    unit.encoding(),
                    current_unit,
                )
                .ok_or_else(|| "missing byte offset for inherited class".to_string())?;
                let (new_cur_unit, new_dbginfo_offset) =
                    get_type_attribute(child_entry, &self.units, current_unit)?;

                let (unit, abbrev) = &self.units[new_cur_unit];
                let new_unit_offset = new_dbginfo_offset.to_unit_offset(unit).unwrap();
                let mut baseclass_tree = unit
                    .entries_tree(abbrev, Some(new_unit_offset))
                    .map_err(|err| err.to_string())?;
                let baseclass_tree_node = baseclass_tree.root().map_err(|err| err.to_string())?;
                let baseclass_entry = baseclass_tree_node.entry();
                let baseclass_name = get_name_attribute(baseclass_entry, &self.dwarf, unit)?;

                let baseclass_type =
                    self.get_type(new_cur_unit, new_dbginfo_offset, typereader_data)?;

                inheritance.insert(baseclass_name, (baseclass_type, data_location));
            }
        }
        Ok(inheritance)
    }
}

fn get_base_type(
    entry: &gimli::DebuggingInformationEntry<EndianSlice<RunTimeEndian>, usize>,
    unit: &gimli::UnitHeader<EndianSlice<RunTimeEndian>>,
) -> (DwarfDataType, String) {
    let byte_size = get_byte_size_attribute(entry).unwrap_or(1u64);
    let encoding = get_encoding_attribute(entry).unwrap_or(gimli::constants::DW_ATE_unsigned);
    match encoding {
        gimli::constants::DW_ATE_address => {
            // if compilers use DW_TAG_base_type with DW_AT_encoding = DW_ATE_address, then it is only used for void pointers
            // in all other cases DW_AT_pointer is used
            (
                DwarfDataType::Pointer(u64::from(unit.encoding().address_size), DebugInfoOffset(0)),
                "unknown".to_string(),
            )
        }
        gimli::constants::DW_ATE_float => {
            if byte_size == 8 {
                (DwarfDataType::Double, "double".to_string())
            } else {
                (DwarfDataType::Float, "float".to_string())
            }
        }
        gimli::constants::DW_ATE_signed | gimli::constants::DW_ATE_signed_char => match byte_size {
            1 => (DwarfDataType::Sint8, "sint8".to_string()),
            2 => (DwarfDataType::Sint16, "sint16".to_string()),
            4 => (DwarfDataType::Sint32, "sint32".to_string()),
            8 => (DwarfDataType::Sint64, "sint64".to_string()),
            _ => (DwarfDataType::Other(byte_size), "double".to_string()),
        },
        gimli::constants::DW_ATE_boolean
        | gimli::constants::DW_ATE_unsigned
        | gimli::constants::DW_ATE_unsigned_char => match byte_size {
            1 => (DwarfDataType::Uint8, "uint8".to_string()),
            2 => (DwarfDataType::Uint16, "uint16".to_string()),
            4 => (DwarfDataType::Uint32, "uint32".to_string()),
            8 => (DwarfDataType::Uint64, "uint64".to_string()),
            _ => (DwarfDataType::Other(byte_size), "other".to_string()),
        },
        _other => (DwarfDataType::Other(byte_size), "other".to_string()),
    }
}

impl WipItemInfo {
    fn new(offset: usize, name: Option<String>, tag: DwTag) -> Self {
        Self { offset, name, tag }
    }
}

impl TypeReaderData {
    // get_pointer_name() is a solution for a really ugly edge case:
    // Data structures can reference themselves using pointers.
    // Since types are normally read recursively, this would case would result in an infinite loop.
    // The fix is to keep track of in-progress types in self.wip_items, and break the recursion if needed.
    // Now pointers have a new problem: they normally get their names from the pointed-to child type, whose info is not available yet
    // Here we try to recover a name from the wip_items stack
    fn get_pointer_name(&self, idx: usize) -> Option<String> {
        let mut nameidx = idx;
        while nameidx < self.wip_items.len() {
            if self.wip_items[nameidx].name.is_some() {
                return self.wip_items[nameidx].name.clone();
            }
            // if the type would propagate its name backward, we're allowed to look further up the stack
            if !(self.wip_items[nameidx].tag == gimli::constants::DW_TAG_const_type
                || self.wip_items[nameidx].tag == gimli::constants::DW_TAG_volatile_type
                || self.wip_items[nameidx].tag == gimli::constants::DW_TAG_pointer_type
                || self.wip_items[nameidx].tag == gimli::constants::DW_TAG_array_type)
            {
                return None;
            }
            nameidx += 1;
        }
        None
    }
}
