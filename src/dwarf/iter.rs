use crate::dwarf::{DebugData, DwarfDataType, TypeInfo, VarInfo};
use std::collections::HashMap;
use std::fmt::Write;

pub(crate) struct TypeInfoIter<'dbg> {
    types: &'dbg HashMap<usize, TypeInfo>,
    type_stack: Vec<&'dbg TypeInfo>,
    position_stack: Vec<usize>,
    offset_stack: Vec<u64>,
    name_stack: Vec<String>,
    use_new_arrays: bool,
}

pub(crate) struct VariablesIterator<'dbg> {
    debugdata: &'dbg DebugData,
    var_iter: indexmap::map::Iter<'dbg, String, VarInfo>,
    current_var: Option<(&'dbg String, &'dbg VarInfo)>,
    type_iter: Option<TypeInfoIter<'dbg>>,
    use_new_arrays: bool,
}

impl<'dbg> Iterator for TypeInfoIter<'dbg> {
    type Item = (String, &'dbg TypeInfo, u64);

    fn next(&mut self) -> Option<Self::Item> {
        let mut result = self.next_core();
        while result.is_none() && !self.type_stack.is_empty() {
            self.up();
            result = self.next_core();
        }
        result
    }
}

impl<'dbg> TypeInfoIter<'dbg> {
    pub(crate) fn new(
        types: &'dbg HashMap<usize, TypeInfo>,
        typeinfo: &'dbg TypeInfo,
        use_new_arrays: bool,
    ) -> Self {
        Self {
            types,
            type_stack: vec![typeinfo],
            position_stack: vec![0],
            offset_stack: vec![0],
            name_stack: vec!["".to_string()],
            use_new_arrays,
        }
    }

    fn next_core(&mut self) -> Option<(String, &'dbg TypeInfo, u64)> {
        match &self.type_stack.last()?.datatype {
            DwarfDataType::Class { members, .. }
            | DwarfDataType::Struct { members, .. }
            | DwarfDataType::Union { members, .. } => {
                let depth = self.type_stack.len() - 1;
                let position = self.position_stack[depth];
                let base = self.offset_stack[depth];
                let prev_name = &self.name_stack[depth];
                let (member_name, (member_typeinfo, member_offset)) =
                    members.get_index(position)?;
                let member_typeinfo = member_typeinfo.get_reference(self.types);
                let complete_offset = base + member_offset;
                let fullname = format!("{prev_name}.{member_name}");

                // advance to next member
                self.position_stack[depth] += 1;

                // prepare to return the children of the current member
                self.type_stack.push(member_typeinfo);
                self.position_stack.push(0);
                self.offset_stack.push(complete_offset);
                self.name_stack.push(fullname.clone());

                Some((fullname, member_typeinfo, complete_offset))
            }
            DwarfDataType::Array {
                size,
                dim,
                stride,
                arraytype,
            } => {
                let total_elemcount = size / stride;
                let depth = self.type_stack.len() - 1;
                let position = self.position_stack[depth] as u64;
                let prev_name = &self.name_stack[depth];
                let base = self.offset_stack[depth];

                if total_elemcount > position {
                    // in a multi-dimensional array, e.g. [5][10], position goes from 0 to 50
                    // it needs to be decomposed into individual array indices
                    let mut current_indices = vec![0; dim.len()];
                    let mut rem = position;

                    // going backward over the list of array dimensions, divide and keep the remainder
                    for idx in (0..dim.len()).rev() {
                        current_indices[idx] = rem % dim[idx];
                        rem /= dim[idx];
                    }
                    let idxstr =
                        current_indices
                            .iter()
                            .fold(prev_name.clone(), |mut output, val| {
                                if self.use_new_arrays {
                                    let _ = write!(output, "[{val}]");
                                } else {
                                    let _ = write!(output, "._{val}_");
                                }
                                output
                            });

                    // calculate the storage offset of this array element. Each element is stride bytes wide.
                    let complete_offset = base + (*stride * position);

                    // advance to next member
                    self.position_stack[depth] += 1;

                    // prepare to return the children of the current member
                    self.type_stack.push(arraytype);
                    self.position_stack.push(0);
                    self.offset_stack.push(complete_offset);
                    self.name_stack.push(idxstr.clone());

                    Some((idxstr, arraytype, complete_offset))
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn up(&mut self) {
        self.type_stack.pop();
        self.position_stack.pop();
        self.name_stack.pop();
        self.offset_stack.pop();
    }

    // pub(crate) fn next_sibling(&mut self) -> Option<(String, &'dbg TypeInfo, u64)> {
    //     self.up();
    //     self.next()
    // }
}

impl<'dbg> Iterator for VariablesIterator<'dbg> {
    type Item = (String, Option<&'dbg TypeInfo>, u64);

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

impl<'dbg> VariablesIterator<'dbg> {
    pub(crate) fn new(debugdata: &'dbg DebugData, use_new_arrays: bool) -> Self {
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

    pub(crate) fn next_sibling(&mut self) -> Option<(String, Option<&'dbg TypeInfo>, u64)> {
        if let Some(type_iter) = &mut self.type_iter {
            type_iter.up();
        }

        self.next()
    }
}

//########################################################

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
