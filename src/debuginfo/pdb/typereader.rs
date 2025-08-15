use crate::debuginfo::{DbgDataType, TypeInfo, VarInfo};
use indexmap::IndexMap;
use pdb2::{FallibleIterator, ItemIter, PDB, TypeData, TypeIndex};
use std::{collections::HashMap, fs::File};

mod builtin_types;

#[derive(Debug)]
struct WipItemInfo {
    offset: u32,
    name: Option<String>,
}

pub(crate) struct TypeReaderData {
    pub(crate) types: HashMap<usize, TypeInfo>,
    pub(crate) typenames: HashMap<String, Vec<usize>>,
    wip_items: Vec<WipItemInfo>,
}

struct PdbData<'t> {
    types: Vec<TypeData<'t>>,
    raw_kind: Vec<u16>,
    id_lookup: HashMap<u32, usize>,
    name_lookup: HashMap<u16, HashMap<String, (u32, usize)>>,
}

impl<'t> PdbData<'t> {
    fn new(type_iter: &mut ItemIter<'t, TypeIndex>) -> Result<Self, pdb2::Error> {
        let mut pdb_data: PdbData<'t> = PdbData {
            types: Vec::new(),
            raw_kind: Vec::new(),
            id_lookup: HashMap::<u32, usize>::new(),
            name_lookup: HashMap::<u16, HashMap<String, (u32, usize)>>::new(),
        };
        while let Some(t) = type_iter.next()? {
            if let Ok(t_parsed) = t.parse() {
                let item_idx = pdb_data.types.len();
                let type_id = t.index().0;
                pdb_data.id_lookup.insert(type_id, item_idx);

                let raw_kind = t.raw_kind();
                // prepare the lookup able that is used to resolve forward references
                // only store complex types that are not forward references themselves
                if matches!(
                    t_parsed,
                    TypeData::Class(_) | TypeData::Enumeration(_) | TypeData::Union(_)
                ) && !Self::is_forward_reference(&t_parsed)
                {
                    // must have a unique name to be resolvable
                    if let Some(uname) = Self::unique_name_of(&t_parsed) {
                        pdb_data
                            .name_lookup
                            .entry(raw_kind)
                            .or_default()
                            .insert(uname, (type_id, item_idx));
                    }
                }

                pdb_data.types.push(t_parsed);
                pdb_data.raw_kind.push(raw_kind);
            }
        }
        Ok(pdb_data)
    }

    fn get_type_by_id(&self, id: u32) -> Option<&TypeData<'t>> {
        self.id_lookup.get(&id).and_then(|idx| self.types.get(*idx))
    }

    fn lookup_forward_reference(&self, type_id: u32) -> Option<(u32, &TypeData<'t>)> {
        let idx = *self.id_lookup.get(&type_id)?;
        let input_type = &self.types[idx];
        let unique_name = Self::unique_name_of(input_type)?;
        // only complex types struct / class / union use forward references, and they indicate this with a flag
        if !Self::is_forward_reference(input_type) {
            return Some((type_id, input_type));
        }
        let type_kind = self.raw_kind[idx];

        // lookup first by kind and then by unique name
        if let Some(referenced_type) = self
            .name_lookup
            .get(&type_kind)
            .and_then(|lookup| lookup.get(&unique_name))
            .and_then(|(ref_type_id, idx)| self.types.get(*idx).map(|t| (*ref_type_id, t)))
        {
            Some(referenced_type)
        } else {
            // lookup failed, return a circular reference
            Some((type_id, input_type))
        }
    }

    fn unique_name_of(type_data: &TypeData<'_>) -> Option<String> {
        match type_data {
            TypeData::Class(class_type) => class_type.unique_name,
            TypeData::Enumeration(enum_type) => enum_type.unique_name,
            TypeData::Union(union_type) => union_type.unique_name,
            _ => None,
        }
        .map(|n| n.to_string().into())
    }

    fn is_forward_reference(type_data: &TypeData<'_>) -> bool {
        match type_data {
            TypeData::Class(class_type) => class_type.properties.forward_reference(),
            TypeData::Enumeration(enum_type) => enum_type.properties.forward_reference(),
            TypeData::Union(union_type) => union_type.properties.forward_reference(),
            _ => false,
        }
    }
}

