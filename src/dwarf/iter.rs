use crate::dwarf::{DebugData, DwarfDataType, TypeInfo, VarInfo};
use std::collections::HashMap;
use std::fmt::Write;

pub(crate) enum TypeInfoIter<'a> {
    NotIterable,
    StructLike {
        types: &'a HashMap<usize, TypeInfo>,
        struct_iter: indexmap::map::Iter<'a, String, (TypeInfo, u64)>,
        current_member: Option<(&'a String, &'a (TypeInfo, u64))>,
        member_iter: Option<Box<TypeInfoIter<'a>>>,
        use_new_arrays: bool,
    },
    Array {
        types: &'a HashMap<usize, TypeInfo>,
        size: u64,
        dim: &'a Vec<u64>,
        stride: u64,
        position: u64,
        arraytype: &'a TypeInfo,
        item_iter: Option<Box<TypeInfoIter<'a>>>,
        use_new_arrays: bool,
    },
}

pub(crate) struct VariablesIterator<'a> {
    debugdata: &'a DebugData,
    var_iter: indexmap::map::Iter<'a, String, VarInfo>,
    current_var: Option<(&'a String, &'a VarInfo)>,
    type_iter: Option<TypeInfoIter<'a>>,
    use_new_arrays: bool,
}

impl<'a> TypeInfoIter<'a> {
    pub(crate) fn new(
        types: &'a HashMap<usize, TypeInfo>,
        typeinfo: &'a TypeInfo,
        use_new_arrays: bool,
    ) -> Self {
        use DwarfDataType::{Array, Class, Struct, Union};
        match &typeinfo.datatype {
            Class { members, .. } | Union { members, .. } | Struct { members, .. } => {
                let mut struct_iter = members.iter();
                let currentmember = struct_iter.next();
                if currentmember.is_some() {
                    TypeInfoIter::StructLike {
                        types,
                        struct_iter,
                        current_member: currentmember,
                        member_iter: None,
                        use_new_arrays,
                    }
                } else {
                    TypeInfoIter::NotIterable
                }
            }
            Array {
                ref size,
                dim,
                ref stride,
                arraytype,
            } => TypeInfoIter::Array {
                types,
                size: *size,
                dim,
                stride: *stride,
                arraytype,
                position: 0,
                item_iter: None,
                use_new_arrays,
            },
            _ => TypeInfoIter::NotIterable,
        }
    }
}

impl<'a> Iterator for TypeInfoIter<'a> {
    type Item = (String, &'a TypeInfo, u64);

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            TypeInfoIter::NotIterable => None,

            TypeInfoIter::StructLike {
                types,
                struct_iter,
                current_member,
                member_iter,
                use_new_arrays,
            } => {
                // current_member will be Some(...) while the iteration is still in progress
                if let Some((name, (member_typeinfo, offset))) = current_member {
                    if member_iter.is_none() {
                        let typeinfo = if let DwarfDataType::TypeRef(typeinfo_offset, _) =
                            &member_typeinfo.datatype
                        {
                            types.get(typeinfo_offset).unwrap_or(&TypeInfo {
                                name: None,
                                unit_idx: usize::MAX,
                                datatype: DwarfDataType::Uint8,
                                dbginfo_offset: 0,
                            })
                        } else {
                            member_typeinfo
                        };
                        *member_iter = Some(Box::new(TypeInfoIter::new(
                            types,
                            typeinfo,
                            *use_new_arrays,
                        )));
                        Some((format!(".{name}"), typeinfo, *offset))
                    } else {
                        let member = member_iter.as_deref_mut().unwrap().next();
                        if let Some((member_name, member_typeinfo, member_offset)) = member {
                            Some((
                                format!(".{name}{member_name}"),
                                member_typeinfo,
                                offset + member_offset,
                            ))
                        } else {
                            // this struct member can't iterate or has finished iterating, move to the next struct member
                            *current_member = struct_iter.next();
                            *member_iter = None;
                            self.next()
                        }
                    }
                } else {
                    None
                }
            }

            TypeInfoIter::Array {
                types,
                size,
                dim,
                stride,
                position,
                arraytype,
                item_iter,
                use_new_arrays,
            } => {
                let total_elemcount = *size / *stride;
                // are there more elements to iterate over
                if *position < total_elemcount {
                    // in a multi-dimensional array, e.g. [5][10], position goes from 0 to 50
                    // it needs to be decomposed into individual array indices
                    let mut current_indices = vec![0; dim.len()];
                    let mut rem = *position;

                    // going backward over the list of array dimensions, divide and keep the remainder
                    for idx in (0..dim.len()).rev() {
                        current_indices[idx] = rem % dim[idx];
                        rem /= dim[idx];
                    }
                    let idxstr = current_indices
                        .iter()
                        .fold(String::new(), |mut output, val| {
                            if *use_new_arrays {
                                let _ = write!(output, "[{val}]");
                            } else {
                                let _ = write!(output, "._{val}_");
                            }
                            output
                        });

                    // calculate the storage offset of this array element. Each element is stride bytes wide.
                    let offset = *stride * (*position);

                    // each array element might be a struct, in this case iterate over the
                    // individual elements of that before advancing to the next array element
                    if item_iter.is_none() {
                        // first, return the array element directly
                        *item_iter = Some(Box::new(TypeInfoIter::new(
                            types,
                            arraytype,
                            *use_new_arrays,
                        )));
                        Some((idxstr, arraytype, offset))
                    } else {
                        // then try to return struct elements
                        let item = item_iter.as_deref_mut().unwrap().next();
                        if let Some((item_name, item_typeinfo, item_offset)) = item {
                            Some((
                                format!("{idxstr}{item_name}"),
                                item_typeinfo,
                                offset + item_offset,
                            ))
                        } else {
                            // no (more) struct elements to return, advance to the next array element
                            *position += 1;
                            *item_iter = None;
                            self.next()
                        }
                    }
                } else {
                    // reached the end of the array
                    None
                }
            }
        }
    }
}

