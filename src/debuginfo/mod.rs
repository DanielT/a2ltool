use indexmap::IndexMap;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::fmt::Display;

mod dwarf;
pub(crate) mod iter;
mod pdb;

#[derive(Debug)]
pub(crate) struct VarInfo {
    pub(crate) address: u64,
    pub(crate) typeref: usize,
    pub(crate) unit_idx: usize,
    pub(crate) function: Option<String>,
    pub(crate) namespaces: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct TypeInfo {
    pub(crate) name: Option<String>, // not all types have a name
    pub(crate) unit_idx: usize,
    pub(crate) datatype: DbgDataType,
    pub(crate) dbginfo_offset: usize,
}

#[derive(Debug, Clone)]
pub(crate) enum DbgDataType {
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
    Pointer(u64, usize),
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
        signed: bool,
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

#[derive(Debug)]
pub(crate) struct DebugData {
    pub(crate) variables: IndexMap<String, Vec<VarInfo>>,
    pub(crate) types: HashMap<usize, TypeInfo>,
    pub(crate) typenames: HashMap<String, Vec<usize>>,
    pub(crate) demangled_names: HashMap<String, String>,
    pub(crate) unit_names: Vec<Option<String>>,
    pub(crate) sections: HashMap<String, (u64, u64)>,
}

impl DebugData {
    // load the debug info from an elf file
    pub(crate) fn load_dwarf(filename: &OsStr, verbose: bool) -> Result<Self, String> {
        dwarf::load_dwarf(filename, verbose)
    }

    pub(crate) fn load_pdb(filename: &OsStr, verbose: bool) -> Result<Self, String> {
        pdb::load_pdb(filename, verbose)
    }

    pub(crate) fn iter(&self, use_new_arrays: bool) -> iter::VariablesIterator {
        iter::VariablesIterator::new(self, use_new_arrays)
    }
}

/// convert a full unit name, which might include a path, into a simple unit name
pub(crate) fn make_simple_unit_name(debug_data: &DebugData, unit_idx: usize) -> Option<String> {
    let full_name = debug_data.unit_names.get(unit_idx)?.as_deref()?;

    let file_name = if let Some(pos) = full_name.rfind('\\') {
        &full_name[(pos + 1)..]
    } else if let Some(pos) = full_name.rfind('/') {
        &full_name[(pos + 1)..]
    } else {
        full_name
    };

    Some(file_name.replace('.', "_"))
}

// pub(crate) fn demangle_cpp_varnames(input: &[&String]) -> HashMap<String, String> {
//     let mut demangled_symbols = HashMap::<String, String>::new();
//     let demangle_opts = cpp_demangle::DemangleOptions::new()
//         .no_params()
//         .no_return_type();
//     for varname in input {
//         // some really simple strings can be processed by the demangler, e.g "c" -> "const", which is wrong here.
//         // by only processing symbols that start with _Z (variables in classes/namespaces) this problem is avoided
//         if varname.starts_with("_Z") {
//             if let Ok(sym) = cpp_demangle::Symbol::new(*varname) {
//                 // exclude useless demangled names like "typeinfo for std::type_info" or "{vtable(std::type_info)}"
//                 if let Ok(demangled) = sym.demangle(&demangle_opts) {
//                     if !demangled.contains(' ') && !demangled.starts_with("{vtable") {
//                         demangled_symbols.insert(demangled, (*varname).clone());
//                     }
//                 }
//             }
//         }
//     }

//     demangled_symbols
// }

impl TypeInfo {
    const MAX_RECURSION_DEPTH: usize = 5;

    pub(crate) fn get_size(&self) -> u64 {
        match &self.datatype {
            DbgDataType::Uint8 => 1,
            DbgDataType::Uint16 => 2,
            DbgDataType::Uint32 => 4,
            DbgDataType::Uint64 => 8,
            DbgDataType::Sint8 => 1,
            DbgDataType::Sint16 => 2,
            DbgDataType::Sint32 => 4,
            DbgDataType::Sint64 => 8,
            DbgDataType::Float => 4,
            DbgDataType::Double => 8,
            DbgDataType::Bitfield { basetype, .. } => basetype.get_size(),
            DbgDataType::Pointer(size, _)
            | DbgDataType::Other(size)
            | DbgDataType::Struct { size, .. }
            | DbgDataType::Class { size, .. }
            | DbgDataType::Union { size, .. }
            | DbgDataType::Enum { size, .. }
            | DbgDataType::Array { size, .. }
            | DbgDataType::FuncPtr(size)
            | DbgDataType::TypeRef(_, size) => *size,
        }
    }

    pub(crate) fn get_members(&self) -> Option<&IndexMap<String, (TypeInfo, u64)>> {
        match &self.datatype {
            DbgDataType::Struct { members, .. }
            | DbgDataType::Class { members, .. }
            | DbgDataType::Union { members, .. } => Some(members),

            _ => None,
        }
    }

    pub(crate) fn get_pointer<'a>(
        &self,
        types: &'a HashMap<usize, TypeInfo>,
    ) -> Option<(u64, &'a TypeInfo)> {
        if let DbgDataType::Pointer(pt_size, pt_ref) = &self.datatype {
            let typeinfo = types.get(pt_ref)?;
            Some((*pt_size, typeinfo))
        } else {
            None
        }
    }