pub(crate) fn read_all_types(
    pdb: &mut PDB<'_, File>,
    variables: &IndexMap<String, Vec<VarInfo>>,
) -> Result<TypeReaderData, pdb2::Error> {
    let mut typereader_data = TypeReaderData {
        types: HashMap::<usize, TypeInfo>::new(),
        typenames: HashMap::<String, Vec<usize>>::new(),
        wip_items: Vec::new(),
    };
    // parse all the types in the PDB file
    let type_information = pdb.type_information()?;
    let mut type_iter = type_information.iter();
    let pdb_data = PdbData::new(&mut type_iter)?;

    // read all of the types that are used by the variables and convert them into TypeInfo objects
    for (varname, vars) in variables {
        for var in vars {
            let type_index = var.typeref;
            if typereader_data.types.contains_key(&type_index) {
                continue;
            }

            match read_type(type_index as u32, &mut typereader_data, &pdb_data) {
                Ok(_) => {}
                Err(err) => {
                    println!(
                        "for variable {varname}: Error reading type 0x{type_index:X}: {err:?}"
                    );
                }
            }
            typereader_data.wip_items.clear();
        }
    }

    Ok(typereader_data)
}

fn read_type(
    type_index: u32,
    typereader_data: &mut TypeReaderData,
    pdb_data: &PdbData<'_>,
) -> Result<(), String> {
    // if the type is already read, return
    if typereader_data.types.contains_key(&(type_index as usize)) {
        return Ok(());
    }

    // types below 0x1000 are built-in types
    if builtin_types::is_builtin_type(type_index) {
        return builtin_types::read_builtin_type(type_index, typereader_data);
    }

    // get the parsed type data
    let type_data = pdb_data.get_type_by_id(type_index).ok_or_else(|| {
        format!("read_type: Type with ID 0x{type_index:X} not found in the PDB file")
    })?;

    // store the type name in the WIP list to prevent infinite loops when following pointers
    let typename = type_data.name().map(|n| n.to_string().into());
    typereader_data.wip_items.push(WipItemInfo {
        offset: type_index,
        name: typename.clone(),
    });

    // build an a2l type from the pdb type data
    if let Some((datatype, inner_name)) =
        read_type_from_typedata(type_index, type_data, typereader_data, pdb_data)?
    {
        let is_ref = matches!(datatype, DbgDataType::TypeRef(_, _));
        // use the inner name as a display name for the type if the type has no name of its own
        let display_name = typename.clone().or(inner_name);
        let typeinfo = TypeInfo {
            datatype,
            name: display_name,
            unit_idx: 0, // in the PDB, all types are global
            dbginfo_offset: type_index as usize,
        };

        typereader_data.types.insert(type_index as usize, typeinfo);
        if !is_ref && let Some(typename) = typename {
            typereader_data
                .typenames
                .entry(typename)
                .or_default()
                .push(type_index as usize);
        }
    }
    typereader_data.wip_items.pop();

    Ok(())
}