impl<'a> VariablesIterator<'a> {
    pub(crate) fn new(debugdata: &'a DebugData, use_new_arrays: bool) -> Self {
        let mut var_iter = debugdata.variables.iter();
        // current_var == None signals the end of iteration, so it needs to be set to the first value here
        let current_var = var_iter.next();
        VariablesIterator {
            debugdata,
            var_iter,
            current_var,
            type_iter: None,
            use_new_arrays,
        }
    }
}

impl<'a> Iterator for VariablesIterator<'a> {
    type Item = (String, Option<&'a TypeInfo>, u64);

    fn next(&mut self) -> Option<Self::Item> {
        if let Some((
            varname,
            VarInfo {
                address, typeref, ..
            },
        )) = self.current_var
        {
            if self.type_iter.is_none() {
                // newly set current_var, should be returned before using type_iter to return its sub-elements
                let typeinfo = self.debugdata.types.get(typeref);
                if let Some(ti) = typeinfo {
                    self.type_iter = Some(TypeInfoIter::new(
                        &self.debugdata.types,
                        ti,
                        self.use_new_arrays,
                    ));
                } else {
                    self.type_iter = None;
                    self.current_var = self.var_iter.next();
                }
                Some((varname.to_string(), typeinfo, *address))
            } else {
                // currently iterating over sub-elements described by the type_iter
                if let Some((type_name, type_info, offset)) =
                    self.type_iter.as_mut().unwrap().next()
                {
                    Some((
                        format!("{varname}{type_name}"),
                        Some(type_info),
                        *address + offset,
                    ))
                } else {
                    // reached the end of this type_iter, try to advance var_iter to get a new current_var
                    self.current_var = self.var_iter.next();
                    self.type_iter = None;
                    self.next()
                }
            }
        } else {
            // current_var is None -> reached the end
            None
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use indexmap::IndexMap;

    const DEFAULT_TYPEINFO: TypeInfo = TypeInfo {
        name: None,
        unit_idx: usize::MAX,
        datatype: DwarfDataType::Sint16,
        dbginfo_offset: 0,
    };

    #[test]
    fn test_typeinfo_iter() {
        // basic types, e.g. Sint<x> and Uint<x> cannot be iterated over
        // a TypeInfoIter for these immediately returns None
        let typeinfo = TypeInfo {
            datatype: DwarfDataType::Sint16,
            ..DEFAULT_TYPEINFO.clone()
        };
        let types = HashMap::new();
        let mut iter = TypeInfoIter::new(&types, &typeinfo, false);
        let result = iter.next();
        assert!(result.is_none());

        // a struct iterates over all of its members
        let mut types = HashMap::new();
        let t_uint64 = TypeInfo {
            datatype: DwarfDataType::Uint64,
            ..DEFAULT_TYPEINFO.clone()
        };
        let t_sint8 = TypeInfo {
            datatype: DwarfDataType::Uint64,
            ..DEFAULT_TYPEINFO.clone()
        };
        let mut structmembers_a: IndexMap<String, (TypeInfo, u64)> = IndexMap::new();
        structmembers_a.insert("structmember_1".to_string(), (t_uint64.clone(), 0));
        structmembers_a.insert("structmember_2".to_string(), (t_uint64.clone(), 0));
        structmembers_a.insert("structmember_3".to_string(), (t_uint64.clone(), 0));
        structmembers_a.insert("structmember_4".to_string(), (t_uint64.clone(), 0));
        structmembers_a.insert("structmember_5".to_string(), (t_uint64.clone(), 0));
        let typeinfo_inner_1 = TypeInfo {
            datatype: DwarfDataType::Struct {
                size: 64,
                members: structmembers_a,
            },
            ..DEFAULT_TYPEINFO.clone()
        };
        let mut structmembers_b: IndexMap<String, (TypeInfo, u64)> = IndexMap::new();
        structmembers_b.insert("foobar_1".to_string(), (t_sint8.clone(), 0));
        structmembers_b.insert("foobar_2".to_string(), (t_sint8.clone(), 0));
        structmembers_b.insert("foobar_3".to_string(), (t_sint8.clone(), 0));
        let typeinfo_inner_2 = TypeInfo {
            datatype: DwarfDataType::Struct {
                size: 64,
                members: structmembers_b,
            },
            ..DEFAULT_TYPEINFO.clone()
        };
        types.insert(100, typeinfo_inner_1);
        types.insert(101, typeinfo_inner_2);
        let typeref_inner_1 = TypeInfo {
            datatype: DwarfDataType::TypeRef(100, 0),
            ..DEFAULT_TYPEINFO.clone()
        };
        let typeref_inner_2 = TypeInfo {
            datatype: DwarfDataType::TypeRef(101, 0),
            ..DEFAULT_TYPEINFO.clone()
        };
        let mut structmembers: IndexMap<String, (TypeInfo, u64)> = IndexMap::new();
        structmembers.insert("inner_a".to_string(), (typeref_inner_1, 0));
        structmembers.insert("inner_b".to_string(), (typeref_inner_2, 0));
        let typeinfo = TypeInfo {
            datatype: DwarfDataType::Struct {
                size: 64,
                members: structmembers,
            },
            ..DEFAULT_TYPEINFO.clone()
        };
        let iter = TypeInfoIter::new(&types, &typeinfo, false);
        assert_eq!(iter.count(), 10);
    }

    #[test]
    fn test_varinfo_iter() {
        let mut variables = IndexMap::<String, VarInfo>::new();
        variables.insert(
            "var_a".to_string(),
            VarInfo {
                address: 1,
                typeref: 0,
            },
        );
        variables.insert(
            "var_b".to_string(),
            VarInfo {
                address: 2,
                typeref: 0,
            },
        );
        variables.insert(
            "var_c".to_string(),
            VarInfo {
                address: 3,
                typeref: 1,
            },
        );
        variables.insert(
            "var_d_wo_type_info".to_string(),
            VarInfo {
                address: 4,
                typeref: 404, // some number with no correspondence in the types hash map
            },
        );

        let mut types = HashMap::<usize, TypeInfo>::new();
        let t_uint8 = TypeInfo {
            datatype: DwarfDataType::Uint8,
            ..DEFAULT_TYPEINFO.clone()
        };
        let mut structmembers: IndexMap<String, (TypeInfo, u64)> = IndexMap::new();
        structmembers.insert("member_1".to_string(), (t_uint8.clone(), 0));
        structmembers.insert("member_2".to_string(), (t_uint8.clone(), 1));
        let structtype = TypeInfo {
            datatype: DwarfDataType::Struct {
                size: 64,
                members: structmembers,
            },
            ..DEFAULT_TYPEINFO.clone()
        };
        types.insert(1, structtype);
        let demangled_names = HashMap::new();
        let debugdata = DebugData {
            variables,
            types,
            typenames: HashMap::new(),
            demangled_names,
            unit_names: Vec::new(),
        };

        let iter = VariablesIterator::new(&debugdata, false);
        for item in iter {
            println!("{}", item.0);
        }
        assert_eq!(VariablesIterator::new(&debugdata, false).count(), 6);
    }
}
