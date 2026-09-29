use super::{DbgDataType, TypeInfo, VarInfo};
use super::{DebugDataReader, attributes::*};
use gimli::{DebugInfoOffset, DwTag, EndianSlice, EntriesTreeNode, RunTimeEndian, UnitOffset};
use indexmap::IndexMap;
use object::Endianness;
use std::collections::HashMap;

/// maps the name of a struct member or base class to its type and its offset
/// inside the containing type
type MemberMap = IndexMap<String, (TypeInfo, u64)>;

/// The largest type (in bytes) that can contain a bitfield.
const MAX_BITFIELD_CONTAINER_SIZE: u64 = 8;

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

impl DebugDataReader<'_> {
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
                if !typereader_data.types.contains_key(typeref)
                    && let Some(unit_idx) = self.units.get_unit(*typeref)
                {
                    // create an entries_tree iterator that makes it possible to read the DIEs of this type
                    let dbginfo_offset = gimli::DebugInfoOffset(*typeref);

                    // load one type and add it to the collection (always succeeds for correctly structured DWARF debug info)
                    let result = self.get_type(unit_idx, dbginfo_offset, &mut typereader_data);
                    if let Err(errmsg) = result
                        && self.verbose
                    {
                        println!("Error loading type info for variable {name}: {errmsg}");
                    }
                    typereader_data.wip_items.clear();
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
                    datatype: DbgDataType::Other(0),
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
        let offset = dbginfo_offset.to_unit_offset(unit).ok_or_else(|| {
            format!(
                "invalid type offset 0x{:X} for unit {}",
                dbginfo_offset.0, current_unit
            )
        })?;
        let mut entries_tree = unit
            .entries_tree(abbrev, Some(offset))
            .map_err(|err| err.to_string())?;
        let entries_tree_node = entries_tree.root().map_err(|err| err.to_string())?;
        let entry = entries_tree_node.entry();
        let typename = get_name_attribute(entry, &self.dwarf, unit).ok();
        let is_declaration = get_declaration_attribute(entry).unwrap_or(false);

        if is_declaration {
            // This is a declaration, not a definition. This happens when a type is declared but not defined
            // e.g. "struct foo;" in a header file.
            // We can't do anything with this - return a dummy type, and don't store it in the types map.
            return Ok(TypeInfo {
                datatype: DbgDataType::Other(0),
                name: typename,
                unit_idx: current_unit,
                dbginfo_offset: dbginfo_offset.0,
            });
        }

        // track in-progress items to prevent infinite recursion
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
            gimli::constants::DW_TAG_pointer_type
            | gimli::constants::DW_TAG_reference_type
            | gimli::constants::DW_TAG_rvalue_reference_type => {
                let (unit, _) = &self.units[current_unit];
                if let Ok(Some((new_cur_unit, ptype_offset))) =
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
                            DbgDataType::Pointer(
                                u64::from(unit.encoding().address_size),
                                ptype_offset.0,
                            ),
                            name.clone(),
                        )
                    } else {
                        let pt_type = self.get_type(new_cur_unit, ptype_offset, typereader_data)?;
                        (
                            DbgDataType::Pointer(
                                u64::from(unit.encoding().address_size),
                                ptype_offset.0,
                            ),
                            pt_type.name,
                        )
                    }
                } else {
                    // void*
                    (
                        DbgDataType::Pointer(u64::from(unit.encoding().address_size), 0),
                        Some("void".to_string()),
                    )
                }
                //DwarfDataType::Pointer(u64::from(unit.encoding().address_size), dest_type)
            }
            gimli::constants::DW_TAG_array_type => {
                self.get_array_type(entry, current_unit, offset, typereader_data)?
            }
            gimli::constants::DW_TAG_enumeration_type => (
                self.get_enumeration_type(current_unit, offset, typereader_data)?,
                None,
            ),
            gimli::constants::DW_TAG_structure_type => (
                self.get_struct_or_class_type(false, current_unit, offset, typereader_data)?,
                None,
            ),
            gimli::constants::DW_TAG_class_type => (
                self.get_struct_or_class_type(true, current_unit, offset, typereader_data)?,
                None,
            ),
            gimli::constants::DW_TAG_union_type => {
                let size = get_byte_size_attribute(entry)
                    .ok_or_else(|| "missing union byte size attribute".to_string())?;
                let (members, _) = self.get_struct_or_union_members(
                    entries_tree_node,
                    current_unit,
                    typereader_data,
                )?;
                (DbgDataType::Union { size, members }, None)
            }
            gimli::constants::DW_TAG_typedef => {
                if let Some((new_cur_unit, dbginfo_offset)) =
                    get_type_attribute(entry, &self.units, current_unit)?
                {
                    let reftype = self.get_type(new_cur_unit, dbginfo_offset, typereader_data)?;
                    (reftype.datatype, None)
                } else {
                    // possibly typedef void? Whatever it is, we can't do anything with it
                    (DbgDataType::Other(0), None)
                }
            }
            gimli::constants::DW_TAG_const_type
            | gimli::constants::DW_TAG_volatile_type
            | gimli::constants::DW_TAG_packed_type
            | gimli::constants::DW_TAG_restrict_type
            | gimli::constants::DW_TAG_immutable_type
            | gimli::constants::DW_TAG_atomic_type => {
                // ignore these tags, they don't matter in the context of a2l files
                // note: some compilers might omit the type reference if the type is void / void*
                if let Ok(Some((new_cur_unit, dbginfo_offset))) =
                    get_type_attribute(entry, &self.units, current_unit)
                {
                    let typeinfo = self.get_type(new_cur_unit, dbginfo_offset, typereader_data)?;
                    (typeinfo.datatype, typeinfo.name)
                } else {
                    // const void* / volatile void* / packed void*???
                    (
                        DbgDataType::Other(u64::from(unit.encoding().address_size)),
                        None,
                    )
                }
            }
            gimli::constants::DW_TAG_subroutine_type => {
                // function pointer
                (
                    DbgDataType::FuncPtr(u64::from(unit.encoding().address_size)),
                    Some("p_function".to_string()),
                )
            }
            gimli::constants::DW_TAG_unspecified_type => {
                // ?
                (
                    DbgDataType::Other(get_byte_size_attribute(entry).unwrap_or(0)),
                    None,
                )
            }
            other_tag => {
                return Err(format!(
                    "unexpected DWARF tag {other_tag} in type definition"
                ));
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
        entry: &gimli::DebuggingInformationEntry<EndianSlice<'_, RunTimeEndian>, usize>,
        current_unit: usize,
        offset: UnitOffset,
        typereader_data: &mut TypeReaderData,
    ) -> Result<(DbgDataType, Option<String>), String> {
        let (unit, abbrev) = &self.units[current_unit];
        let mut entries_tree = unit
            .entries_tree(abbrev, Some(offset))
            .map_err(|err| err.to_string())?;
        let entries_tree_node = entries_tree.root().map_err(|err| err.to_string())?;

        let maybe_size = get_byte_size_attribute(entry);
        let Some((new_cur_unit, arraytype_offset)) =
            get_type_attribute(entry, &self.units, current_unit)?
        else {
            // it seems unlikely that any compiler would ever generate an array type without an element type
            return Err("missing array element type attribute".to_string());
        };

        let arraytype = if typereader_data
            .wip_items
            .iter()
            .any(|item| item.offset == arraytype_offset.0)
        {
            // exceptional case: the array and its element type are recursive
            // it seems it is possible to construct declarations where the element type is the array itself?!
            self.make_arraytype_ref(new_cur_unit, arraytype_offset)?
        } else {
            // this is the standard case, where the element type does not contain the array type
            self.get_type(new_cur_unit, arraytype_offset, typereader_data)?
        };

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
        if dim.len() == 1
            && dim[0] == 0
            && stride != 0
            && let Some(count) = maybe_size.map(|s: u64| s / stride)
        {
            dim[0] = count;
        }
        let size = maybe_size.unwrap_or_else(|| dim.iter().fold(stride, |acc, num| acc * num));
        // other code will fail with divide by zero if stride is zero, so set it to 1 in that case
        let stride = stride.max(1);
        Ok((
            DbgDataType::Array {
                dim,
                arraytype: Box::new(arraytype),
                size,
                stride,
            },
            arraytype_name,
        ))
    }

    /// create a type reference for the element type of an array
    /// this is (very rarely) needed to break a circular dependency while loading array types
    fn make_arraytype_ref(
        &self,
        at_unit_idx: usize,
        arraytype_offset: DebugInfoOffset,
    ) -> Result<TypeInfo, String> {
        let (at_unit, at_abbrev) = &self.units[at_unit_idx];
        let at_offset = arraytype_offset.to_unit_offset(at_unit).ok_or_else(|| {
            format!(
                "invalid type offset 0x{:X} for unit {}",
                arraytype_offset.0, at_unit_idx
            )
        })?;
        let mut at_entries_tree = at_unit
            .entries_tree(at_abbrev, Some(at_offset))
            .map_err(|err| err.to_string())?;
        let at_entries_tree_node = at_entries_tree.root().map_err(|err| err.to_string())?;
        let at_entry = at_entries_tree_node.entry();
        let arraytype_size = get_byte_size_attribute(at_entry).unwrap_or(0);
        let arraytype_name = get_name_attribute(at_entry, &self.dwarf, at_unit).ok();
        Ok(TypeInfo {
            name: arraytype_name,
            unit_idx: at_unit_idx,
            dbginfo_offset: arraytype_offset.0,
            datatype: DbgDataType::TypeRef(arraytype_offset.0, arraytype_size),
        })
    }

    fn get_enumeration_type(
        &self,
        current_unit: usize,
        offset: UnitOffset,
        typereader_data: &mut TypeReaderData,
    ) -> Result<DbgDataType, String> {
        let (unit, abbrev) = &self.units[current_unit];
        let mut entries_tree = unit
            .entries_tree(abbrev, Some(offset))
            .map_err(|err| err.to_string())?;
        let entries_tree_node = entries_tree.root().map_err(|err| err.to_string())?;
        let entry = entries_tree_node.entry();

        let opt_size = get_byte_size_attribute(entry);
        let mut enumerators = Vec::new();
        let (unit, _) = &self.units[current_unit];

        // The enumeration type entry may have a DW_AT_type attribute which refers to the underlying
        // data type used to implement the enumeration
        let (signed, opt_ut_size) = if let Ok(Some((utype_unit, utype_dbginfo_offset))) =
            get_type_attribute(entry, &self.units, current_unit)
            && let Ok(utype) = self.get_type(utype_unit, utype_dbginfo_offset, typereader_data)
        {
            // get size and signedness of the underlying type
            let signed = matches!(
                utype.datatype,
                DbgDataType::Sint8
                    | DbgDataType::Sint16
                    | DbgDataType::Sint32
                    | DbgDataType::Sint64
            );
            (signed, Some(utype.get_size()))
        } else {
            (false, None)
        };

        // if no byte size is given, use the size of the underlying type
        let size = opt_size
            .or(opt_ut_size)
            .ok_or_else(|| "missing enum byte size attribute".to_string())?;

        if size == 0 || size > 8 {
            return Err(format!("invalid enum size {size}"));
        }

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

        let signed = enum_is_signed(signed, size, &enumerators);

        Ok(DbgDataType::Enum {
            size,
            signed,
            enumerators,
        })
    }

    /// Read a struct or class type.
    /// Structs and classes are handled identically: in C++ both can have base classes
    /// and members, and the debug info describes them in the same way. Only the wording
    /// of messages differs, which is what `is_class` is used for.
    fn get_struct_or_class_type(
        &self,
        is_class: bool,
        current_unit: usize,
        offset: UnitOffset,
        typereader_data: &mut TypeReaderData,
    ) -> Result<DbgDataType, String> {
        let kind = if is_class { "class" } else { "struct" };
        let (unit, abbrev) = &self.units[current_unit];
        let mut entries_tree = unit
            .entries_tree(abbrev, Some(offset))
            .map_err(|err| err.to_string())?;
        let entries_tree_node = entries_tree.root().map_err(|err| err.to_string())?;
        let entry = entries_tree_node.entry();

        let size = get_byte_size_attribute(entry)
            .ok_or_else(|| format!("missing {kind} byte size attribute"))?;
        let (mut members, inheritance) =
            self.get_struct_or_union_members(entries_tree_node, current_unit, typereader_data)?;
        // copy all inherited members from the base classes
        // this allows the inherited members ot be accessed without naming the base class
        for (baseclass_type, baseclass_offset) in inheritance.values() {
            if let DbgDataType::Struct {
                members: baseclass_members,
                ..
            } = &baseclass_type.datatype
            {
                for (name, (m_type, m_offset)) in baseclass_members {
                    if !members.contains_key(name) {
                        // if the derived class has a member with the same name as an inherited member, the derived class member takes precedence
                        members.insert(
                            name.to_owned(),
                            (m_type.clone(), m_offset + baseclass_offset),
                        );
                    }
                }
            }
        }
        Ok(DbgDataType::Struct {
            size,
            is_class,
            inheritance,
            members,
        })
    }

    /// get all the members and base classes of a struct or union or class
    /// Unions never have base classes, so the returned inheritance map is empty for them.
    fn get_struct_or_union_members(
        &self,
        entries_tree: EntriesTreeNode<EndianSlice<RunTimeEndian>>,
        current_unit: usize,
        typereader_data: &mut TypeReaderData,
    ) -> Result<(MemberMap, MemberMap), String> {
        let (unit, _) = &self.units[current_unit];
        let mut members = MemberMap::new();
        let mut inheritance = MemberMap::new();
        let mut iter = entries_tree.children();
        while let Ok(Some(child_node)) = iter.next() {
            let child_entry = child_node.entry();
            if child_entry.tag() == gimli::constants::DW_TAG_inheritance {
                // failing to read one base class should not make the whole type unusable,
                // so errors are silently ignored here
                if let Ok((name, baseclass_type, baseclass_offset)) =
                    self.get_inherited_class(child_entry, current_unit, typereader_data)
                {
                    inheritance.insert(name, (baseclass_type, baseclass_offset));
                }
            } else if child_entry.tag() == gimli::constants::DW_TAG_member {
                // Static and constexpr data members are only declared inside the struct: they have
                // no storage of their own and are not part of the struct's layout, so skip them.
                if get_declaration_attribute(child_entry).unwrap_or(false) {
                    continue;
                }

                // the name can be missing if this struct/union contains an anonymous struct/union
                let opt_name = get_name_attribute(child_entry, &self.dwarf, unit).ok();

                // Union members and Dwarf 4/5 bitfields have no DW_AT_data_member_location.
                // Zero is the correct default for union members; for bitfields the byte offset is
                // derived from DW_AT_data_bit_offset in get_bitfield_entry() below.
                let mut offset = get_data_member_location_attribute(
                    self,
                    child_entry,
                    unit.encoding(),
                    current_unit,
                )
                .unwrap_or(0);

                // get the type of the member
                if let Some((new_cur_unit, new_dbginfo_offset)) =
                    get_type_attribute(child_entry, &self.units, current_unit)?
                    && let Ok(mut membertype) =
                        self.get_type(new_cur_unit, new_dbginfo_offset, typereader_data)
                {
                    // wrap bitfield members in a TypeInfo::Bitfield to store bit_size and bit_offset
                    if let Some(bit_size) = get_bit_size_attribute(child_entry) {
                        membertype = self.get_bitfield_entry(
                            unit,
                            child_entry,
                            &mut offset,
                            bit_size,
                            membertype,
                        );
                    }
                    if let Some(name) = opt_name {
                        // in bitfields it's actually possible for the name to be empty!
                        // "int :31;" is valid C!
                        if !name.is_empty() {
                            // refer to the loaded type instead of duplicating it in the members
                            if matches!(
                                membertype.datatype,
                                DbgDataType::Struct { .. } | DbgDataType::Union { .. }
                            ) {
                                membertype.datatype = DbgDataType::TypeRef(
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
                            DbgDataType::Struct {
                                members: anon_members,
                                ..
                            }
                            | DbgDataType::Union {
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
        Ok((members, inheritance))
    }

    fn get_bitfield_entry(
        &self,
        unit: &gimli::UnitHeader<EndianSlice<RunTimeEndian>, usize>,
        child_entry: &gimli::DebuggingInformationEntry<EndianSlice<RunTimeEndian>, usize>,
        offset: &mut u64,
        bit_size: u64,
        mut membertype: TypeInfo,
    ) -> TypeInfo {
        let type_size = membertype.get_size();
        let dbginfo_offset = child_entry.offset().to_debug_info_offset(unit).unwrap().0;

        // The containing type of a bitfield is always an integer type, so it is at most 8
        // bytes. Larger values only occur for garbage input, where DW_AT_byte_size is
        // unconstrained; rejecting them here also keeps type_size * 8 from overflowing.
        if type_size == 0 || type_size > MAX_BITFIELD_CONTAINER_SIZE {
            // invalid bitfield container - return the member type as-is
            return membertype;
        }
        let mut type_size_bits = type_size * 8;

        if bit_size == 0 || bit_size > type_size_bits {
            // invalid bitfield size - return the member type as-is
            // this case doesn't happen with sane input, but is easily triggered by fuzzing
            return membertype;
        }

        // only treat those types as bitfields that have a bit size attribute with a size that is different from the default, or a non-zero offset
        // e.g. base: uint16, with bit-size 16 and offset 0 should not be a bitfield
        // but base: uint16, with bit-size 4 and offset 0 should be a bitfield
        if let Some(bit_offset) = get_bit_offset_attribute(child_entry)
            && (bit_offset != 0 || type_size_bits != bit_size)
        {
            // Dwarf 2 / 3
            let Some(bit_offset_le) = bitfield_offset_le(type_size_bits, bit_offset, bit_size)
            else {
                // invalid bitfield position - return the member type as-is
                return membertype;
            };

            // bit_offset can be negative(!) so it is not guaranteed that the bitfield is fully inside the containing type
            // in this case we'll try to increase the containing type's size
            if !fix_bitfield_container_type(&mut membertype, *offset, bit_size, bit_offset_le) {
                // the containing type is too small and could not be enlarged, so any
                // BIT_MASK derived from these values would be wrong
                return membertype;
            }

            TypeInfo {
                name: membertype.name.clone(),
                unit_idx: membertype.unit_idx,
                dbginfo_offset,
                datatype: DbgDataType::Bitfield {
                    basetype: Box::new(membertype),
                    bit_size: bit_size as u16,
                    bit_offset: bit_offset_le as u16,
                },
            }
        } else if let Some(mut data_bit_offset) = get_data_bit_offset_attribute(child_entry)
            && (data_bit_offset != 0 || type_size_bits != bit_size)
        {
            // Dwarf 4 / 5:
            // The data bit offset attribute is the offset in bits from the beginning of the containing storage to the beginning of the value
            // this means the bitfield member may have type uint32, but have an offset > 32 bits
            if data_bit_offset >= type_size_bits {
                // Dwarf 4 / 5: re-calculate offset
                // (data_bit_offset / type_size_bits) * type_size is at most
                // data_bit_offset / 8, so only the addition can overflow
                let Some(new_offset) =
                    offset.checked_add((data_bit_offset / type_size_bits) * type_size)
                else {
                    // garbage input: the position of the bitfield is not representable
                    return membertype;
                };
                *offset = new_offset;
                data_bit_offset %= type_size_bits;
            }
            if data_bit_offset + bit_size > type_size_bits {
                // the bitfield crosses the end of the containing type; try to grow the
                // container, exactly as in the Dwarf 2 / 3 case above
                if !fix_bitfield_container_type(&mut membertype, *offset, bit_size, data_bit_offset)
                {
                    return membertype;
                }
                type_size_bits = membertype.get_size() * 8;
            }
            if self.endian == Endianness::Big {
                // reverse the mask for big endian. Example
                // In: type_size 32, offset: 5, size 4 -> 0000_0000_0000_0000_0000_0001_1110_0000
                // Out: offset = 32 - 5 - 4 = 23       -> 0000_0111_1000_0000_0000_0000_0000_0000
                data_bit_offset = type_size_bits - data_bit_offset - bit_size;
            }
            // these values should be independent of Endianness
            TypeInfo {
                name: membertype.name.clone(),
                unit_idx: membertype.unit_idx,
                dbginfo_offset,
                datatype: DbgDataType::Bitfield {
                    basetype: Box::new(membertype),
                    bit_size: bit_size as u16,
                    bit_offset: data_bit_offset as u16,
                },
            }
        } else {
            membertype
        }
    }

    /// read one `DW_TAG_inheritance` entry, i.e. one base class of a struct or class,
    /// and return its name, type and offset inside the derived type
    fn get_inherited_class(
        &self,
        child_entry: &gimli::DebuggingInformationEntry<EndianSlice<RunTimeEndian>, usize>,
        current_unit: usize,
        typereader_data: &mut TypeReaderData,
    ) -> Result<(String, TypeInfo, u64), String> {
        let (unit, _) = &self.units[current_unit];
        let data_location =
            get_data_member_location_attribute(self, child_entry, unit.encoding(), current_unit)
                .ok_or_else(|| "missing byte offset for inherited class".to_string())?;

        let Some((new_cur_unit, new_dbginfo_offset)) =
            get_type_attribute(child_entry, &self.units, current_unit)?
        else {
            // a base class whose type is "nothing"?
            return Err("missing type for inherited class".to_string());
        };

        let (unit, abbrev) = &self.units[new_cur_unit];
        let new_unit_offset = new_dbginfo_offset.to_unit_offset(unit).ok_or_else(|| {
            format!(
                "invalid type offset 0x{:X} for unit {}",
                new_dbginfo_offset.0, new_cur_unit
            )
        })?;
        let mut baseclass_tree = unit
            .entries_tree(abbrev, Some(new_unit_offset))
            .map_err(|err| err.to_string())?;
        let baseclass_tree_node = baseclass_tree.root().map_err(|err| err.to_string())?;
        let baseclass_entry = baseclass_tree_node.entry();
        let baseclass_name = get_name_attribute(baseclass_entry, &self.dwarf, unit)?;

        let baseclass_type = self.get_type(new_cur_unit, new_dbginfo_offset, typereader_data)?;

        Ok((baseclass_name, baseclass_type, data_location))
    }
}

/// Compute the little-endian bit offset of a DWARF 2/3 bitfield: the offset of the bitfield's
/// least significant bit from the least significant bit of the containing type.
///
/// `DW_AT_bit_offset` counts from the most significant bit of the containing type and can even be
/// negative: compilers emit a negative value when the bitfield extends past the end of the
/// containing type. `get_bit_offset_attribute` sign-extends such a value into a `u64`, so we
/// reinterpret the `u64` as an `i64` and perform the subtraction in `i128`. This both produces the
/// correct result for valid input and lets us detect invalid input.
///
/// Returns `None` if the values do not describe a bitfield that [`fix_bitfield_container_type`]
/// could place inside a containing type: either it starts before the beginning of the containing
/// type, or it extends more than one size step past the end. Compilers never produce such input.
fn bitfield_offset_le(type_size_bits: u64, bit_offset: u64, bit_size: u64) -> Option<u64> {
    // The caller has limited type_size_bits to MAX_BITFIELD_CONTAINER_SIZE * 8
    // and bit_size to type_size_bits, so only bit_offset can be out of range here.
    let bit_offset_le =
        i128::from(type_size_bits) - i128::from(bit_offset as i64) - i128::from(bit_size);
    let bit_offset_le = u64::try_from(bit_offset_le).ok()?;
    (bit_offset_le + bit_size <= type_size_bits * 2).then_some(bit_offset_le)
}

/// Ensure the bitfield described by `bit_offset` and `bit_size` fits inside `membertype`, growing
/// `membertype` by one size step if it does not.
///
/// Returns `false` if the bitfield does not fit and the containing type cannot be grown. The
/// caller must not build a `Bitfield` from these values in that case, because the resulting
/// BIT_MASK would refer to bits outside of the item.
fn fix_bitfield_container_type(
    membertype: &mut TypeInfo,
    offset: u64,
    bit_size: u64,
    bit_offset: u64,
) -> bool {
    let type_size = membertype.get_size();
    if bit_offset + bit_size <= type_size * 8 {
        // The bitfield is fully inside the containing type – nothing to do.
        return true;
    }
    let adjusted_type_size = type_size * 2;

    let unaligned_bytes = offset % adjusted_type_size;
    // Growing the containing type won't work if it would create unaligned access.
    if unaligned_bytes != 0 {
        return false;
    }
    // Increase any integer data type by one step. Both callers have verified that the bitfield
    // ends within twice the size of the containing type, so one step is enough.
    match membertype.datatype {
        DbgDataType::Uint8 => membertype.datatype = DbgDataType::Uint16,
        DbgDataType::Uint16 => membertype.datatype = DbgDataType::Uint32,
        DbgDataType::Uint32 => membertype.datatype = DbgDataType::Uint64,
        DbgDataType::Sint8 => membertype.datatype = DbgDataType::Sint16,
        DbgDataType::Sint16 => membertype.datatype = DbgDataType::Sint32,
        DbgDataType::Sint32 => membertype.datatype = DbgDataType::Sint64,
        _ => {
            // Either there is no larger integer type, or the containing type is not an integer
            // type at all. Compilers never emit a bitfield in either of these situations.
            return false;
        }
    }
    // We could theoretically also handle cases where the containing type
    // would become unaligned, but that would require increasing the size of
    // the containing type by several steps, extending it both forward and
    // backward. It is unclear whether any compiler would generate such code.
    true
}

/// Decide whether an enum should be treated as signed.
///
/// Some compilers claim that an enum is unsigned, but then put negative values in it.
/// Such an enum is treated as signed, as long as every value still fits into a signed
/// integer of the enum's size.
fn enum_is_signed(underlying_signed: bool, size: u64, enumerators: &[(String, i64)]) -> bool {
    if underlying_signed {
        return true;
    }

    let min_val = enumerators.iter().map(|(_, val)| *val).min().unwrap_or(0);
    let max_val = enumerators.iter().map(|(_, val)| *val).max().unwrap_or(0);
    // i128 is required here: for size == 8 the bound is 2^63, which does not fit in an i64
    let signed_limit = 1i128 << (size.clamp(1, 8) * 8 - 1);

    min_val < 0 && i128::from(max_val) < signed_limit
}

fn get_base_type(
    entry: &gimli::DebuggingInformationEntry<EndianSlice<RunTimeEndian>, usize>,
    unit: &gimli::UnitHeader<EndianSlice<RunTimeEndian>>,
) -> (DbgDataType, String) {
    let byte_size = get_byte_size_attribute(entry).unwrap_or(1u64);
    let encoding = get_encoding_attribute(entry).unwrap_or(gimli::constants::DW_ATE_unsigned);
    match encoding {
        gimli::constants::DW_ATE_address => {
            // if compilers use DW_TAG_base_type with DW_AT_encoding = DW_ATE_address, then it is only used for void pointers
            // in all other cases DW_AT_pointer is used
            (
                DbgDataType::Pointer(u64::from(unit.encoding().address_size), 0),
                "unknown".to_string(),
            )
        }
        gimli::constants::DW_ATE_float => {
            if byte_size == 8 {
                (DbgDataType::Double, "double".to_string())
            } else {
                (DbgDataType::Float, "float".to_string())
            }
        }
        gimli::constants::DW_ATE_signed | gimli::constants::DW_ATE_signed_char => match byte_size {
            1 => (DbgDataType::Sint8, "sint8".to_string()),
            2 => (DbgDataType::Sint16, "sint16".to_string()),
            4 => (DbgDataType::Sint32, "sint32".to_string()),
            8 => (DbgDataType::Sint64, "sint64".to_string()),
            _ => (DbgDataType::Other(byte_size), "other".to_string()),
        },
        gimli::constants::DW_ATE_boolean
        | gimli::constants::DW_ATE_unsigned
        | gimli::constants::DW_ATE_unsigned_char => match byte_size {
            1 => (DbgDataType::Uint8, "uint8".to_string()),
            2 => (DbgDataType::Uint16, "uint16".to_string()),
            4 => (DbgDataType::Uint32, "uint32".to_string()),
            8 => (DbgDataType::Uint64, "uint64".to_string()),
            _ => (DbgDataType::Other(byte_size), "other".to_string()),
        },
        _other => (DbgDataType::Other(byte_size), "other".to_string()),
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

#[cfg(test)]
mod test {
    use super::*;

    fn enumerators(values: &[i64]) -> Vec<(String, i64)> {
        values
            .iter()
            .map(|val| (format!("item_{val}"), *val))
            .collect()
    }

    #[test]
    fn test_enum_is_signed() {
        // a signed underlying type always wins, even for an enum with no negative values
        assert!(enum_is_signed(true, 4, &enumerators(&[0, 1, 2])));

        // no negative values: the enum stays unsigned
        assert!(!enum_is_signed(false, 1, &enumerators(&[0, 255])));
        assert!(!enum_is_signed(false, 8, &enumerators(&[0, 1])));
        assert!(!enum_is_signed(false, 4, &[]));

        // negative values in an "unsigned" enum: treat it as signed for every size.
        // Previously this failed for size == 8, because the limit 2^63 was computed in an
        // i64 and overflowed to i64::MIN, making the comparison always false.
        for size in 1..=8 {
            assert!(
                enum_is_signed(false, size, &enumerators(&[-1, 5])),
                "size {size} was not detected as signed"
            );
        }
        assert!(enum_is_signed(
            false,
            8,
            &enumerators(&[i64::MIN, 0, i64::MAX])
        ));

        // negative values, but the largest value does not fit into a signed type of this
        // size, so the enum cannot be represented as signed and stays unsigned
        assert!(!enum_is_signed(false, 1, &enumerators(&[-1, 200])));
        assert!(!enum_is_signed(false, 2, &enumerators(&[-1, 40000])));
        // the value exactly at the limit is out of range, the one below it is not
        assert!(!enum_is_signed(false, 1, &enumerators(&[-1, 128])));
        assert!(enum_is_signed(false, 1, &enumerators(&[-1, 127])));
    }

    /// imitate `get_bit_offset_attribute()` by casting `DW_AT_bit_offset`s to u64
    fn bit_offset(value: i64) -> u64 {
        value as u64
    }

    /// Test Dwarf 2/3 bitfield handling. The details of the encoding are different from
    /// version 4 onward.
    #[test]
    fn test_bitfield_offset_le() {
        // an ordinary bitfield: 5 bits at the top of a 32 bit container are the bits
        // 27..32 counted from the least significant bit
        assert_eq!(bitfield_offset_le(32, bit_offset(0), 5), Some(27));
        assert_eq!(bitfield_offset_le(32, bit_offset(27), 5), Some(0));
        assert_eq!(bitfield_offset_le(8, bit_offset(3), 5), Some(0));

        // a negative bit offset: the bitfield extends past the end of its containing
        // type and fix_bitfield_container_type() expands it
        assert_eq!(bitfield_offset_le(8, bit_offset(-2), 5), Some(5));
        assert_eq!(bitfield_offset_le(32, bit_offset(-5), 30), Some(7));

        // the bitfield starts before the beginning of the containing type
        assert_eq!(bitfield_offset_le(8, bit_offset(27), 5), None);
        assert_eq!(bitfield_offset_le(8, bit_offset(8), 1), None);

        // the bitfield ends more than one size step past the end of the containing type,
        // so growing that type once would not be enough
        assert_eq!(bitfield_offset_le(8, bit_offset(-3), 2), Some(9));
        assert_eq!(bitfield_offset_le(8, bit_offset(-25), 2), None);

        // extreme values must not panic or wrap around.
        // DW_AT_bit_offset of -1 becomes u64::MAX, so it is accepted.
        assert_eq!(bitfield_offset_le(8, u64::MAX, 1), Some(8));
        assert_eq!(bitfield_offset_le(8, bit_offset(i64::MIN), 1), None);
        assert_eq!(bitfield_offset_le(8, bit_offset(i64::MAX), 1), None);
    }

    fn typeinfo(datatype: DbgDataType) -> TypeInfo {
        TypeInfo {
            name: None,
            unit_idx: 0,
            dbginfo_offset: 0,
            datatype,
        }
    }

    #[test]
    fn test_fix_bitfield_container_type() {
        // the bitfield fits: the containing type is left alone
        let mut membertype = typeinfo(DbgDataType::Uint8);
        assert!(fix_bitfield_container_type(&mut membertype, 0, 5, 3));
        assert!(matches!(membertype.datatype, DbgDataType::Uint8));

        // the bitfield extends past the end, so the containing type grows by one step
        let mut membertype = typeinfo(DbgDataType::Uint8);
        assert!(fix_bitfield_container_type(&mut membertype, 0, 5, 5));
        assert!(matches!(membertype.datatype, DbgDataType::Uint16));

        let mut membertype = typeinfo(DbgDataType::Sint32);
        assert!(fix_bitfield_container_type(&mut membertype, 8, 30, 7));
        assert!(matches!(membertype.datatype, DbgDataType::Sint64));

        // growing the containing type would make the access unaligned
        let mut membertype = typeinfo(DbgDataType::Uint8);
        assert!(!fix_bitfield_container_type(&mut membertype, 1, 5, 5));
        assert!(matches!(membertype.datatype, DbgDataType::Uint8));

        // there is no next size step for a 64 bit type or for a type that isn't an integer
        let mut membertype = typeinfo(DbgDataType::Uint64);
        assert!(!fix_bitfield_container_type(&mut membertype, 0, 5, 60));
        let mut membertype = typeinfo(DbgDataType::Float);
        assert!(!fix_bitfield_container_type(&mut membertype, 0, 5, 30));
    }
}