fn read_type_from_typedata(
    type_index: u32,
    type_data: &TypeData<'_>,
    typereader_data: &mut TypeReaderData,
    pdb_data: &PdbData<'_>,
) -> Result<Option<(DbgDataType, Option<String>)>, String> {
    let (datatype, inner_name) = match type_data {
        TypeData::Primitive(primitive_type) => read_primitive_type(primitive_type),
        TypeData::Class(class_type) => {
            if class_type.properties.forward_reference() {
                // find and read the referenced stuct / class, or return an empty struct if it's not resolvable
                read_forward_referenced_type(type_index, typereader_data, pdb_data)?.unwrap_or((
                    DbgDataType::Struct {
                        size: class_type.size,
                        members: IndexMap::new(),
                    },
                    None,
                ))
            } else {
                read_class(class_type, typereader_data, pdb_data)?
            }
        }
        TypeData::Union(union_type) => {
            if union_type.properties.forward_reference() {
                // find and read the referenced union, or return an empty placeholder type
                read_forward_referenced_type(type_index, typereader_data, pdb_data)?.unwrap_or((
                    DbgDataType::Union {
                        size: union_type.size,
                        members: IndexMap::new(),
                    },
                    None,
                ))
            } else {
                read_union(union_type, typereader_data, pdb_data)?
            }
        }
        TypeData::Pointer(pointer_type) => read_pointer(pointer_type, typereader_data, pdb_data)?,
        TypeData::Modifier(modifier_type) => {
            read_modifier(modifier_type, typereader_data, pdb_data)?
        }
        TypeData::Enumeration(enumeration_type) => {
            read_enum(enumeration_type, typereader_data, pdb_data)?
        }
        TypeData::Array(array_type) => read_array(array_type, typereader_data, pdb_data)?,
        TypeData::Bitfield(bitfield_type) => {
            read_bitfield(bitfield_type, typereader_data, pdb_data)?
        }
        TypeData::MemberFunction(_) | TypeData::Procedure(_) => {
            // probably not relevant for a2l
            return Ok(None);
        }
        TypeData::Enumerate(_)
        | TypeData::Member(_)
        | TypeData::OverloadedMethod(_)
        | TypeData::Method(_)
        | TypeData::StaticMember(_)
        | TypeData::Nested(_)
        | TypeData::BaseClass(_)
        | TypeData::VirtualBaseClass(_)
        | TypeData::VirtualFunctionTablePointer(_)
        | TypeData::FieldList(_)
        | TypeData::ArgumentList(_) => {
            // all of these types should only appear in the context of other types
            // e.g. a field list is nested inside a class type, and an enumerate is nested inside an enumeration type
            unreachable!("Type {type_data:?} should not be encountered here");
        }
        TypeData::MethodList(_) => todo!(),
        _ => {
            return Err(format!("Could not read unknown type: {type_data:?}"));
        }
    };
    Ok(Some((datatype, inner_name)))
}

