use super::DebugData;
use super::TypeInfo;
use super::VarInfo;
use std::fmt::Write;

#[cfg(test)]
use std::collections::HashMap;

pub(crate) enum TypeInfoIter<'a> {
    NotIterable,
    StructLike {
        struct_iter: std::collections::hash_map::Iter<'a, String, (TypeInfo, u64)>,
        current_member: Option<(&'a String, &'a (TypeInfo, u64))>,
        member_iter: Option<Box<TypeInfoIter<'a>>>,
    },
    Array {
        size: u64,
        dim: &'a Vec<u64>,
        stride: u64,
        position: u64,
        arraytype: &'a TypeInfo,
        item_iter: Option<Box<TypeInfoIter<'a>>>,
    },
}

pub(crate) struct VariablesIterator<'a> {
    debugdata: &'a DebugData,
    var_iter: std::collections::hash_map::Iter<'a, String, VarInfo>,
    current_var: Option<(&'a String, &'a VarInfo)>,
    type_iter: Option<TypeInfoIter<'a>>,
}

impl<'a> TypeInfoIter<'a> {
    pub(crate) fn new(typeinfo: &'a TypeInfo) -> Self {
        use TypeInfo::{Array, Class, Struct, Union};
        match typeinfo {
            Class { members, .. } | Union { members, .. } | Struct { members, .. } => {
                let mut struct_iter = members.iter();
                let currentmember = struct_iter.next();
                if currentmember.is_some() {
                    TypeInfoIter::StructLike {
                        struct_iter,
                        current_member: currentmember,
                        member_iter: None,
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
                size: *size,
                dim,
                stride: *stride,
                arraytype,
                position: 0,
                item_iter: None,
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
                struct_iter,
                current_member,
                member_iter,
            } => {
                // current_member will be Some(...) while the iteration is still in progress
                if let Some((name, (typeinfo, offset))) = current_member {
                    if member_iter.is_none() {
                        *member_iter = Some(Box::new(TypeInfoIter::new(typeinfo)));
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
                size,
                dim,
                stride,
                position,
                arraytype,
                item_iter,
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
                            let _ = write!(output, "._{val}_");
                            output
                        });

                    // calculate the storage offset of this array element. Each element is stride bytes wide.
                    let offset = *stride * (*position);

                    // each array element might be a struct, in this case iterate over the
                    // individual elements of that before advancing to the next array element
                    if item_iter.is_none() {
                        // first, return the array element directly
                        *item_iter = Some(Box::new(TypeInfoIter::new(arraytype)));
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
    pub(crate) fn new(debugdata: &'a DebugData) -> Self {
        let mut var_iter = debugdata.variables.iter();
        // current_var == None signals the end of iteration, so it needs to be set to the first value here
        let current_var = var_iter.next();
        VariablesIterator {
            debugdata,
            var_iter,
            current_var,
            type_iter: None,
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
                    self.type_iter = Some(TypeInfoIter::new(ti));
                } else {
                    self.type_iter = None;
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

#[test]
fn test_typeinfo_iter() {
    // basic types, e.g. Sint<x> and Uint<x> cannot be iterated over
    // a TypeInfoIter for these immediately returns None
    let typeinfo = TypeInfo::Sint16;
    let mut iter = TypeInfoIter::new(&typeinfo);
    let result = iter.next();
    assert!(result.is_none());

    // a struct iterates over all of its members
    let mut structmembers_a: HashMap<String, (TypeInfo, u64)> = HashMap::new();
    structmembers_a.insert("structmember_1".to_string(), (TypeInfo::Uint64, 0));
    structmembers_a.insert("structmember_2".to_string(), (TypeInfo::Uint64, 0));
    structmembers_a.insert("structmember_3".to_string(), (TypeInfo::Uint64, 0));
    structmembers_a.insert("structmember_4".to_string(), (TypeInfo::Uint64, 0));
    structmembers_a.insert("structmember_5".to_string(), (TypeInfo::Uint64, 0));
    let typeinfo_inner_1 = TypeInfo::Struct {
        size: 64,
        members: structmembers_a,
    };
    let mut structmembers_b: HashMap<String, (TypeInfo, u64)> = HashMap::new();
    structmembers_b.insert("foobar_1".to_string(), (TypeInfo::Sint8, 0));
    structmembers_b.insert("foobar_2".to_string(), (TypeInfo::Sint8, 0));
    structmembers_b.insert("foobar_3".to_string(), (TypeInfo::Sint8, 0));
    let typeinfo_inner_2 = TypeInfo::Struct {
        size: 64,
        members: structmembers_b,
    };
    let mut structmembers: HashMap<String, (TypeInfo, u64)> = HashMap::new();
    structmembers.insert("inner_a".to_string(), (typeinfo_inner_1, 0));
    structmembers.insert("inner_b".to_string(), (typeinfo_inner_2, 0));
    let typeinfo = TypeInfo::Struct {
        size: 64,
        members: structmembers,
    };
    // for (displaystring, element_type, offset) in TypeInfoIter::new(&typeinfo) {
    //     println!("name: {}, \toffset: {}", displaystring, offset);
    // }
    let iter = TypeInfoIter::new(&typeinfo);
    assert_eq!(iter.count(), 10);
}

#[test]
fn test_varinfo_iter() {
    let mut variables = HashMap::<String, VarInfo>::new();
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
    let mut types = HashMap::<usize, TypeInfo>::new();
    let mut structmembers: HashMap<String, (TypeInfo, u64)> = HashMap::new();
    structmembers.insert("member_1".to_string(), (TypeInfo::Uint8, 0));
    structmembers.insert("member_2".to_string(), (TypeInfo::Uint8, 1));
    let structtype = TypeInfo::Struct {
        size: 64,
        members: structmembers,
    };
    types.insert(0, TypeInfo::Uint8);
    types.insert(1, structtype);
    let demangled_names = HashMap::new();
    let debugdata = DebugData {
        variables,
        types,
        demangled_names,
    };

    let iter = VariablesIterator::new(&debugdata);
    for item in iter {
        println!("{}", item.0);
    }
    assert_eq!(VariablesIterator::new(&debugdata).count(), 5);
}