    pub(crate) fn get_arraytype(&self) -> Option<&TypeInfo> {
        if let DbgDataType::Array { arraytype, .. } = &self.datatype {
            Some(arraytype)
        } else {
            None
        }
    }

    pub(crate) fn get_reference<'a>(&'a self, types: &'a HashMap<usize, TypeInfo>) -> &'a Self {
        if let DbgDataType::TypeRef(dbginfo_offset, _) = &self.datatype {
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
                    (DbgDataType::Uint8, DbgDataType::Uint8)
                    | (DbgDataType::Uint16, DbgDataType::Uint16)
                    | (DbgDataType::Uint32, DbgDataType::Uint32)
                    | (DbgDataType::Uint64, DbgDataType::Uint64)
                    | (DbgDataType::Sint8, DbgDataType::Sint8)
                    | (DbgDataType::Sint16, DbgDataType::Sint16)
                    | (DbgDataType::Sint32, DbgDataType::Sint32)
                    | (DbgDataType::Sint64, DbgDataType::Sint64)
                    | (DbgDataType::Float, DbgDataType::Float)
                    | (DbgDataType::Double, DbgDataType::Double) => true,
                    (
                        DbgDataType::Enum {
                            size,
                            signed,
                            enumerators,
                        },
                        DbgDataType::Enum {
                            size: size2,
                            signed: signed2,
                            enumerators: enumerators2,
                        },
                    ) => size == size2 && signed == signed2 && enumerators == enumerators2,
                    (
                        DbgDataType::Array {
                            size,
                            dim,
                            stride,
                            arraytype,
                        },
                        DbgDataType::Array {
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
                        DbgDataType::Pointer(size1, dest_offset1),
                        DbgDataType::Pointer(size2, dest_offset2),
                    ) => {
                        size1 == size2
                            && if dest_offset1 == dest_offset2 {
                                true
                            } else if let (Some(dest_type1), Some(dest_type2)) =
                                (types.get(dest_offset1), types.get(dest_offset2))
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
                    (DbgDataType::Other(size1), DbgDataType::Other(size2)) => size1 == size2,
                    (
                        DbgDataType::Bitfield {
                            basetype,
                            bit_offset,
                            bit_size,
                        },
                        DbgDataType::Bitfield {
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
                        DbgDataType::Struct { size, members },
                        DbgDataType::Struct {
                            size: size2,
                            members: members2,
                        },
                    ) => size == size2 && Self::compare_members(members, members2, types, depth),
                    (
                        DbgDataType::Union { size, members },
                        DbgDataType::Union {
                            size: size2,
                            members: members2,
                        },
                    ) => size == size2 && Self::compare_members(members, members2, types, depth),
                    (
                        DbgDataType::Class {
                            size,
                            members,
                            inheritance,
                        },
                        DbgDataType::Class {
                            size: size2,
                            members: members2,
                            inheritance: inheritance2,
                        },
                    ) => {
                        size == size2
                            && Self::compare_members(members, members2, types, depth)
                            && Self::compare_members(inheritance, inheritance2, types, depth)
                    }
                    (DbgDataType::FuncPtr(size1), DbgDataType::FuncPtr(size2)) => size1 == size2,
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
            DbgDataType::Uint8 => f.write_str("Uint8"),
            DbgDataType::Uint16 => f.write_str("Uint16"),
            DbgDataType::Uint32 => f.write_str("Uint32"),
            DbgDataType::Uint64 => f.write_str("Uint64"),
            DbgDataType::Sint8 => f.write_str("Sint8"),
            DbgDataType::Sint16 => f.write_str("Sint16"),
            DbgDataType::Sint32 => f.write_str("Sint32"),
            DbgDataType::Sint64 => f.write_str("Sint64"),
            DbgDataType::Float => f.write_str("Float"),
            DbgDataType::Double => f.write_str("Double"),
            DbgDataType::Bitfield { .. } => f.write_str("Bitfield"),
            DbgDataType::Pointer(_, _) => write!(f, "Pointer(...)"),
            DbgDataType::Other(osize) => write!(f, "Other({osize})"),
            DbgDataType::FuncPtr(osize) => write!(f, "function pointer({osize})"),
            DbgDataType::Struct { members, .. } => {
                if let Some(name) = &self.name {
                    write!(f, "Struct {name}({} members)", members.len())
                } else {
                    write!(f, "Struct <anonymous>({} members)", members.len())
                }
            }
            DbgDataType::Class { members, .. } => {
                if let Some(name) = &self.name {
                    write!(f, "Class {name}({} members)", members.len())
                } else {
                    write!(f, "Class <anonymous>({} members)", members.len())
                }
            }
            DbgDataType::Union { members, .. } => {
                if let Some(name) = &self.name {
                    write!(f, "Union {name}({} members)", members.len())
                } else {
                    write!(f, "Union <anonymous>({} members)", members.len())
                }
            }
            DbgDataType::Enum { enumerators, .. } => {
                if let Some(name) = &self.name {
                    write!(f, "Enum {name}({} enumerators)", enumerators.len())
                } else {
                    write!(f, "Enum <anonymous>({} enumerators)", enumerators.len())
                }
            }
            DbgDataType::Array { dim, arraytype, .. } => {
                write!(f, "Array({dim:?} x {arraytype})")
            }
            DbgDataType::TypeRef(t_ref, _) => write!(f, "TypeRef({t_ref})"),
        }
    }
}

#[cfg(test)]
mod test {}