fn read_primitive_type(primitive_type: &pdb2::PrimitiveType) -> (DbgDataType, Option<String>) {
    let (datatype, name) = match primitive_type.kind {
        pdb2::PrimitiveKind::NoType => (DbgDataType::Uint8, "notype"), // representing this as uint8 allows variables of this type to be inserted in the a2l
        pdb2::PrimitiveKind::Void => (DbgDataType::Other(0), "void"),
        pdb2::PrimitiveKind::Char => (DbgDataType::Sint8, "char"),
        pdb2::PrimitiveKind::UChar => (DbgDataType::Sint8, "uchar"),
        pdb2::PrimitiveKind::RChar => (DbgDataType::Uint8, "rchar"),
        pdb2::PrimitiveKind::WChar => (DbgDataType::Uint16, "wchar"),
        pdb2::PrimitiveKind::RChar16 => (DbgDataType::Uint16, "rchar16"),
        pdb2::PrimitiveKind::RChar32 => (DbgDataType::Uint32, "rchar32"),
        pdb2::PrimitiveKind::I8 => (DbgDataType::Sint8, "i8"),
        pdb2::PrimitiveKind::U8 => (DbgDataType::Uint8, "u8"),
        pdb2::PrimitiveKind::Short => (DbgDataType::Sint16, "short"),
        pdb2::PrimitiveKind::UShort => (DbgDataType::Uint16, "ushort"),
        pdb2::PrimitiveKind::I16 => (DbgDataType::Sint16, "i16"),
        pdb2::PrimitiveKind::U16 => (DbgDataType::Uint16, "u16"),
        pdb2::PrimitiveKind::Long => (DbgDataType::Sint32, "long"),
        pdb2::PrimitiveKind::ULong => (DbgDataType::Uint32, "ulong"),
        pdb2::PrimitiveKind::I32 => (DbgDataType::Sint32, "i32"),
        pdb2::PrimitiveKind::U32 => (DbgDataType::Uint32, "u32"),
        pdb2::PrimitiveKind::Quad => (DbgDataType::Sint64, "quad"),
        pdb2::PrimitiveKind::UQuad => (DbgDataType::Uint64, "uquad"),
        pdb2::PrimitiveKind::I64 => (DbgDataType::Sint64, "i64"),
        pdb2::PrimitiveKind::U64 => (DbgDataType::Uint64, "u64"),
        pdb2::PrimitiveKind::F32 => (DbgDataType::Float, "f32"),
        pdb2::PrimitiveKind::F64 => (DbgDataType::Double, "f64"),
        pdb2::PrimitiveKind::Bool8 => (DbgDataType::Uint8, "bool8"),
        pdb2::PrimitiveKind::Bool16 => (DbgDataType::Uint16, "bool16"),
        pdb2::PrimitiveKind::Bool32 => (DbgDataType::Uint32, "bool32"),
        pdb2::PrimitiveKind::Bool64 => (DbgDataType::Uint64, "bool64"),
        // types below are not supported by a2l
        pdb2::PrimitiveKind::Octa => (DbgDataType::Other(16), "octa"),
        pdb2::PrimitiveKind::UOcta => (DbgDataType::Other(16), "uocta"),
        pdb2::PrimitiveKind::I128 => (DbgDataType::Other(16), "i128"),
        pdb2::PrimitiveKind::U128 => (DbgDataType::Other(16), "u128"),
        pdb2::PrimitiveKind::F16 => (DbgDataType::Other(2), "f16"),
        pdb2::PrimitiveKind::F32PP => (DbgDataType::Other(4), "f32pp"),
        pdb2::PrimitiveKind::F48 => (DbgDataType::Other(6), "f48"),
        pdb2::PrimitiveKind::F80 => (DbgDataType::Other(10), "f80"),
        pdb2::PrimitiveKind::F128 => (DbgDataType::Other(16), "f128"),
        pdb2::PrimitiveKind::Complex32 => (DbgDataType::Other(4), "complex32"),
        pdb2::PrimitiveKind::Complex64 => (DbgDataType::Other(8), "complex64"),
        pdb2::PrimitiveKind::Complex80 => (DbgDataType::Other(10), "complex80"),
        pdb2::PrimitiveKind::Complex128 => (DbgDataType::Other(16), "complex128"),
        pdb2::PrimitiveKind::HRESULT => (DbgDataType::Other(4), "HRESULT"),
        _ => (DbgDataType::Other(0), "unknown"),
    };

    (datatype, Some(name.to_string()))
}

fn read_forward_referenced_type(
    type_index: u32,
    typereader_data: &mut TypeReaderData,
    pdb_data: &PdbData<'_>,
) -> Result<Option<(DbgDataType, Option<String>)>, String> {
    // When a class or union type is a forward reference, the index of the referenced type is unknown.
    // The index of the referenced type is resolved by matching up the type kind and unique name in
    // a table that was previously built by reading all types in the PDB file
    let (fw_ref_idx, _) = pdb_data
        .lookup_forward_reference(type_index)
        .expect("failed to lookup forward reference");
    // PDB file contain some cases where the forward reference is not resolvable or points to itself
    if fw_ref_idx != type_index {
        read_type(fw_ref_idx, typereader_data, pdb_data)?;
        let referenced_type = typereader_data
            .types
            .get(&(fw_ref_idx as usize))
            .ok_or_else(|| {
                format!("Forward reference type 0x{fw_ref_idx:X} was not loaded correctly")
            })?;

        Ok(Some((
            DbgDataType::TypeRef(referenced_type.dbginfo_offset, referenced_type.get_size()),
            referenced_type.name.clone(),
        )))
    } else {
        // unresolvable forward reference
        Ok(None)
    }
}

