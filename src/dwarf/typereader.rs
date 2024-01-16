use super::{attributes::*, DebugDataReader};
use super::{TypeInfo, VarInfo};
use gimli::{EndianSlice, EntriesTreeNode, RunTimeEndian, UnitOffset};
use indexmap::IndexMap;
use object::Endianness;
use std::collections::HashMap;

impl<'elffile> DebugDataReader<'elffile> {
    // load all the types referenced by variables in given HashMap
    pub(crate) fn load_types(
        &mut self,
        variables: &IndexMap<String, VarInfo>,
    ) -> HashMap<usize, TypeInfo> {
        let mut types = HashMap::<usize, TypeInfo>::new();
        // for each variable
        for (name, VarInfo { typeref, .. }) in variables {
            // check if the type was already loaded
            if types.get(typeref).is_none() {
                if let Some(unit_idx) = self.units.get_unit(*typeref) {
                    // create an entries_tree iterator that makes it possible to read the DIEs of this type
                    let (unit, _) = &self.units[unit_idx];
                    let dbginfo_offset = gimli::DebugInfoOffset(*typeref);
                    let unit_offset = dbginfo_offset.to_unit_offset(unit).unwrap();

                    // load one type and add it to the collection (always succeeds for correctly structured DWARF debug info)
                    match self.get_type(unit_idx, unit_offset, None) {
                        Ok(vartype) => {
                            types.insert(*typeref, vartype);
                        }
                        Err(errmsg) => {
                            if self.verbose {
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
        &self,
        current_unit: usize,
        offset: UnitOffset,
        typedef_name: Option<String>,
    ) -> Result<TypeInfo, String> {
        let (unit, abbrev) = &self.units[current_unit];
        let mut entries_tree = unit
            .entries_tree(abbrev, Some(offset))
            .map_err(|err| err.to_string())?;
        let entries_tree_node = entries_tree.root().map_err(|err| err.to_string())?;
        let entry = entries_tree_node.entry();
        match entry.tag() {
            gimli::constants::DW_TAG_base_type => {
                Ok(get_base_type(entry, &self.units[current_unit].0))
            }
            gimli::constants::DW_TAG_pointer_type => {
                let (unit, _) = &self.units[current_unit];
                Ok(TypeInfo::Pointer(u64::from(unit.encoding().address_size)))
            }
            gimli::constants::DW_TAG_array_type => self.get_array_type(entry, current_unit, offset),
            gimli::constants::DW_TAG_enumeration_type => {
                self.get_enumeration_type(current_unit, offset, typedef_name)
            }
            gimli::constants::DW_TAG_structure_type => {
                let size = get_byte_size_attribute(entry)
                    .ok_or_else(|| "missing struct byte size attribute".to_string())?;
                let members = self.get_struct_or_union_members(entries_tree_node, current_unit)?;
                Ok(TypeInfo::Struct { size, members })
            }
            gimli::constants::DW_TAG_class_type => self.get_class_type(current_unit, offset),
            gimli::constants::DW_TAG_union_type => {
                let size = get_byte_size_attribute(entry)
                    .ok_or_else(|| "missing union byte size attribute".to_string())?;
                let members = self.get_struct_or_union_members(entries_tree_node, current_unit)?;
                Ok(TypeInfo::Union { size, members })
            }
            gimli::constants::DW_TAG_typedef => {
                let name = get_name_attribute(entry, &self.dwarf, unit)?;
                let (new_cur_unit, new_unit_offset) =
                    get_type_attribute(entry, &self.units, current_unit)?;
                self.get_type(new_cur_unit, new_unit_offset, Some(name))
            }
            gimli::constants::DW_TAG_const_type | gimli::constants::DW_TAG_volatile_type => {
                let (new_cur_unit, new_unit_offset) =
                    get_type_attribute(entry, &self.units, current_unit)?;
                self.get_type(new_cur_unit, new_unit_offset, typedef_name)
            }
            other_tag => Err(format!(
                "unexpected DWARF tag {other_tag} in type definition"
            )),
        }
    }

    fn get_array_type(
        &self,
        entry: &gimli::DebuggingInformationEntry<'_, '_, EndianSlice<'_, RunTimeEndian>, usize>,
        current_unit: usize,
        offset: UnitOffset,
    ) -> Result<TypeInfo, String> {
        let (unit, abbrev) = &self.units[current_unit];
        let mut entries_tree = unit
            .entries_tree(abbrev, Some(offset))
            .map_err(|err| err.to_string())?;
        let entries_tree_node = entries_tree.root().map_err(|err| err.to_string())?;

        let maybe_size = get_byte_size_attribute(entry);
        let (new_cur_unit, new_unit_offset) = get_type_attribute(entry, &self.units, current_unit)?;
        let arraytype = self.get_type(new_cur_unit, new_unit_offset, None)?;
        let mut dim = Vec::<u64>::new();
        let stride = if let Some(stride) = get_byte_stride_attribute(entry) {
            stride
        } else {
            // this is the usual case
            arraytype.get_size()
        };
        let default_ubound = maybe_size.map(|s: u64| s / stride - 1);
        let mut iter = entries_tree_node.children();
        while let Ok(Some(child_node)) = iter.next() {
            let child_entry = child_node.entry();
            if child_entry.tag() == gimli::constants::DW_TAG_subrange_type {
                let ubound = get_upper_bound_attribute(child_entry)
                    .or(default_ubound)
                    .unwrap_or(0);
                dim.push(ubound + 1);
            }
        }
        let size = maybe_size.unwrap_or_else(|| dim.iter().fold(stride, |acc, num| acc * num));
        Ok(TypeInfo::Array {
            dim,
            arraytype: Box::new(arraytype),
            size,
            stride,
        })
    }

    fn get_enumeration_type(
        &self,
        current_unit: usize,
        offset: UnitOffset,
        typedef_name: Option<String>,
    ) -> Result<TypeInfo, String> {
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
        let dioffset = entry.offset().to_debug_info_offset(unit).unwrap().0;
        let typename = if let Some(name) = typedef_name {
            // enum referenced by a typedef: the compiler generated debuginfo that had e.g.
            //   variable -> typedef -> (named or anonymous) enum
            name
        } else if let Ok(name_from_attr) = get_name_attribute(entry, &self.dwarf, unit) {
            // named enum that is not directly referenced by a typedef. It might still have been typedef'd in the original code.
            name_from_attr
        } else if let Some(name) = self.typedefs.get(&dioffset) {
            // anonymous enum, with a typedef name recovered from the global list
            // the compiler had the typedef info at compile time, but didn't refer to it in the debug info
            name.to_owned()
        } else {
            // a truly anonymous enum. This can happen if someone writes C code that looks like this:
            // enum { ... } varname;
            format!("anonymous_enum_{dioffset}")
        };
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
        Ok(TypeInfo::Enum {
            typename,
            size,
            enumerators,
        })
    }

    fn get_class_type(&self, current_unit: usize, offset: UnitOffset) -> Result<TypeInfo, String> {
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
            .get_class_inheritance(entries_tree_node2, current_unit)
            .unwrap_or(IndexMap::<String, (TypeInfo, u64)>::new());
        let mut members = self.get_struct_or_union_members(entries_tree_node, current_unit)?;
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

    // get all the members of a struct or union or class
    fn get_struct_or_union_members(
        &self,
        entries_tree: EntriesTreeNode<EndianSlice<RunTimeEndian>>,
        current_unit: usize,
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
                let (new_cur_unit, new_unit_offset) =
                    get_type_attribute(child_entry, &self.units, current_unit)?;
                if let Ok(mut membertype) = self.get_type(new_cur_unit, new_unit_offset, None) {
                    // wrap bitfield members in a TypeInfo::Bitfield to store bit_size and bit_offset
                    if let Some(bit_size) = get_bit_size_attribute(child_entry) {
                        if let Some(bit_offset) = get_bit_offset_attribute(child_entry) {
                            // Dwarf 2 / 3
                            if self.endian == Endianness::Big {
                                membertype = TypeInfo::Bitfield {
                                    basetype: Box::new(membertype),
                                    bit_size: bit_size as u16,
                                    bit_offset: bit_offset as u16,
                                };
                            } else {
                                // Endianness::Little
                                let type_size = membertype.get_size();
                                let type_size_bits = type_size * 8;
                                let bit_offset_le = type_size_bits - bit_offset - bit_size;
                                membertype = TypeInfo::Bitfield {
                                    basetype: Box::new(membertype),
                                    bit_size: bit_size as u16,
                                    bit_offset: bit_offset_le as u16,
                                };
                            }
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
                                data_bit_offset = data_bit_offset % type_size_bits;
                            }
                            // these values should be independent of Endianness
                            membertype = TypeInfo::Bitfield {
                                basetype: Box::new(membertype),
                                bit_size: bit_size as u16,
                                bit_offset: data_bit_offset as u16,
                            };
                        }
                    }
                    if let Ok(name) = opt_name {
                        members.insert(name, (membertype, offset));
                    } else {
                        // no name: the member is an anon struct / union
                        // In this case, the contained members are transferred
                        match membertype {
                            TypeInfo::Class {
                                members: anon_members,
                                ..
                            }
                            | TypeInfo::Struct {
                                members: anon_members,
                                ..
                            }
                            | TypeInfo::Union {
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
    ) -> Result<IndexMap<String, (TypeInfo, u64)>, String> {
        let (unit, _) = &self.units[current_unit];
        let mut inheritance = IndexMap::<String, (TypeInfo, u64)>::new();
        let mut iter = entries_tree.children();
        while let Ok(Some(child_node)) = iter.next() {
            let child_entry = child_node.entry();
            if child_entry.tag() == gimli::constants::DW_TAG_inheritance {
                let offset = get_data_member_location_attribute(
                    self,
                    child_entry,
                    unit.encoding(),
                    current_unit,
                )
                .ok_or_else(|| "missing byte offset for inherited class".to_string())?;
                let (new_cur_unit, new_unit_offset) =
                    get_type_attribute(child_entry, &self.units, current_unit)?;

                let (unit, abbrev) = &self.units[new_cur_unit];
                let mut baseclass_tree = unit
                    .entries_tree(abbrev, Some(new_unit_offset))
                    .map_err(|err| err.to_string())?;
                let baseclass_tree_node = baseclass_tree.root().map_err(|err| err.to_string())?;
                let baseclass_entry = baseclass_tree_node.entry();
                let baseclass_name = get_name_attribute(baseclass_entry, &self.dwarf, unit)?;

                let baseclass_type = self.get_type(new_cur_unit, new_unit_offset, None)?;

                inheritance.insert(baseclass_name, (baseclass_type, offset));
            }
        }
        Ok(inheritance)
    }
}

fn get_base_type(
    entry: &gimli::DebuggingInformationEntry<EndianSlice<RunTimeEndian>, usize>,
    unit: &gimli::UnitHeader<EndianSlice<RunTimeEndian>>,
) -> TypeInfo {
    let byte_size = get_byte_size_attribute(entry).unwrap_or(1u64);
    let encoding = get_encoding_attribute(entry).unwrap_or(gimli::constants::DW_ATE_unsigned);
    match encoding {
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
            _ => TypeInfo::Other(byte_size),
        },
        gimli::constants::DW_ATE_boolean
        | gimli::constants::DW_ATE_unsigned
        | gimli::constants::DW_ATE_unsigned_char => match byte_size {
            1 => TypeInfo::Uint8,
            2 => TypeInfo::Uint16,
            4 => TypeInfo::Uint32,
            8 => TypeInfo::Uint64,
            _ => TypeInfo::Other(byte_size),
        },
        _other => TypeInfo::Other(byte_size),
    }
}