fn read_class(
    class_type: &pdb2::ClassType<'_>,
    typereader_data: &mut TypeReaderData,
    pdb_data: &PdbData<'_>,
) -> Result<(DbgDataType, Option<String>), String> {
    let size = class_type.size;
    let fields_index = class_type.fields.map(|tidx| tidx.0);
    let datatype = if let Some(fields_index) = fields_index {
        let mut members = read_fields(fields_index, typereader_data, pdb_data)?;
        let inheritance = read_class_inheritance(fields_index, typereader_data, pdb_data)?;

        if inheritance.is_empty() {
            DbgDataType::Struct { size, members }
        } else {
            // copy all inherited members from the base classes
            // this allows the inherited members ot be accessed without naming the base class
            for (baseclass_type, baseclass_offset) in inheritance.values() {
                match &baseclass_type.datatype {
                    DbgDataType::Struct {
                        members: baseclass_members,
                        ..
                    }
                    | DbgDataType::Class {
                        members: baseclass_members,
                        ..
                    } => {
                        for (name, (m_type, m_offset)) in baseclass_members {
                            members.insert(
                                name.clone(),
                                (m_type.clone(), m_offset + baseclass_offset),
                            );
                        }
                    }
                    _ => {
                        return Err(format!(
                            "Base class type 0x{:?} is not a struct or class",
                            baseclass_type.datatype
                        ));
                    }
                }
            }

            DbgDataType::Class {
                size,
                members,
                inheritance,
            }
        }
    } else {
        // empty struct / class
        DbgDataType::Struct {
            size,
            members: IndexMap::new(),
        }
    };

    Ok((datatype, None))
}

fn read_union(
    union_type: &pdb2::UnionType<'_>,
    typereader_data: &mut TypeReaderData,
    pdb_data: &PdbData<'_>,
) -> Result<(DbgDataType, Option<String>), String> {
    let size = union_type.size;
    let fields_index = union_type.fields.0;
    let members = read_fields(fields_index, typereader_data, pdb_data)?;
    let datatype = DbgDataType::Union { members, size };
    Ok((datatype, None))
}

fn read_fields(
    type_index: u32,
    typereader_data: &mut TypeReaderData,
    pdb_data: &PdbData<'_>,
) -> Result<IndexMap<String, (TypeInfo, u64)>, String> {
    let mut opt_fields_index = Some(type_index);

    let mut members = IndexMap::new();
    while let Some(fields_index) = opt_fields_index {
        // get the field list
        let fields_typedata = pdb_data.get_type_by_id(fields_index).ok_or_else(|| {
            format!("Fieldlist: Type with ID 0x{fields_index:X} not found in the PDB file")
        })?;

        // check if the field list is actually a field list
        let TypeData::FieldList(field_list) = fields_typedata else {
            return Err(format!("Expected a field list, got: {fields_typedata:?}"));
        };

        for field in &field_list.fields {
            if let TypeData::Member(member) = field {
                let member_name = member.name.to_string().into();
                let offset = member.offset;

                read_type(member.field_type.0, typereader_data, pdb_data)?;
                let member_typeinfo = typereader_data
                    .types
                    .get(&(member.field_type.0 as usize))
                    .ok_or_else(|| {
                        format!(
                            "Member type 0x{:X} was not loaded correctly",
                            member.field_type.0
                        )
                    })?;

                let typeinfo = if matches!(
                    member_typeinfo.datatype,
                    DbgDataType::Struct { .. }
                        | DbgDataType::Class { .. }
                        | DbgDataType::Union { .. }
                ) {
                    // create a reference to the type instead of embedding it
                    let name = member_typeinfo.name.clone();

                    let datatype = DbgDataType::TypeRef(
                        member.field_type.0 as usize,
                        member_typeinfo.get_size(),
                    );
                    TypeInfo {
                        datatype,
                        name,
                        unit_idx: 0,
                        dbginfo_offset: 0,
                    }
                } else {
                    // use simple types directly
                    member_typeinfo.clone()
                };

                members.insert(member_name, (typeinfo, offset));
            }
        }

        // if the field list has a continuation, read the next one
        opt_fields_index = field_list.continuation.map(|fidx| fidx.0);
    }

    Ok(members)
}

fn read_class_inheritance(
    type_index: u32,
    typereader_data: &mut TypeReaderData,
    pdb_data: &PdbData<'_>,
) -> Result<IndexMap<String, (TypeInfo, u64)>, String> {
    let mut inheritance = IndexMap::new();
    let mut opt_fields_index = Some(type_index);

    while let Some(fields_index) = opt_fields_index {
        // get the field list
        let fields_typedata = pdb_data.get_type_by_id(fields_index).ok_or_else(|| {
            format!("Fieldlist: Type with ID 0x{fields_index:X} not found in the PDB file")
        })?;

        // check if the field list is actually a field list
        let TypeData::FieldList(field_list) = fields_typedata else {
            return Err(format!("Expected a field list, got: {fields_typedata:?}"));
        };

        for field in &field_list.fields {
            if let TypeData::BaseClass(baseclass) = field {
                let bcindex = baseclass.base_class.0;
                let base_offset = baseclass.offset as u64;

                read_type(bcindex, typereader_data, pdb_data)?;
                let Some(typeinfo) = typereader_data.types.get(&(bcindex as usize)) else {
                    return Err(format!(
                        "Base class type 0x{bcindex:X} was not loaded correctly"
                    ));
                };
                // follow the TypeRef, if any
                let referenced_typeinfo = typeinfo.get_reference(&typereader_data.types);

                let name = referenced_typeinfo
                    .name
                    .clone()
                    .expect("class lacks a name");

                inheritance.insert(name, (referenced_typeinfo.clone(), base_offset));
            }
        }

        // if the field list has a continuation, read the next one
        opt_fields_index = field_list.continuation.map(|fidx| fidx.0);
    }

    Ok(inheritance)
}

fn read_enum(
    enumeration_type: &pdb2::EnumerationType<'_>,
    typereader_data: &mut TypeReaderData,
    pdb_data: &PdbData<'_>,
) -> Result<(DbgDataType, Option<String>), String> {
    let type_idx = enumeration_type.underlying_type.0;
    read_type(type_idx, typereader_data, pdb_data)?;
    let ut = typereader_data.types.get(&(type_idx as usize)).unwrap();

    // get the signedness of the underlying type
    let signed = matches!(
        ut.datatype,
        DbgDataType::Sint8 | DbgDataType::Sint16 | DbgDataType::Sint32 | DbgDataType::Sint64
    );

    let mut enumerators = Vec::new();

    // process the field list that contains the enumerators. If the field list index is 0, then the enum has no enumerators
    let fidx = enumeration_type.fields.0;
    let mut opt_fields_index = (fidx > 0).then_some(fidx);

    while let Some(fields_index) = opt_fields_index {
        let fields_typedata = pdb_data
            .get_type_by_id(fields_index)
            .ok_or_else(|| format!("Enum: type with ID 0x{fidx:X} not found in the PDB file"))?;

        // check if the field list is actually a field list
        let TypeData::FieldList(field_list) = fields_typedata else {
            return Err(format!("Expected a field list, got: {fields_typedata:?}"));
        };

        for field in &field_list.fields {
            let TypeData::Enumerate(enumerator) = field else {
                return Err(format!(
                    "Expected an enumerator inside the field list, got: {field:?}"
                ));
            };
            let value = match enumerator.value {
                pdb2::Variant::U8(val_u8) => val_u8 as i64,
                pdb2::Variant::U16(val_u16) => val_u16 as i64,
                pdb2::Variant::U32(val_u32) => val_u32 as i64,
                pdb2::Variant::U64(val_u64) => val_u64 as i64,
                pdb2::Variant::I8(val_i8) => val_i8 as i64,
                pdb2::Variant::I16(val_i16) => val_i16 as i64,
                pdb2::Variant::I32(val_i32) => val_i32 as i64,
                pdb2::Variant::I64(val_i64) => val_i64,
            };
            enumerators.push((enumerator.name.to_string().into(), value));
        }

        // If the field list has a continuation, read the next one.
        // Continuations are used to keep the field list size under 64kB (about 2000 enumerators)
        opt_fields_index = field_list.continuation.map(|fidx| fidx.0);
    }
    let datatype = DbgDataType::Enum {
        size: ut.get_size(),
        signed,
        enumerators,
    };

    Ok((datatype, None))
}

fn read_pointer(
    pointer_type: &pdb2::PointerType,
    typereader_data: &mut TypeReaderData,
    pdb_data: &PdbData<'_>,
) -> Result<(DbgDataType, Option<String>), String> {
    let pt_typeindex = pointer_type.underlying_type.0;
    let address_size = pointer_type.attributes.size() as u64;
    let pt_name = if let Some(idx) = typereader_data
        .wip_items
        .iter()
        .position(|item| item.offset == pt_typeindex)
    {
        // this is a linked list or similar self-referential data structure, and one of the callers
        // of this function is already working to get this type
        // Trying to recursively decode this type would result in an infinite loop
        typereader_data.wip_items[idx].name.clone()
    } else {
        read_type(pt_typeindex, typereader_data, pdb_data)?;
        let pt_type = typereader_data.types.get(&(pt_typeindex as usize));
        pt_type.and_then(|typeinfo| typeinfo.name.clone())
    };
    let datatype = DbgDataType::Pointer(address_size, pt_typeindex as usize);
    Ok((datatype, pt_name))
}

fn read_modifier(
    modifier_type: &pdb2::ModifierType,
    typereader_data: &mut TypeReaderData,
    pdb_data: &PdbData<'_>,
) -> Result<(DbgDataType, Option<String>), String> {
    let type_index = modifier_type.underlying_type.0;
    read_type(type_index, typereader_data, pdb_data)?;
    let underlying_type = typereader_data
        .types
        .get(&(modifier_type.underlying_type.0 as usize))
        .ok_or_else(|| {
            format!("Modifier underlying type 0x{type_index:X} was not loaded correctly")
        })?;

    Ok((
        underlying_type.datatype.clone(),
        underlying_type.name.clone(),
    ))
}

fn read_array(
    array_type: &pdb2::ArrayType,
    typereader_data: &mut TypeReaderData,
    pdb_data: &PdbData<'_>,
) -> Result<(DbgDataType, Option<String>), String> {
    let element_type_id = array_type.element_type.0;
    read_type(element_type_id, typereader_data, pdb_data)?;
    let mut element_type = typereader_data
        .types
        .get(&(element_type_id as usize))
        .ok_or_else(|| {
            format!("Array element type 0x{element_type_id:X} was not loaded correctly")
        })?
        .clone();

    let stride = array_type.stride.map_or(element_type.get_size(), u64::from);
    // stride must be at least 1
    let stride = u64::max(stride, 1);

    // assumption: multi-dimensional arrays are created by nesting arrays in the element type
    // this matches the observed behavior PDB files created in MSVC
    if array_type.dimensions.len() > 1 {
        return Err(format!(
            "Unsupported encoding of a multi-dimenasional array: {array_type:?}, element type: {element_type:?}"
        ));
    }

    let size = array_type.dimensions[0] as u64;
    let mut array_dim = Vec::new();
    array_dim.push(size / stride);

    if let DbgDataType::Array { dim, arraytype, .. } = element_type.datatype {
        // the element type is already an array, so we need to merge the dimensions
        array_dim.extend(dim);
        element_type = *arraytype;
    }

    let datatype = DbgDataType::Array {
        size,
        dim: array_dim,
        stride,
        arraytype: Box::new(element_type.clone()),
    };

    Ok((datatype, element_type.name.clone()))
}

fn read_bitfield(
    bitfield_type: &pdb2::BitfieldType,
    typereader_data: &mut TypeReaderData,
    pdb_data: &PdbData<'_>,
) -> Result<(DbgDataType, Option<String>), String> {
    read_type(bitfield_type.underlying_type.0, typereader_data, pdb_data)?;
    let ut = typereader_data
        .types
        .get(&(bitfield_type.underlying_type.0 as usize))
        .unwrap();
    let datatype = DbgDataType::Bitfield {
        basetype: Box::new(ut.clone()),
        bit_offset: bitfield_type.position as u16,
        bit_size: bitfield_type.length as u16,
    };

    Ok((datatype, None))
}
