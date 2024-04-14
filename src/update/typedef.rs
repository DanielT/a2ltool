use crate::dwarf::{DebugData, DwarfDataType, TypeInfo};
use crate::update::{
    adjust_limits, get_a2l_datatype, get_fnc_values_memberid, get_inner_type, set_address_type,
    set_bitmask, set_matrix_dim, update_characteristic_axis, update_record_layout,
    RecordLayoutInfo, TypedefNames, TypedefReferrer, TypedefsRefInfo,
};
use a2lfile::{
    A2lObject, AddrType, CharacteristicType, FncValues, IndexMode, Module, Number, RecordLayout,
    StructureComponent, SymbolTypeLink, TypedefBlob, TypedefCharacteristic, TypedefMeasurement,
    TypedefStructure,
};
use fxhash::FxBuildHasher;
use indexmap::{IndexMap, IndexSet};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::fmt::Write;

use crate::update::enums::{cond_create_enum_conversion, update_enum_compu_methods};

type FxIndexMap<K, V> = IndexMap<K, V, FxBuildHasher>;

/// `TypeQuality` is used to identify how precise the information in `typedef_map` is.
///
#[derive(Clone, PartialEq)]
enum TypeQuality {
    /// Exact: The Type is fully known using the cross reference from an INSTANCE
    ///      or STRUCTURE_COMPONENT
    Exact,
    /// SymTypeLinkOnly: The SYMBOL_TYPE_LINK was resolved, but this doesn't tell
    ///      us if it's a struct or a pointer to a struct.
    SymTypeLinkOnly,
}

struct TypedefUpdater<'dbg, 'a2l, 'rl, 'log> {
    // --- provided input ---
    /// the a2l module that is processed by the TypedefUpdater
    module: &'a2l mut Module,
    /// DWARF2+ debug data that serves as the information source for the update
    debug_data: &'dbg DebugData,
    /// names of all TYPEDEF_* items that were collected during a previous update phase
    typedef_names: TypedefNames,
    /// information about RECORD_LAYOUT items that exist in the module
    recordlayout_info: &'rl mut RecordLayoutInfo,
    /// information about references from INSTANCEs to TYPEDEF_*
    typedef_ref_info: TypedefsRefInfo<'dbg>,
    /// array of strings used to output log messages
    log_msgs: &'log mut Vec<String>,

    // --- computed data ---
    /// all TYPEDEF_STRUCTURES, extracted from the module during the update for access by name
    typedef_structs: FxIndexMap<String, TypedefStructure>,
    /// is_calib_struct indicates for each TYPEDEF_STRUCTURE whether it contains calibration or measurement items
    is_calib_struct: HashMap<String, bool>,
    /// mapping: debug typeinfo -> Set of TYPEDEFs using that typeinfo
    /// multiple TYPEDEFs might use the same type - e.g. the type "float32" might be used by a
    /// TYPEDEF_MEASUREMENT and a TYPEDEF_CHARACTERISTIC
    type_map: FxIndexMap<usize, IndexSet<String>>,
    /// mapping: TYPEDEF name to (data type, type quality)
    /// during the start of the update, some type information might be present but imprecise
    typedef_map: FxIndexMap<String, (&'dbg TypeInfo, TypeQuality)>,
    /// TYPEDEF_STRUCTURES that aren't referenced, have bad type information and can't be updated
    preserved_structs: FxIndexMap<String, TypedefStructure>,
    /// AXIS_PTS information. It is derived from the module and used while creating or
    /// updating TYPEDEF_CHARACTERISTICs
    axis_pts_dim: HashMap<String, u16>,
}

pub(crate) const FLAG_CREATE_CALIB: &str = "||calib||";
pub(crate) const FLAG_CREATE_MEAS: &str = "||meas||";

pub(crate) fn update_module_typedefs<'a>(
    module: &mut Module,
    debug_data: &'a DebugData,
    log_msgs: &mut Vec<String>,
    preserve_unknown: bool,
    typedef_ref_info: TypedefsRefInfo<'a>,
    typedef_names: TypedefNames,
    recordlayout_info: &mut RecordLayoutInfo,
) {
    let updater = TypedefUpdater::new(
        module,
        debug_data,
        log_msgs,
        typedef_names,
        recordlayout_info,
        typedef_ref_info,
    );

    updater.process_typedefs(preserve_unknown, false);
}

pub(crate) fn create_new_typedefs<'a>(
    module: &mut Module,
    debug_data: &'a DebugData,
    log_msgs: &mut Vec<String>,
    create_list: &[(&'a TypeInfo, usize)],
) {
    let typedef_names = TypedefNames::new(module);
    let mut recordlayout_info = RecordLayoutInfo::build(module);
    let mut typedef_ref_info: TypedefsRefInfo = HashMap::new();

    for (typeinfo, instance_idx) in create_list {
        let name = module.instance[*instance_idx].name.clone();
        typedef_ref_info
            .entry(name)
            .or_default()
            .push((Some(typeinfo), TypedefReferrer::Instance(*instance_idx)));
    }

    let updater = TypedefUpdater::new(
        module,
        debug_data,
        log_msgs,
        typedef_names,
        &mut recordlayout_info,
        typedef_ref_info,
    );

    updater.process_typedefs(true, true);
}

impl<'dbg, 'a2l, 'rl, 'log> TypedefUpdater<'dbg, 'a2l, 'rl, 'log> {
    /// create a new `TypedefUpdater`
    pub(crate) fn new(
        module: &'a2l mut Module,
        debug_data: &'dbg DebugData,
        log_msgs: &'log mut Vec<String>,
        typedef_names: TypedefNames,
        recordlayout_info: &'rl mut RecordLayoutInfo,
        typedef_ref_info: TypedefsRefInfo<'dbg>,
    ) -> Self {
        let axis_pts_dim: HashMap<String, u16> = module
            .axis_pts
            .iter()
            .map(|item| (item.name.clone(), item.max_axis_points))
            .collect();

        Self {
            is_calib_struct: HashMap::with_capacity(module.typedef_structure.len()),
            type_map: FxIndexMap::default(),
            typedef_map: FxIndexMap::default(),
            typedef_structs: FxIndexMap::<String, TypedefStructure>::default(),
            module,
            debug_data,
            log_msgs,
            typedef_names,
            recordlayout_info,
            typedef_ref_info,
            preserved_structs: FxIndexMap::default(),
            axis_pts_dim,
        }
    }

    /// process the TYPEDEFs in self.module
    fn process_typedefs(mut self, preserve_unknown: bool, create_only: bool) {
        self.typedef_names.structure = HashSet::new();

        self.calc_structure_category();
        self.build_structure_hash();
        self.process_structure_components(create_only);
        self.create_missing_instance_targets();

        if !create_only {
            self.update_all_typedef_axis();
            self.update_all_typedef_blob();
            self.update_all_typedef_characteristic();
            self.update_all_typedef_measurement();
            self.update_all_typedef_structure();

            if !preserve_unknown {
                self.cleanup_unused_typedefs();
            }
        }

        // store the TPEDEF_STRUCTUREs in the module again
        for (_, td_struct) in self.typedef_structs {
            self.module.typedef_structure.push(td_struct);
        }
        if preserve_unknown {
            for (_, td_struct) in self.preserved_structs {
                self.module.typedef_structure.push(td_struct);
            }
        }
    }

    /// separate the `TYPEDEF_STRUCTUREs` into two groups - one references only
    /// `TYPEDEF_MEASUREMENTS`, the other only references `TYPEDEF_AXIS/BLOB/CHARACTERISTIC`
    fn calc_structure_category(&mut self) {
        assert!(self.typedef_names.structure.is_empty());
        let mut updated = true;
        let mut warn_set = IndexSet::new();
        // In the graph of TYPEDEF_STRUCTUREs we want to find all chains that ultimately end in references to
        // TYPEDEF_AXIS/BLOB/CHARACTERISTIC or TYPEDEF_MEASUREMENTs.
        // A correct A2L file should not mix measurement with other types (axis, blob, characteristic).
        //
        // The graph of TYPEDEF_STRUCTUREs can have several cases that are quite ugly:
        // - Chains that go nowhere:
        //   INSTANCE -> TYPEDEF_STRUCTURE (without STRUCTURE_COMPONENTs)
        // - Circular references:
        //   INSTANCE -> TYPEDEF_STRUCTURE [name = "x"] -> TYPEDEF_STRUCTURE [name = "x"] -> ...
        //   This could happen if an INSTANCE is created for a linked list: the linked list
        //   struct in C code contains a pointer "pNext", which is of the same type.
        // - unreferenced chains:
        //   (INSTANCE does not exist) -/-> TYPEDEF_STRUCTURE -> TYPEDEF_MEASUREMENT
        while updated {
            updated = false;
            for td_struct in &self.module.typedef_structure {
                if self.is_calib_struct.contains_key(&td_struct.name) {
                    continue;
                }
                let mut is_meas = false;
                let mut is_calib = false;
                for sc in &td_struct.structure_component {
                    if self.typedef_names.measurement.contains(&sc.component_type) {
                        is_meas = true;
                    } else if self.typedef_names.contains(&sc.component_type) {
                        is_calib = true;
                    } else if let Some(&target_is_calib) =
                        self.is_calib_struct.get(&sc.component_type)
                    {
                        if target_is_calib {
                            is_calib = true;
                        } else {
                            is_meas = true;
                        }
                    }
                }
                if is_calib {
                    if is_meas {
                        // don't warn here, we might hit this condition several times due to the outer "while updated" loop
                        warn_set.insert(td_struct.name.clone());
                    }
                    self.is_calib_struct.insert(td_struct.name.clone(), true);
                    updated = true;
                } else if is_meas {
                    self.is_calib_struct.insert(td_struct.name.clone(), false);
                    updated = true;
                }
            }
        }
        for w in warn_set {
            self.log_msgs.push(format!("Warning: TYPEDEF_STRUCTURE {w} contains both calibration and measurement elements. This is forbiden by the spec."));
        }

        // Now update all structs referenced by structs with a known classification.
        // All of these are "dead ends", which do not reach a TYPEDEF_MEASUREMENT or similar.
        let mut updated = true;
        while updated {
            updated = false;
            for td_struct in &self.module.typedef_structure {
                if let Some(&is_calib) = self.is_calib_struct.get(&td_struct.name) {
                    // the current struct has a classification
                    for sc in &td_struct.structure_component {
                        if !self.typedef_names.contains(&sc.component_type) {
                            // it's a struct type, because typedef_names currently contains no struct names
                            // but this struct was not reached in the first pass and has no classification
                            if !self.is_calib_struct.contains_key(&sc.component_type) {
                                updated = true;
                                self.is_calib_struct
                                    .insert(sc.component_type.clone(), is_calib);
                            }
                        }
                    }
                }
            }
        }
    }

    /// collect all valid `TYPEDEF_STRUCTURES` into `self.typedef_structs`.
    /// Invalid items are discarded or preserved in `self.preserved_structs`.
    fn build_structure_hash(&mut self) {
        let mut structs_list = vec![];
        std::mem::swap(&mut structs_list, &mut self.module.typedef_structure);

        // collect all TYPEDEF_STRUCTUREs whose SYMBOL_TYPE_LINK points to a valid type into an IndexMap
        for td_struct in structs_list {
            // keep only those structs whose type in SYMBOL_TYPE_LINK is valid
            if let Some(typeinfo) =
                get_typeinfo_from_symbol_link(self.debug_data, &td_struct.symbol_type_link)
            {
                // Try to correct the case where the SYMBOL_TYPE_LINK incorrectly point to the type of
                // the pointer to the struct instead of the type of the struct
                let pt_type = typeinfo
                    .get_pointer(&self.debug_data.types)
                    .map_or(typeinfo, |(_, t)| t);
                let member_count_pt = pt_type.get_members().map_or(1, indexmap::IndexMap::len);
                let sc_count = td_struct.structure_component.len();
                // override this typeinfo with the type it points to
                let typeinfo = if pt_type.dbginfo_offset != typeinfo.dbginfo_offset
                    && sc_count > 1 // a TYPEDEF_STRUCTURE representing a pointer would have only one member
                    && member_count_pt == sc_count
                    && td_struct.address_type.is_some()
                    && is_structure_typeinfo(pt_type, &self.debug_data.types)
                {
                    pt_type
                } else {
                    typeinfo
                };

                // make sure the typeinfo actually represents a structure - for example if a structure
                // represents some usertype_t, which was previously a typedef of struct {...} and is not
                // a typedef of integer, then the corresponding TYPEDEF_STRUCTURE should be dropped, so
                // that a TYPEDEF_CHARACTERISTIC/MEASUREMENT can be created in its place

                // check if there is an entry in self.typedef_map that refers to this TYPEDEF_STRUCTURE
                // since the STRUCTURE_COMPONENTS have not been processed yet, this reference could only
                // come from an INSTANCE
                if let Some(instance_typeinfo) = self
                    .typedef_ref_info
                    .get(&td_struct.name)
                    .and_then(|info_vec| {
                        info_vec
                            .iter()
                            .filter_map(|(opt_type, _)| *opt_type)
                            .find(|t| {
                                t.name == typeinfo.name
                                    && is_structure_typeinfo(t, &self.debug_data.types)
                            })
                    })
                {
                    // this is the best case, giving the most accurate information
                    // -- unless there are multiple INSTANCES referring to the same TYPEDEF_STRUCTURE, but expecting
                    // different types. Should that happen, one of them "owns" this TYPEDEF_STRUCTURE, and the others
                    // are updated later.
                    self.type_map
                        .entry(instance_typeinfo.dbginfo_offset)
                        .or_default()
                        .insert(td_struct.name.clone());

                    // in particular, there is no doubt whether this is a struct or a pointer to a struct
                    self.typedef_map.insert(
                        td_struct.name.clone(),
                        (instance_typeinfo, TypeQuality::Exact),
                    );
                    self.typedef_structs
                        .insert(td_struct.name.clone(), td_struct);
                } else if is_structure_typeinfo(typeinfo, &self.debug_data.types) {
                    // the typeinfo was derived from the symbol_type_link.
                    // this will give us inexact info, but enough to analyse the STRUCUTRE_COMPONENTs later on.
                    // If some other TYPEDEF_STRUCTURE's component refers to this one, then the info saved here
                    // will be overwritten with better info during process_structure_components().
                    self.type_map
                        .entry(typeinfo.dbginfo_offset)
                        .or_default()
                        .insert(td_struct.name.clone());

                    self.typedef_map.insert(
                        td_struct.name.clone(),
                        (typeinfo, TypeQuality::SymTypeLinkOnly),
                    );
                    self.typedef_structs
                        .insert(td_struct.name.clone(), td_struct);
                } else {
                    // we have typeinfo from the SYMBOL_TYPE_LINK, but the type does not represent a structure
                    // e.g. plain uint32 or similar.
                    // this TYPEDEF_STRUCTURE is not retained in self.typedef_structs
                }
            } else if let Some(unnamed_typeinfo) = self
                .typedef_ref_info
                .get(&td_struct.name)
                .and_then(|info_vec| {
                    info_vec
                        .iter()
                        .filter_map(|(opt_type, _)| *opt_type)
                        .find(|t| {
                            t.name.is_none() && is_structure_typeinfo(t, &self.debug_data.types)
                        })
                })
            {
                // no valid symbol link, but an INSTANCE refers to the TYPEDEF_STRUCTURE
                // here we've found a suitable nameless typeinfo among the types referring to this structure.

                self.type_map
                    .entry(unnamed_typeinfo.dbginfo_offset)
                    .or_default()
                    .insert(td_struct.name.clone());

                self.typedef_map.insert(
                    td_struct.name.clone(),
                    (unnamed_typeinfo, TypeQuality::Exact),
                );
                self.typedef_structs
                    .insert(td_struct.name.clone(), td_struct);
            } else {
                self.preserved_structs
                    .insert(td_struct.name.clone(), td_struct);
            }
        }
        // collect all names of TYPEDEF_STRUCTUREs
        self.typedef_names.structure = self.typedef_structs.keys().cloned().collect();
        if !self.preserved_structs.is_empty() {
            self.typedef_names
                .structure
                .extend(self.preserved_structs.keys().cloned());
        }
    }

    /// delete all invalid `STRUCTURE_COMPONENTs`, and also collect the typeinfos for `TYPEDEF_CHARACRERISTIC` & co
    fn process_structure_components(&mut self, create_only: bool) {
        let mut idx = 0;
        // note: self.typedef_structs may be extended, so "for idx in 0..len()" would not work
        while idx < self.typedef_structs.len() {
            let (typeinfo, quality) = self
                .typedef_map
                .get(&self.typedef_structs[idx].name)
                .unwrap();
            // the typeinfo for the current structure is exact, so it could represent a pointer to a struct
            let typeinfo = if *quality == TypeQuality::Exact {
                // try to follow the pointer to get the target struct
                typeinfo
                    .get_pointer(&self.debug_data.types)
                    .map(|(_, t)| t)
                    .unwrap_or(typeinfo)
            } else {
                typeinfo
            };

            if let Some(members) = typeinfo.get_members() {
                // normal case: typeinfo is a struct / class / union and has multiple members
                let mut sc_old = Vec::new();
                std::mem::swap(
                    &mut sc_old,
                    &mut self.typedef_structs[idx].structure_component,
                );
                for sc in sc_old {
                    // the structure must have a member for each STRUCTURE_COMPONENT
                    if let Some(component_typeinfo) =
                        get_structure_component_typeinfo(self.debug_data, &sc, members)
                    {
                        if self.is_valid_structure_component(&sc.component_type, component_typeinfo)
                            || create_only
                        {
                            self.store_structure_component(idx, sc, component_typeinfo);
                        }
                    }
                }
            } else if let Some(component_typeinfo) = typeinfo.get_arraytype().or_else(|| {
                typeinfo
                    .get_pointer(&self.debug_data.types)
                    .map(|result| result.1)
            }) {
                // rare: typeinfo is a pointer or array. In this case a TYPEDEF_STRUCTURE is used as a layer of indirection.
                // This structure can only have a single structure component
                self.typedef_structs[idx].structure_component.truncate(1);
                if !self.typedef_structs[idx].structure_component.is_empty() {
                    let sc = self.typedef_structs[idx].structure_component.remove(0);
                    if self.is_valid_structure_component(&sc.component_type, component_typeinfo)
                        || create_only
                    {
                        self.store_structure_component(idx, sc, component_typeinfo);
                    }
                }
            }
            idx += 1;
        }
    }

    /// check if a structure component is valid during `process_structure_components()`
    /// the target must exist, and its type must be suitable for the kind of target
    fn is_valid_structure_component(
        &mut self,
        component_type: &str,
        typeinfo: &'dbg TypeInfo,
    ) -> bool {
        if self.typedef_names.structure.contains(component_type) {
            if is_structure_typeinfo(typeinfo, &self.debug_data.types) {
                // stored_typeinfo was calculated for each TYPEDEF_STRUCTURE based on the SYMBOL_TYPE_LINK
                // The typeinfo from the component member and the stored type must match.
                if let Some((stored_typeinfo, quality)) = self.typedef_map.get(component_type) {
                    // the stored_typeinfo for the target struct of the component_type might be inexact, lacking pointer info
                    let typeinfo = if *quality == TypeQuality::SymTypeLinkOnly {
                        // in this case we remove the pointer info from the precide information in
                        // typeinfo, in order to be able to compare them
                        typeinfo
                            .get_pointer(&self.debug_data.types)
                            .map_or(typeinfo, |(_, t)| t)
                    } else {
                        typeinfo
                    };
                    if typeinfo.compare(stored_typeinfo, &self.debug_data.types) {
                        // ok - refers to another TYPEDEF_STRUCTURE
                        return true;
                    }
                } else if let Some(td_struct) = self.preserved_structs.swap_remove(component_type) {
                    // The type was not in self.typedef_map, but at least there is a preserved struct whose name matches.
                    // Now we assume that this struct must be the one we're looking for.
                    // It gets moved back into self.typedef_structs to be updated, and all the type mappings are
                    // updated based on the component type referencing this struct
                    self.typedef_structs
                        .insert(td_struct.name.clone(), td_struct);
                    // the accounting gets updated in store_structure_component()
                    return true;
                } else {
                    // no valid target for the structure component
                    // a suitable TYPEDEF_* will be created later
                }
            }
        } else if self.typedef_names.measurement.contains(component_type) {
            // a TYPEDEF_MEASUREMENT with the target name exists
            if is_measurement_typeinfo(typeinfo, &self.debug_data.types) {
                // ok - refers to a TYPEDEF_MEASUREMENT
                return true;
            }
        } else if self.typedef_names.characteristic.contains(component_type) {
            if is_calibration_typeinfo(typeinfo) {
                // ok - refers to TYPEDEF_CHARACTERISTIC
                return true;
            }
        } else if self.typedef_names.contains(component_type) {
            // ok - refers to TYPEDEF_AXIS or TYPEDEF_BLOB
            // no additional restrictions here, we don't create or update
            // TYPEDEF_AXIS, and TYPEDEF_BLOB can legitimately be anything
            return true;
        }
        false
    }

    /// store a valid structure component and update information about the target
    fn store_structure_component(
        &mut self,
        struct_idx: usize,
        sc: StructureComponent,
        component_typeinfo: &'dbg TypeInfo,
    ) {
        self.typedef_ref_info
            .entry(sc.component_type.clone())
            .or_default()
            .push((
                Some(component_typeinfo),
                TypedefReferrer::StructureComponent(
                    self.typedef_structs[struct_idx].name.clone(),
                    sc.component_name.clone(),
                ),
            ));
        if let Some((old_typeinfo, old_quality)) = self.typedef_map.swap_remove(&sc.component_type)
        {
            if !old_typeinfo.compare(component_typeinfo, &self.debug_data.types)
                && old_quality == TypeQuality::SymTypeLinkOnly
            {
                self.type_map
                    .get_mut(&old_typeinfo.dbginfo_offset)
                    .unwrap()
                    .swap_remove(&sc.component_type);
            }
        }
        self.type_map
            .entry(component_typeinfo.dbginfo_offset)
            .or_default()
            .insert(sc.component_type.clone());
        self.typedef_map.insert(
            sc.component_type.clone(),
            (component_typeinfo, TypeQuality::Exact),
        );
        self.typedef_structs[struct_idx]
            .structure_component
            .push(sc);
    }

    /// each INSTANCE in the module has a `type_ref` that refers to a TYPEDEF_* by name
    /// Additionally, a dwarf typeinfo was associated with each of them by looking up
    /// the variable name of the INSTANCE in the debug data.
    /// Now we need to make sure that a correct target exists for each of these references.
    fn create_missing_instance_targets(&mut self) {
        let mut enum_convlist = HashMap::<String, &TypeInfo>::new();
        let mut delete_instances = HashSet::new();
        let mut refnames: Vec<_> = self.typedef_ref_info.keys().cloned().collect();
        refnames.sort();
        for refname in &refnames {
            // extract the update_info, otherwise the borrow checker considers all of self to be borrowed
            let mut update_info = Vec::new();
            std::mem::swap(
                &mut update_info,
                self.typedef_ref_info.get_mut(refname).unwrap(),
            );

            // nothing to do here if the referrer is not an Instance because typedef_ref_info
            // for structs was only added if it was consistent
            if !update_info
                .iter()
                .any(|(_, r)| matches!(r, TypedefReferrer::Instance(_)))
            {
                // put the update info back
                std::mem::swap(
                    &mut update_info,
                    self.typedef_ref_info.get_mut(refname).unwrap(),
                );
                // and then skip this TYPEDEF
                continue;
            }

            // get the set of distinct types - usually there should only be a single distinct type per refname
            // but if there are multiple references to a single TYPEDEF_x and one of these is modified
            // then this situation could happen
            let mut dtypes = calc_distinct_types(&update_info, self.debug_data);
            // try to ensure the distinct type whose name matches the typedef name is first in the list
            for idx in 1..dtypes.len() {
                if let Some(name) = dtypes[0].name.as_deref() {
                    if name == refname {
                        dtypes.swap(0, idx);
                        break;
                    }
                }
            }

            let is_calib = if self.typedef_names.measurement.contains(refname) {
                // TYPEDEF_MEASUREMENT
                false
            } else if self.typedef_names.structure.contains(refname) {
                // existing TYPEDEF_STRUCTURE
                *self.is_calib_struct.get(refname).unwrap_or(&false)
            } else if self.typedef_names.contains(refname) {
                // TYPEDEF_AXIS / TYPEDEF_BLOB / TYPEDEF_CHARACTERISTIC
                true
            } else {
                // nonexistent TYPEDEF, use the "magic" refname to determine if a TYPEDEF_CHARWYCTERISTIC should be created
                // '|' is not allowed in names, so this name should only occur when it is used as a flag in the insert code.
                refname == FLAG_CREATE_CALIB
            };

            for typeinfo in dtypes {
                // create will create a new TYPEDEF as needed, but may also associate the typeinfo with an existing typedef
                if let Some(typedef_name) =
                    self.create_typedef(typeinfo, is_calib, &mut enum_convlist)
                {
                    self.update_typedef_referrers(&update_info, typeinfo, &typedef_name);
                } else {
                    // If no TYPEDEF can be found or created for an INSTANCE, then deleting it is the best option
                    for (opt_typeinfo, referrer) in &update_info {
                        if let TypedefReferrer::Instance(idx) = referrer {
                            if let Some(ref_typeinfo) = opt_typeinfo {
                                if ref_typeinfo.compare(typeinfo, &self.debug_data.types) {
                                    delete_instances.insert(*idx);
                                }
                            }
                        }
                    }
                }
            }
        }

        let mut delete_instances_list: Vec<_> = delete_instances.iter().collect();
        delete_instances_list.sort_by(|a, b| b.cmp(a));
        for idx in delete_instances_list {
            self.module.instance.remove(*idx);
        }

        update_enum_compu_methods(self.module, &enum_convlist);
    }

    /// ensure a `TYPEDEF_STRUCTURE`, `TYPEDEF_CHARACTERISTIC` or `TYPEDEF_MEASUREMENT`
    /// exists for the input typeinfo. Usually this means creating a TYPEDEF_*, but
    /// it might be possible to find an existing target.
    fn create_typedef(
        &mut self,
        typeinfo: &'dbg TypeInfo,
        is_calib: bool,
        enum_convlist: &mut HashMap<String, &'dbg TypeInfo>,
    ) -> Option<String> {
        // first look for an existing TYPEDEF
        if let Some(existing) = self.find_existing_typedef(typeinfo, is_calib) {
            self.type_map
                .entry(typeinfo.dbginfo_offset)
                .or_default()
                .insert(existing.clone());
            return Some(existing);
        }

        // make a new name for the TYPEDEF_*. This name is not neccessarily unique.
        let typedef_name = make_typedef_name(self.debug_data, typeinfo, is_calib);
        let mut newname: Cow<str> = Cow::Borrowed(&typedef_name);
        let mut copycount = 0;
        let mut should_create = true;
        // make the name unique - if the "ideal" typedef_name is already in use, append _Copy<x>
        // until it is unique.
        while self.typedef_names.contains(&newname) {
            if is_calib {
                if is_calibration_typeinfo(typeinfo)
                    && self.typedef_names.characteristic.contains::<str>(&newname)
                {
                    // there is an existing matching TYPEDEF_CHARACTERISTIC called <newname>
                    should_create = false;
                    break;
                }
            } else if is_measurement_typeinfo(typeinfo, &self.debug_data.types)
                && self.typedef_names.measurement.contains::<str>(&newname)
            {
                // there is an existing matching TYPEDEF_MEASUREMENT called <newname>
                should_create = false;
                break;
            }
            copycount += 1;
            newname = format!("{typedef_name}_Copy{copycount}").into();
        }

        let name: String = newname.into_owned();
        if should_create {
            if is_calib {
                if is_structure_typeinfo(typeinfo, &self.debug_data.types) {
                    self.create_typedef_structure(name.clone(), typeinfo, enum_convlist, is_calib);
                } else if is_calibration_typeinfo(typeinfo) {
                    self.create_typedef_characteristic(name.clone(), typeinfo, enum_convlist);
                } else {
                    // for typeinfo.datatype == Other, typically *void
                    self.create_typedef_blob(name.clone(), typeinfo);
                }
            } else if is_measurement_typeinfo(typeinfo, &self.debug_data.types) {
                self.create_typedef_measurement(name.clone(), typeinfo, enum_convlist);
            } else if is_structure_typeinfo(typeinfo, &self.debug_data.types) {
                self.create_typedef_structure(name.clone(), typeinfo, enum_convlist, is_calib);
            } else {
                // FuncPtr and Other don't work for measurement
                return None;
            }
        } else {
            // didn't create anthing, but insert the TYPEDEF_* name into type_map
            self.type_map
                .entry(typeinfo.dbginfo_offset)
                .or_default()
                .insert(name.clone());
        }

        Some(name)
    }

    /// create a new `TYPEDEF_CHARACTERISTIC` with the given name
    fn create_typedef_characteristic(
        &mut self,
        name: String,
        typeinfo: &'dbg TypeInfo,
        enum_convlist: &mut HashMap<String, &'dbg TypeInfo>,
    ) {
        // bookkeeping
        self.type_map
            .entry(typeinfo.dbginfo_offset)
            .or_default()
            .insert(name.clone());
        self.typedef_map
            .insert(name.clone(), (typeinfo, TypeQuality::Exact));
        self.typedef_names.characteristic.insert(name.clone());

        self.log_msgs
            .push(format!("creating TYPEDEF_CHARACTERISTIC \"{name}\""));

        let datatype = get_a2l_datatype(typeinfo);
        let recordlayout_name = format!("__{datatype}_Z");
        let mut td_char = TypedefCharacteristic::new(
            name,
            String::new(),
            CharacteristicType::Value,
            recordlayout_name.clone(),
            0.0,
            "NO_COMPU_METHOD".to_string(),
            0.0,
            0.0,
        );
        // create a RECORD_LAYOUT for the _CHARACTERISTIC if it doesn't exist yet
        // the used naming convention (__<type>_Z) matches default naming used by Vector tools
        let mut recordlayout = RecordLayout::new(recordlayout_name.clone());
        // set item 0 (name) to use an offset of 0 lines, i.e. no line break after /begin RECORD_LAYOUT
        recordlayout.get_layout_mut().item_location.0 = 0;
        recordlayout.fnc_values = Some(FncValues::new(
            1,
            datatype,
            IndexMode::RowDir,
            AddrType::Direct,
        ));

        // check if there is an existing record layout and only add the new one if it doesn't exist yet
        if let Some(idx) = self.recordlayout_info.idxmap.get(&recordlayout_name) {
            // make sure the refcount in self.recordlayout_info is correct, or else update_record_layout can fail
            self.recordlayout_info.refcount[*idx] += 1;
        } else {
            let idx = self.module.record_layout.len();
            self.module.record_layout.push(recordlayout);
            self.recordlayout_info.idxmap.insert(recordlayout_name, idx);
            self.recordlayout_info.refcount.push(1);
        }

        self.update_typedef_characteristic(&mut td_char, typeinfo, enum_convlist);
        self.module.typedef_characteristic.push(td_char);
    }

    // create a new TYPEDEF_BLOB with the given name
    fn create_typedef_blob(&mut self, name: String, typeinfo: &'dbg TypeInfo) {
        // bookkeeping
        self.type_map
            .entry(typeinfo.dbginfo_offset)
            .or_default()
            .insert(name.clone());
        self.typedef_map
            .insert(name.clone(), (typeinfo, TypeQuality::Exact));
        self.typedef_names.blob.insert(name.clone());

        self.log_msgs
            .push(format!("creating TYPEDEF_BLOB \"{name}\""));
        let td_blob = TypedefBlob::new(name, String::new(), typeinfo.get_size() as u32);
        self.module.typedef_blob.push(td_blob);
    }

    // create a new TYPEDEF_MEASUREMENT with the given name
    fn create_typedef_measurement(
        &mut self,
        name: String,
        typeinfo: &'dbg TypeInfo,
        enum_convlist: &mut HashMap<String, &'dbg TypeInfo>,
    ) {
        // bookkeeping
        self.type_map
            .entry(typeinfo.dbginfo_offset)
            .or_default()
            .insert(name.clone());
        self.typedef_map
            .insert(name.clone(), (typeinfo, TypeQuality::Exact));
        self.typedef_names.measurement.insert(name.clone());

        self.log_msgs
            .push(format!("creating TYPEDEF_MEASUREMENT \"{name}\""));
        let mut td_meas = TypedefMeasurement::new(
            name,
            String::new(),
            get_a2l_datatype(typeinfo),
            "NO_COMPU_METHOD".to_string(),
            0,
            0.0,
            0.0,
            0.0,
        );
        self.update_typedef_measurement(&mut td_meas, typeinfo, enum_convlist);
        self.module.typedef_measurement.push(td_meas);
    }

    // create a new TYPEDEF_STRUCTURE
    fn create_typedef_structure(
        &mut self,
        name: String,
        typeinfo: &'dbg TypeInfo,
        enum_convlist: &mut HashMap<String, &'dbg TypeInfo>,
        is_calib: bool,
    ) {
        // bookkeeping - it's important to do this *before* update_typedef_structure()!
        // update_typedef_structure() can create more TYPEDEF_STRUCTUREs and will use this info
        self.type_map
            .entry(typeinfo.dbginfo_offset)
            .or_default()
            .insert(name.clone());
        self.typedef_map
            .insert(name.clone(), (typeinfo, TypeQuality::Exact));
        self.typedef_names.structure.insert(name.clone());
        self.is_calib_struct.insert(name.clone(), is_calib);

        self.log_msgs
            .push(format!("creating TYPEDEF_STRUCTURE \"{name}\""));

        // create the TYPEDEF_STRUCTURE
        let mut td_struct = TypedefStructure::new(name.clone(), String::new(), 0);
        self.update_typedef_structure(&mut td_struct, typeinfo, enum_convlist);

        // display item .2 (size) in hex by default
        td_struct.get_layout_mut().item_location.2 = (1, true);

        self.typedef_structs.insert(name, td_struct);
    }

    /// try to find an existing typedef for the typeinfo and calib class
    fn find_existing_typedef(
        &mut self,
        typeinfo: &'dbg TypeInfo,
        is_calib: bool,
    ) -> Option<String> {
        if let Some(typeoffsets) = typeinfo
            .name
            .as_deref()
            .and_then(|tname| self.debug_data.typenames.get(tname))
        {
            let typename = typeinfo.name.as_deref().unwrap();
            // If the type has a typename, then debug_data.typenames will have a HashMap of all typeinfos for this typename
            // there is a good chance that all of these are actually the same type, just used in different compilation
            // units, resulting in multiple identical entries in the debug data.
            // There might be a TYPEDEF_* for a type which is identical to the input typeinfo, but which has a different
            // debuginfo offset.
            let mut candidates: IndexSet<&String> = typeoffsets
                .iter()
                .filter_map(|offset| self.debug_data.types.get(offset))
                .filter(|t| t.compare(typeinfo, &self.debug_data.types))
                .filter_map(|t| self.type_map.get(&t.dbginfo_offset))
                .flat_map(|set| set.iter())
                .collect();
            candidates.sort();
            // try to find a suitable structure whose name matches the typename exactly
            for candidate in &candidates {
                if *candidate == typename && self.check_typedef_class(candidate, is_calib) {
                    return Some((*candidate).clone());
                }
            }
            // drop the name requirement and find any candidate in the correct class
            for candidate in &candidates {
                if self.check_typedef_class(candidate, is_calib) {
                    return Some((*candidate).clone());
                }
            }
        }
        // no type name available, or nothing found using the type name, so any candidate in the correct class will do
        // Search using the typename always fails for pointers: the dwarf typereader copies the name of the pointer target
        // into the pointer, but it is not present in debug_data.typenames nder this name.
        for candidate in self.type_map.get(&typeinfo.dbginfo_offset)? {
            if self.check_typedef_class(candidate, is_calib) {
                return Some(candidate.clone());
            }
        }
        None
    }

    /// check if a named TYPEDEF belongs to the provided calib class
    fn check_typedef_class(&self, candidate: &String, is_calib: bool) -> bool {
        if self.typedef_names.measurement.contains(candidate) {
            // true if the candidate is a TYPEDEF_MEASUREMENT and !is_calib is required
            !is_calib
        } else if self.typedef_names.structure.contains(candidate) {
            // true if the candidate is a TYPEDEF_STRUCTURE and its existing type matches the required is_calib
            is_calib == *self.is_calib_struct.get(candidate).unwrap_or(&false)
        } else {
            // the candidate must be TYPEDEF_AXIS/BLOB/CHARACTERISTIC
            is_calib
        }
    }

    /// use `self.typedef_ref_info` to update the name of a type in all locations referring to this type
    fn update_typedef_referrers(
        &mut self,
        ref_info: &Vec<(Option<&'dbg TypeInfo>, TypedefReferrer)>,
        newtype: &'dbg TypeInfo,
        newname: &str,
    ) {
        for (reftype, referrer) in ref_info {
            if reftype.is_some()
                && newtype.compare(reftype.as_ref().unwrap(), &self.debug_data.types)
            {
                self.typedef_ref_info
                    .entry(newname.to_string())
                    .or_default()
                    .push((Some(newtype), referrer.clone()));

                match referrer {
                    TypedefReferrer::Instance(instance_idx) => {
                        self.module.instance[*instance_idx].type_ref = newname.to_string();
                    }
                    TypedefReferrer::StructureComponent(st_name, cmp_name) => {
                        if let Some(td_struct) = self.typedef_structs.get_mut(st_name) {
                            if let Some(component) = td_struct
                                .structure_component
                                .iter_mut()
                                .find(|cmp| cmp.component_name == *cmp_name)
                            {
                                component.component_type = newname.to_string();
                            }
                        }
                    }
                }
            }
        }
    }

    /// update `TYPEDEF_AXIS`?
    fn update_all_typedef_axis(&mut self) {
        // It's not clear that these can or should be updated.
    }

    /// update all `TYPEDEF_BLOBs`
    ///
    /// These don't contain much info to begin with, but the size and `address_type` can (usually) be updated.
    fn update_all_typedef_blob(&mut self) {
        let mut typedef_blob = Vec::new();
        std::mem::swap(&mut typedef_blob, &mut self.module.typedef_blob);
        for td_blob in &mut typedef_blob {
            if let Some((blob_type, _)) = self.typedef_map.get(&td_blob.name) {
                self.log_msgs
                    .push(format!("updating TYPEDEF_BLOB \"{}\"", td_blob.name));

                td_blob.size = get_typedef_size(self.debug_data, blob_type);
                set_address_type(&mut td_blob.address_type, blob_type);

                // update all instances referring to this blob
                if let Some(blob_info) = self.typedef_ref_info.get(&td_blob.name).cloned() {
                    let name = td_blob.name.clone();
                    self.update_typedef_referrers(&blob_info, blob_type, &name);
                }
            }
        }
        std::mem::swap(&mut typedef_blob, &mut self.module.typedef_blob);
    }

    /// update all `TYPEDEF_CHARACTERISTICs`
    fn update_all_typedef_characteristic(&mut self) {
        let mut enum_convlist = HashMap::<String, &TypeInfo>::new();
        let mut typedef_characteristic = Vec::new();
        // borrow checker workaround: extract the list of typedef_characteristic from the module, so
        // that we can have mutable references to items without locking up self as well
        std::mem::swap(
            &mut typedef_characteristic,
            &mut self.module.typedef_characteristic,
        );

        for td_char in &mut typedef_characteristic {
            if let Some((char_type, _)) = self.typedef_map.get(&td_char.name).cloned() {
                self.log_msgs.push(format!(
                    "updating TYPEDEF_CHARACTERISTIC \"{}\"",
                    td_char.name
                ));
                self.update_typedef_characteristic(td_char, char_type, &mut enum_convlist);
                // update all instances referring to this characteristic
                if let Some(char_info) = self.typedef_ref_info.get(&td_char.name).cloned() {
                    self.update_typedef_referrers(&char_info, char_type, &td_char.name);
                }
            }
        }

        std::mem::swap(
            &mut typedef_characteristic,
            &mut self.module.typedef_characteristic,
        );
        update_enum_compu_methods(self.module, &enum_convlist);
    }

    /// update one `TYPEDEF_CHARACTERISTIC`
    fn update_typedef_characteristic(
        &mut self,
        td_char: &mut TypedefCharacteristic,
        char_type: &'dbg TypeInfo,
        enum_convlist: &mut HashMap<String, &'dbg TypeInfo>,
    ) {
        // CHARACTERISTICs and TYPEDEF_CHARACTERISTICs can be structs. in that case the RECORD_LAYOUT tells us
        // which of the struct members contains the data. Other members would typically represent AXIS information.
        // the inner_typeinfo we're looking for here is the typeinfo of the data struct member.
        // If this is not a struct, then inner_typeinfo == char_type
        let member_id =
            get_fnc_values_memberid(self.module, self.recordlayout_info, &td_char.record_layout);
        if let Some(inner_typeinfo) = get_inner_type(char_type, member_id) {
            if let DwarfDataType::Enum { enumerators, .. } = &inner_typeinfo.datatype {
                // the values of this struct are of type enum
                let enum_name = inner_typeinfo
                    .name
                    .clone()
                    .unwrap_or_else(|| format!("{}_compu_method", td_char.name));
                if td_char.conversion == "NO_COMPU_METHOD" {
                    td_char.conversion = enum_name;
                }
                cond_create_enum_conversion(self.module, &td_char.conversion, enumerators);
                enum_convlist.insert(td_char.conversion.clone(), inner_typeinfo);
            }
            set_bitmask(&mut td_char.bit_mask, inner_typeinfo);

            let (ll, ul) = adjust_limits(inner_typeinfo, td_char.lower_limit, td_char.upper_limit);
            td_char.lower_limit = ll;
            td_char.upper_limit = ul;
        }

        // if the TYPEDEF_CHARACTERISTIC represents a string (characteristic_type = ASCII),
        // then the element NUMBER should contain the string length
        if td_char.characteristic_type == CharacteristicType::Ascii {
            // a string is an array of characters. We only require the array, because a
            // character type can be different things in different situations or languages: e.g. char / wchar_t
            if let DwarfDataType::Array { dim, .. } = &char_type.datatype {
                if dim.len() == 1 {
                    let number = td_char.number.get_or_insert(Number::new(0));
                    td_char.matrix_dim = None;
                    number.number = dim[0] as u16;
                }
                // don't know what to do with multi-dimensional arrays, so just leave those untouched
            } else {
                // clearly this is not a string - change the type to value instead
                td_char.characteristic_type = CharacteristicType::Value;
            }
        }

        if td_char.characteristic_type == CharacteristicType::Value
            || td_char.characteristic_type == CharacteristicType::ValBlk
        {
            td_char.number = None;
            set_matrix_dim(&mut td_char.matrix_dim, char_type, true);
            // arrays of values should have the type ValBlk, while single values should NOT have the type ValBlk
            if td_char.characteristic_type == CharacteristicType::Value
                && td_char.matrix_dim.is_some()
            {
                // change Value -> ValBlk
                td_char.characteristic_type = CharacteristicType::ValBlk;
            } else if td_char.characteristic_type == CharacteristicType::ValBlk
                && td_char.matrix_dim.is_none()
            {
                // change ValBlk -> Value
                td_char.characteristic_type = CharacteristicType::Value;
            }
        }

        let record_layout =
            if let Some(idx) = self.recordlayout_info.idxmap.get(&td_char.record_layout) {
                Some(&self.module.record_layout[*idx])
            } else {
                None
            };
        update_characteristic_axis(
            &mut td_char.axis_descr,
            record_layout,
            &self.axis_pts_dim,
            char_type,
        );
        td_char.record_layout = update_record_layout(
            self.module,
            self.recordlayout_info,
            &td_char.record_layout,
            char_type,
        );
    }

    /// update all `TYPEDEF_MEASUREMENTs`
    fn update_all_typedef_measurement(&mut self) {
        let mut enum_convlist = HashMap::<String, &TypeInfo>::new();
        let mut typedef_measurement = Vec::new();
        std::mem::swap(
            &mut typedef_measurement,
            &mut self.module.typedef_measurement,
        );

        for td_meas in &mut typedef_measurement {
            if let Some((meas_type, _)) = self.typedef_map.get(&td_meas.name).cloned() {
                self.log_msgs
                    .push(format!("updating TYPEDEF_MEASUREMENT \"{}\"", td_meas.name));

                self.update_typedef_measurement(td_meas, meas_type, &mut enum_convlist);
                // update all instances referring to this characteristic
                if let Some(meas_info) = self.typedef_ref_info.get(&td_meas.name).cloned() {
                    self.update_typedef_referrers(&meas_info, meas_type, &td_meas.name);
                }
            }
        }

        std::mem::swap(
            &mut typedef_measurement,
            &mut self.module.typedef_measurement,
        );
        update_enum_compu_methods(self.module, &enum_convlist);
    }

    /// update one `TYPEDEF_MEASUREMENT`
    fn update_typedef_measurement(
        &mut self,
        td_meas: &mut TypedefMeasurement,
        meas_type: &'dbg TypeInfo,
        enum_convlist: &mut HashMap<String, &'dbg TypeInfo>,
    ) {
        td_meas.datatype = get_a2l_datatype(meas_type);
        set_bitmask(&mut td_meas.bit_mask, meas_type);
        if let DwarfDataType::Enum { enumerators, .. } = &meas_type.datatype {
            if td_meas.conversion == "NO_COMPU_METHOD" {
                td_meas.conversion = meas_type
                    .name
                    .clone()
                    .unwrap_or_else(|| format!("{}_compu_method", td_meas.name));
            }
            cond_create_enum_conversion(self.module, &td_meas.conversion, enumerators);
            enum_convlist.insert(td_meas.conversion.clone(), meas_type);
        }

        let (ll, ul) = adjust_limits(meas_type, td_meas.lower_limit, td_meas.upper_limit);
        td_meas.lower_limit = ll;
        td_meas.upper_limit = ul;

        set_matrix_dim(&mut td_meas.matrix_dim, meas_type, true);
    }

    /// update all `TYPEDEF_STRUCTUREs`
    fn update_all_typedef_structure(&mut self) {
        let mut enum_convlist = HashMap::<String, &TypeInfo>::new();
        let mut typedef_structs = FxIndexMap::default();
        std::mem::swap(&mut typedef_structs, &mut self.typedef_structs);

        for (_, td_struct) in &mut typedef_structs {
            if let Some((struct_type, _)) = self.typedef_map.get(&td_struct.name).cloned() {
                self.log_msgs
                    .push(format!("updating TYPEDEF_STRUCTURE \"{}\"", td_struct.name));

                self.update_typedef_structure(td_struct, struct_type, &mut enum_convlist);
                // update all instances referring to this structure
                if let Some(struct_info) = self.typedef_ref_info.get(&td_struct.name).cloned() {
                    self.update_typedef_referrers(&struct_info, struct_type, &td_struct.name);
                }
            }
        }

        // updating the structs may have caused new structs to be created.
        // These will have been added to the empty self.typedef_structs
        // Build a new IndexMap of old + new
        let mut typedef_structs2 = FxIndexMap::default();
        std::mem::swap(&mut typedef_structs2, &mut self.typedef_structs);
        typedef_structs.extend(typedef_structs2);
        self.typedef_structs = typedef_structs;
        update_enum_compu_methods(self.module, &enum_convlist);
    }

    /// update one `TYPEDEF_STRUCTURE`
    ///
    /// This will update or create all `STRUCTURE_COMPONENTs`, so the update could result
    /// in the creation of multiple other `TYPEDEF_STRUCTUREs`.
    pub(crate) fn update_typedef_structure(
        &mut self,
        td_struct: &mut TypedefStructure,
        typeinfo: &'dbg TypeInfo,
        enum_convlist: &mut HashMap<String, &'dbg TypeInfo>,
    ) {
        let is_calib = *self.is_calib_struct.get(&td_struct.name).unwrap_or(&false);

        td_struct.total_size = get_typedef_size(self.debug_data, typeinfo);
        self.update_symbol_type_link(td_struct, typeinfo);
        set_address_type(&mut td_struct.address_type, typeinfo);

        // dereference one layer of pointer indirection, if there is one
        let typeinfo = typeinfo
            .get_pointer(&self.debug_data.types)
            .map_or(typeinfo, |(_, t)| t);

        match &typeinfo.datatype {
            DwarfDataType::Struct { members, .. }
            | DwarfDataType::Union { members, .. }
            | DwarfDataType::Class { members, .. } => {
                // typical case: the data type of the typedef struct is "structlike" and has a list of members
                self.update_typedef_struct_content(td_struct, members, enum_convlist, is_calib);
            }
            DwarfDataType::Array { .. } => {
                // This type is not a struct, it is actually an array.
                // In this case, there is only one STRUCTURE_COMPONENT which represents the array element type
                // The structure component has an offset of 0 and a MATRIX_DIM to represent the array correctly
                td_struct.structure_component.truncate(1);
                if td_struct.structure_component.is_empty() {
                    td_struct.structure_component.push(StructureComponent::new(
                        String::new(),
                        String::new(),
                        0,
                    ));
                    let layout = td_struct.structure_component[0].get_layout_mut();
                    layout.start_offset = 1; // only one newline before this block -- i.e. no empty lines
                    layout.item_location.2 = (1, false); // offset is placed on a new line, not displayd as hex
                }
                let sc = &mut td_struct.structure_component[0];
                sc.address_offset = 0;
                sc.component_name = "array_element".to_string();
                sc.symbol_type_link = None;
                set_matrix_dim(&mut sc.matrix_dim, typeinfo, true);

                let inner_type = typeinfo.get_arraytype().unwrap_or(typeinfo);
                if let Some(typedef_name) = self.create_typedef(inner_type, is_calib, enum_convlist)
                {
                    sc.component_type = typedef_name;
                } else {
                    td_struct.structure_component.truncate(0);
                }
            }
            DwarfDataType::Pointer(_, _) => {
                // insanity! - the original declaration would have to be something like "sometype*** var".
                // In that situation, the INSTANCE would consume the first layer of indirection and set ADDRESS_TYPE,
                // then this TYPEDEF_STRUCTURE gets the second layer and also sets ADDRESS_TYPE, and finally we get here.
                td_struct.structure_component.truncate(1);
                if td_struct.structure_component.is_empty() {
                    td_struct.structure_component.push(StructureComponent::new(
                        String::new(),
                        String::new(),
                        0,
                    ));
                    let layout = td_struct.structure_component[0].get_layout_mut();
                    layout.start_offset = 1; // only one newline before this block -- i.e. no empty lines
                    layout.item_location.2 = (1, false); // offset is placed on a new line, not displayd as hex
                }
                let sc = &mut td_struct.structure_component[0];
                sc.address_offset = 0;
                sc.component_name = "ptr".to_string();
                set_address_type(&mut sc.address_type, typeinfo);
                if let Some((_, pt_type)) = typeinfo.get_pointer(&self.debug_data.types) {
                    // it might even be a pointer to an array!
                    set_matrix_dim(&mut sc.matrix_dim, pt_type, true);
                }
                let inner_type = typeinfo
                    .get_pointer(&self.debug_data.types)
                    .map_or(typeinfo, |(_, t)| t);
                sc.symbol_type_link = None;

                let inner_type = inner_type.get_arraytype().unwrap_or(inner_type);
                if let Some(typedef_name) = self.create_typedef(inner_type, is_calib, enum_convlist)
                {
                    sc.component_type = typedef_name;
                } else {
                    td_struct.structure_component.truncate(0);
                }
            }
            _ => {
                // we should not get here, since all other datatypes should be a TYPEDEF_CHARACTERISTIC or TYPEDEF_MEASUREMENT instead
            }
        }
    }

    /// update the `STRUCTURE_COMPONENTs` of a `TYPEDEF_STRUCTURE`
    fn update_typedef_struct_content(
        &mut self,
        td_struct: &mut TypedefStructure,
        members: &'dbg IndexMap<String, (TypeInfo, u64)>,
        enum_convlist: &mut HashMap<String, &'dbg TypeInfo>,
        is_calib: bool,
    ) {
        let mut structure_components = Vec::new();
        std::mem::swap(
            &mut structure_components,
            &mut td_struct.structure_component,
        );
        for (cur_member_name, (typeinfo_ref, cur_member_offset)) in members {
            let cur_type = typeinfo_ref.get_reference(&self.debug_data.types);
            let mut sc = if let Some(sc) = structure_components
                .iter()
                .find(|sc| &sc.component_name == cur_member_name)
            {
                sc.clone()
            } else {
                let mut sc = StructureComponent::new(String::new(), String::new(), 0);
                let layout = sc.get_layout_mut();
                layout.start_offset = 1; // only one newline before this block -- i.e. no empty lines
                layout.item_location.2 = (1, false); // offset is placed on a new line, not displayd as hex
                sc
            };

            // follow the pointer if cur_member_typeinfo is a pointer, or keep the current type
            let cur_type_nopointer = cur_type
                .get_pointer(&self.debug_data.types)
                .map_or(cur_type, |(_, t)| t);
            let cur_type_unwrapped = cur_type_nopointer
                .get_arraytype()
                .unwrap_or(cur_type_nopointer);

            if let Some(final_typeinfo) = fully_unwrap_typeinfo(self.debug_data, cur_type_unwrapped)
            {
                // only create a STRUCTURE_COMPONENT for items whose inner datatype is not FuncPtr
                // Other is used for void pointers, which is only allowed for calibration as a TYPEDEF_BLOB
                if !matches!(&final_typeinfo.datatype, DwarfDataType::FuncPtr(_))
                    && (is_calib || !matches!(&final_typeinfo.datatype, DwarfDataType::Other(_)))
                {
                    sc.component_name = cur_member_name.clone();
                    // set ADDRESS_TYPE if cur_member_typeinfo is a pointer, or delete it
                    set_address_type(&mut sc.address_type, cur_type);
                    // update, set or delete MATRIX_DIM
                    set_matrix_dim(&mut sc.matrix_dim, cur_type_nopointer, true);
                    // update or create the SYMBOL_TYPE_LINK of the STRUCTURE_COMPONENT
                    if let Some(symbol_type_link) = &mut sc.symbol_type_link {
                        symbol_type_link.symbol_type = cur_member_name.clone();
                    } else {
                        sc.symbol_type_link = Some(SymbolTypeLink::new(cur_member_name.clone()));
                    }

                    sc.address_offset = *cur_member_offset as u32;
                    if let Some(typedef_name) =
                        self.create_typedef(cur_type_unwrapped, is_calib, enum_convlist)
                    {
                        sc.component_type = typedef_name;

                        self.typedef_ref_info
                            .entry(sc.component_type.clone())
                            .or_default()
                            .push((
                                Some(cur_type_unwrapped),
                                TypedefReferrer::StructureComponent(
                                    td_struct.name.clone(),
                                    sc.component_name.clone(),
                                ),
                            ));
                        td_struct.structure_component.push(sc);
                    }
                }
            }
        }
    }

    /// update the `SYMBOL_TYPE_LINK` of a `TYPEDEF_STRUCTURE`
    fn update_symbol_type_link(&self, td_struct: &mut TypedefStructure, typeinfo: &TypeInfo) {
        if let Some(name) = &typeinfo.name {
            // the type has a name
            if let Some(stl) = &mut td_struct.symbol_type_link {
                // a SYMBOL_TYPE_LINK already exists and can be updated
                if is_type_discriminant_needed(self.debug_data, name)
                    && self.debug_data.unit_names[typeinfo.unit_idx].is_some()
                {
                    // Vector generates 'SYMBOL_TYPE_LINK "SomeSymbol{CompileUnit:Some_File_c}{Namespace:Global}"'
                    // This can remove some ambiguity when multiple files or namespaces define SomeSymbol, but
                    // it's not perfect, since the path is stripped rom the file name.
                    if let Some(simple_unit_name) =
                        make_simple_unit_name(self.debug_data, typeinfo.unit_idx)
                    {
                        // this could fail with advanced DWARF encoding, i.e. when partial units are in use
                        stl.symbol_type = format!("{name}{{CompileUnit:{simple_unit_name}}}");
                    }
                } else {
                    // SYMBOL_TYPE_LINK contains the bare symbol name, which can be directly replaced
                    stl.symbol_type = name.clone();
                }
            } else {
                // SYMBOL_TYPE_LINK did not exist and needs to be created
                td_struct.symbol_type_link = Some(SymbolTypeLink::new(name.clone()));
            }
        } else {
            // the type is unnamed - possible, but very unlikely here
            td_struct.symbol_type_link = None;
        }
    }

    /// delete any unreferenced TYPEDEF_*
    /// The removal of a `TYPEDEF_STRUCTURE` may cause more TYPDEFs to become unreferenced, so this
    /// function continues to search for items to delete until no more deletable TYPEDEFs are found
    fn cleanup_unused_typedefs(&mut self) {
        let mut updated = true;
        // remove TYPEDEF_STRUCTUREs
        while updated {
            updated = false;
            let mut idx = 0;
            while idx < self.typedef_structs.len() {
                let opt_info_vec = self.typedef_ref_info.get(&self.typedef_structs[idx].name);
                if opt_info_vec.is_none() || opt_info_vec.unwrap().is_empty() {
                    for sc in &self.typedef_structs[idx].structure_component {
                        if let Some(target_info) = self.typedef_ref_info.get_mut(&sc.component_type)
                        {
                            target_info.retain(|(_, referrer)| {
                                if let TypedefReferrer::StructureComponent(s, _) = referrer {
                                    *s != self.typedef_structs[idx].name
                                } else {
                                    true
                                }
                            });
                            if target_info.is_empty() {
                                self.typedef_ref_info.remove(&sc.component_type);
                            }
                        }
                    }
                    self.log_msgs.push(format!(
                        "removing unused TYPEDEF_STRUCTURE {}",
                        self.typedef_structs[idx].name
                    ));
                    self.typedef_structs.swap_remove_index(idx);
                } else {
                    idx += 1;
                }
            }
        }

        // remove TYPEDEF_CHARACTERISTICs
        let mut idx = 0;
        while idx < self.module.typedef_characteristic.len() {
            let opt_info_vec = self
                .typedef_ref_info
                .get(&self.module.typedef_characteristic[idx].name);
            if opt_info_vec.is_none() || opt_info_vec.unwrap().is_empty() {
                self.log_msgs.push(format!(
                    "removing unused TYPEDEF_CHARACTERISTIC {}",
                    self.module.typedef_characteristic[idx].name
                ));
                self.module.typedef_characteristic.swap_remove(idx);
            } else {
                idx += 1;
            }
        }

        // remove TYPEDEF_MEASUREMENTs
        idx = 0;
        while idx < self.module.typedef_measurement.len() {
            let opt_info_vec = self
                .typedef_ref_info
                .get(&self.module.typedef_measurement[idx].name);
            if opt_info_vec.is_none() || opt_info_vec.unwrap().is_empty() {
                self.log_msgs.push(format!(
                    "removing unused TYPEDEF_MEASUREMENT {}",
                    self.module.typedef_measurement[idx].name
                ));
                self.module.typedef_measurement.swap_remove(idx);
            } else {
                idx += 1;
            }
        }
    }
}

/// take the type name from a `SYMBOL_TYPE_LINK` and try to find a matching type in the `debug_data`
fn get_typeinfo_from_symbol_link<'dbg>(
    debug_data: &'dbg DebugData,
    stlink: &Option<SymbolTypeLink>,
) -> Option<&'dbg TypeInfo> {
    let symlink: &SymbolTypeLink = stlink.as_ref()?;
    let symbol_type_string: &String = &symlink.symbol_type;

    // Vector has invented a new notation:
    //    "TypeName{CompileUnit:foo_c}{Namespace:Global}"
    // The typereader does not extract namespace information from the debug data, but we can
    // try to match up the compile unit name.
    let mut symbol_type_parts = symbol_type_string.split('{');
    let symbol_type = symbol_type_parts.next().unwrap();
    let mut required_compile_unit = None;
    for additional_info in symbol_type_parts {
        if let Some(comp_unit) = additional_info.strip_prefix("CompileUnit:") {
            if let Some(comp_unit) = comp_unit.strip_suffix('}') {
                required_compile_unit = Some(comp_unit);
            }
        }
    }

    let typeinfo_list = debug_data.typenames.get(symbol_type)?;
    match typeinfo_list.len() {
        0 => None,
        1 => debug_data.types.get(&typeinfo_list[0]),
        _ => {
            // multiple types found for the target type name
            if let Some(req_compile_unit) = required_compile_unit {
                // try to find the correct type by comparing the compile unit name
                for typinfo_idx in typeinfo_list {
                    if let Some(typeinfo) = debug_data.types.get(typinfo_idx) {
                        if let Some(simple_name) =
                            make_simple_unit_name(debug_data, typeinfo.unit_idx)
                        {
                            if simple_name == req_compile_unit {
                                return Some(typeinfo);
                            }
                        }
                    }
                }
                // type was not identified by matching the compile unit name
                debug_data.types.get(&typeinfo_list[0])
            } else {
                // there is no infomation about the comile unit name
                debug_data.types.get(&typeinfo_list[0])
            }
        }
    }
}

/// find the typeinfo for a `STRUCTURE_COMPONENT`
fn get_structure_component_typeinfo<'dbg>(
    debug_data: &'dbg DebugData,
    structure_component: &StructureComponent,
    members: &'dbg IndexMap<String, (TypeInfo, u64)>,
) -> Option<&'dbg TypeInfo> {
    let symtypelink = structure_component.symbol_type_link.as_ref()?;
    // get the member type info - this is a TypeRef when referring to another struct / etc.
    let typeinfo_ref = &members.get(&symtypelink.symbol_type)?.0;
    // dereference the TypeRef (if any)
    let full_typeinfo = typeinfo_ref.get_reference(&debug_data.types);
    // follow the pointer (if any)
    let pointer_deref = full_typeinfo
        .get_pointer(&debug_data.types)
        .map_or(full_typeinfo, |(_, t)| t);
    // unwrap the array member type (if any)
    let array_deref = pointer_deref.get_arraytype().unwrap_or(pointer_deref);
    Some(array_deref)
}

/// convert a full unit name, which might include a path, into a simple unit name
/// usable in a `SYMBOL_TYPE_LINK`.
fn make_simple_unit_name(debug_data: &DebugData, unit_idx: usize) -> Option<String> {
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

/// is the given typeinfo suitable to use for a `TYPEDEF_STRUCTURE`?
fn is_structure_typeinfo(typeinfo: &TypeInfo, types: &HashMap<usize, TypeInfo>) -> bool {
    let typeinfo = typeinfo.get_pointer(types).map_or(typeinfo, |(_, t)| t);
    match &typeinfo.datatype {
        DwarfDataType::Pointer(_, offset) => {
            if let Some(pt_type) = types.get(&offset.0) {
                // inner type can be a pointer to anything, or a valid structure datatype
                matches!(&pt_type.datatype, DwarfDataType::Pointer(_, _))
                    || is_structure_typeinfo(pt_type, types)
            } else {
                false
            }
        }
        DwarfDataType::TypeRef(offset, _) => {
            if let Some(pt_type) = types.get(offset) {
                is_structure_typeinfo(pt_type, types)
            } else {
                false
            }
        }
        DwarfDataType::Struct { .. }
        | DwarfDataType::Class { .. }
        | DwarfDataType::Union { .. }
        | DwarfDataType::Array { .. } => true,
        _ => false,
    }
}

/// is the given typeinfo suitable to use for a `TYPEDEF_CHARACTERISTIC`?
fn is_calibration_typeinfo(typeinfo: &TypeInfo) -> bool {
    // TYPEDEF_CHARACTERISTIC has MATRIX_DIM, but no ADDRESS_TYPE, so only try to get the arraytype
    let typeinfo = typeinfo.get_arraytype().unwrap_or(typeinfo);
    !matches!(
        &typeinfo.datatype,
        DwarfDataType::Pointer(_, _)
            | DwarfDataType::FuncPtr(_)
            | DwarfDataType::Other(_)
            | DwarfDataType::Union { .. }
            | DwarfDataType::TypeRef(_, _)
    )
}

/// is the given typeinfo suitable to use for a `TYPEDEF_MEASUREMENT`?
fn is_measurement_typeinfo(typeinfo: &TypeInfo, types: &HashMap<usize, TypeInfo>) -> bool {
    let typeinfo = typeinfo.get_pointer(types).map_or(typeinfo, |(_, t)| t);
    let typeinfo = typeinfo.get_arraytype().unwrap_or(typeinfo);
    match &typeinfo.datatype {
        DwarfDataType::Pointer(_, offset) => {
            if let Some(pt_type) = types.get(&offset.0) {
                // inner type must be a measurement type, except it can't be a pointer itself
                !matches!(&pt_type.datatype, DwarfDataType::Pointer(_, _))
                    && is_measurement_typeinfo(pt_type, types)
            } else {
                false
            }
        }
        DwarfDataType::Other(_)
        | DwarfDataType::Struct { .. }
        | DwarfDataType::Class { .. }
        | DwarfDataType::Union { .. }
        | DwarfDataType::Array { .. }
        | DwarfDataType::TypeRef(_, _) => false,
        _ => true,
    }
}

/// if there are multiple types with the same name, do we need to use the Vector naming
/// extension to distingush between them?
/// A qualifier {`CompileUnit`:...} is not needed if all of the types are actually identical.
fn is_type_discriminant_needed(debug_data: &DebugData, name: &String) -> bool {
    let type_offsets = debug_data.typenames.get(name).unwrap();
    if type_offsets.len() < 2 {
        return false;
    }
    let type_list: Vec<_> = type_offsets
        .iter()
        .filter_map(|t_off| debug_data.types.get(t_off))
        .collect();
    let mut distinct_types = Vec::<&TypeInfo>::new();
    for t in &type_list {
        if !distinct_types
            .iter()
            .any(|typeinfo2| t.compare(typeinfo2, &debug_data.types))
        {
            distinct_types.push(t);
        }
    }
    distinct_types.len() != 1
}

/// get the size information for a TYPEDEF
/// this is different from `typeinfo.get_size()` for pointers, because it gives the size
/// of the pointer instead of the size of the pointer target.
fn get_typedef_size(debug_data: &DebugData, typeinfo: &TypeInfo) -> u32 {
    if let Some((_, pt_type)) = typeinfo.get_pointer(&debug_data.types) {
        pt_type.get_size() as u32
    } else {
        typeinfo.get_size() as u32
    }
}

/// calc the number of structurally distinct types in a `typedef_ref_info` Vec
fn calc_distinct_types<'a>(
    ref_info: &Vec<(Option<&'a TypeInfo>, TypedefReferrer)>,
    debug_data: &'a DebugData,
) -> Vec<&'a TypeInfo> {
    let mut distinct_types: Vec<&TypeInfo> = vec![];
    for (typeinfo_opt, _) in ref_info {
        if let Some(typeinfo) = typeinfo_opt {
            if !distinct_types
                .iter()
                .any(|typeinfo2| typeinfo.compare(typeinfo2, &debug_data.types))
            {
                distinct_types.push(typeinfo);
            }
        }
    }
    distinct_types
}

/// create a suitable name for a TYPEDEF_* based on the given typeinfo
fn make_typedef_name(debug_data: &DebugData, typeinfo: &TypeInfo, is_calib: bool) -> String {
    match &typeinfo.datatype {
        DwarfDataType::Pointer(pt_size, pt_dbg_offset) => {
            let prefix = match pt_size {
                1 => "BytePointer",
                2 => "ShortPointer",
                4 => "LongPointer",
                8 => "LongLongPointer",
                _ => "Pointer",
            };
            let basename: Cow<str> = if let Some(pt_type) = debug_data.types.get(&pt_dbg_offset.0) {
                make_typedef_name(debug_data, pt_type, is_calib).into()
            } else if let Some(pt_name) = &typeinfo.name {
                pt_name.into()
            } else {
                "unknown".into()
            };
            format!("{prefix}_{basename}")
        }
        DwarfDataType::Array { dim, arraytype, .. } => {
            let basename = make_typedef_name(debug_data, arraytype, is_calib);
            // ex: dim = [3, 4, 5] -> "Array_3_4_5"
            let mut outstr = dim.iter().fold("Array".to_string(), |mut txt, val| {
                write!(txt, "_{val}").unwrap();
                txt
            });
            outstr.push('_');
            outstr.push_str(&basename);
            outstr
        }
        DwarfDataType::Struct { .. } => typeinfo
            .name
            .as_deref()
            .unwrap_or("_unnamed_struct_")
            .to_string(),
        DwarfDataType::Class { .. } => {
            // there is no such thing as an unnamed class
            typeinfo.name.clone().unwrap()
        }
        DwarfDataType::Union { .. } => typeinfo
            .name
            .as_deref()
            .unwrap_or("_unnamed_union_")
            .to_string(),
        DwarfDataType::Enum { .. } => typeinfo
            .name
            .as_deref()
            .unwrap_or("_unnamed_enum_")
            .to_string(),
        DwarfDataType::TypeRef(offset, _) => debug_data
            .types
            .get(offset)
            .map_or("_invalid_reference_".to_string(), |t| {
                make_typedef_name(debug_data, t, is_calib)
            }),
        DwarfDataType::FuncPtr(_) | DwarfDataType::Other(_) => {
            // BLOBs might refer to void pointers, which can be represented as Other()
            typeinfo
                .name
                .as_deref()
                .unwrap_or("_unnamed_item_")
                .to_string()
        }
        DwarfDataType::Uint8 => make_basic_name(is_calib, "UByte"),
        DwarfDataType::Uint16 => make_basic_name(is_calib, "UWord"),
        DwarfDataType::Uint32 => make_basic_name(is_calib, "ULong"),
        DwarfDataType::Uint64 => make_basic_name(is_calib, "UInt64"),
        DwarfDataType::Sint8 => make_basic_name(is_calib, "SByte"),
        DwarfDataType::Sint16 => make_basic_name(is_calib, "SWord"),
        DwarfDataType::Sint32 => make_basic_name(is_calib, "SLong"),
        DwarfDataType::Sint64 => make_basic_name(is_calib, "SInt64"),
        DwarfDataType::Float => make_basic_name(is_calib, "Float32"),
        DwarfDataType::Double => make_basic_name(is_calib, "Double"),
        DwarfDataType::Bitfield {
            basetype,
            bit_offset,
            bit_size,
        } => {
            let basename = make_typedef_name(debug_data, basetype, is_calib);
            let mask: u64 = ((1 << bit_size) - 1) << bit_offset;
            format!("{basename}_0x{mask:X}")
        }
    }
}

fn make_basic_name(is_calib: bool, datatype: &str) -> String {
    if is_calib {
        format!("Parameter_{datatype}")
    } else {
        format!("Measurement_{datatype}")
    }
}

/// check if a typeinfo is suitable for use in a `STRUCTURE_COMPONENT`
fn fully_unwrap_typeinfo<'dbg>(
    debug_data: &'dbg DebugData,
    typeinfo: &'dbg TypeInfo,
) -> Option<&'dbg TypeInfo> {
    let mut cur_typeinfo = typeinfo;
    // fully unwrap all indirections, until the type is not one of Pointer / Array / TypeRef
    loop {
        match &cur_typeinfo.datatype {
            DwarfDataType::Pointer(_, off) => {
                // for void* the off may be 0, then debug_data.types.get() fails
                if let Some(ptype) = debug_data.types.get(&off.0) {
                    cur_typeinfo = ptype;
                } else {
                    return None;
                }
            }
            DwarfDataType::Array { arraytype, .. } => {
                cur_typeinfo = arraytype;
            }
            DwarfDataType::TypeRef(off, _) => {
                if let Some(reftype) = debug_data.types.get(off) {
                    cur_typeinfo = reftype;
                } else {
                    return None;
                }
            }
            _ => return Some(cur_typeinfo),
        }
    }
}

#[cfg(test)]
mod test {
    use super::{update_module_typedefs, TypedefUpdater};
    use crate::{
        dwarf::{DebugData, TypeInfo},
        update::{get_symbol_info, RecordLayoutInfo, TypedefNames, TypedefReferrer},
    };
    use a2lfile::A2lFile;
    use std::{
        collections::{HashMap, HashSet},
        ffi::OsString,
    };

    fn test_setup(
        a2l_name: &str,
        elf_name: &str,
    ) -> (A2lFile, DebugData, TypedefNames, RecordLayoutInfo) {
        let mut log_msgs = Vec::new();
        let a2l = a2lfile::load(a2l_name, None, &mut log_msgs, true).unwrap();
        let debug_data = crate::dwarf::DebugData::load(&OsString::from(elf_name), false).unwrap();
        let typedef_names = TypedefNames::new(&a2l.project.module[0]);
        let recordlayout_info = RecordLayoutInfo::build(&a2l.project.module[0]);
        (a2l, debug_data, typedef_names, recordlayout_info)
    }

    #[test]
    fn test_calc_structure_category() {
        let (mut a2l, debug_data, names, mut reclayout) =
            test_setup("tests/update_test1.a2l", "tests/elffiles/update_test.elf");
        let mut msgs = Vec::new();
        let mut tdu = TypedefUpdater::new(
            &mut a2l.project.module[0],
            &debug_data,
            &mut msgs,
            names,
            &mut reclayout,
            HashMap::new(),
        );

        tdu.typedef_names.structure = HashSet::new();
        tdu.calc_structure_category();

        assert!(!(*tdu.is_calib_struct.get("StructA").unwrap()));
        assert!(!(*tdu.is_calib_struct.get("StructB").unwrap()));
        assert!(!(*tdu.is_calib_struct.get("RegDef").unwrap()));
        assert!(!(*tdu.is_calib_struct.get("TestStruct").unwrap()));
        assert!(!(*tdu.is_calib_struct.get("LongPointer_TestStruct").unwrap()));
        assert!(
            !(*tdu
                .is_calib_struct
                .get("LongPointer_Array_10_TestStruct")
                .unwrap())
        );
        assert!(!(*tdu.is_calib_struct.get("DeadEnd").unwrap()));
        assert!(!(*tdu.is_calib_struct.get("DeadEnd2").unwrap()));
        assert!(tdu.is_calib_struct.get("Unconnected").is_none());
    }

    #[test]
    fn test_build_structure_hash() {
        let (mut a2l, debug_data, names, mut reclayout) =
            test_setup("tests/update_test1.a2l", "tests/elffiles/update_test.elf");
        let num_structs = a2l.project.module[0].typedef_structure.len();
        let mut msgs = Vec::new();
        let mut tdu = TypedefUpdater::new(
            &mut a2l.project.module[0],
            &debug_data,
            &mut msgs,
            names,
            &mut reclayout,
            HashMap::new(),
        );

        tdu.typedef_names.structure = HashSet::new();
        tdu.build_structure_hash();

        assert!(tdu.typedef_structs.contains_key("StructA"));
        assert!(tdu.typedef_structs.contains_key("StructB"));
        assert!(tdu.typedef_structs.contains_key("RegDef"));
        assert!(tdu.typedef_structs.contains_key("TestStruct"));
        assert!(tdu.typedef_structs.contains_key("LongPointer_TestStruct"));
        assert!(tdu
            .typedef_structs
            .contains_key("LongPointer_Array_10_TestStruct"));

        assert!(tdu.preserved_structs.contains_key("Unconnected"));
        assert!(tdu.preserved_structs.contains_key("DeadEnd"));
        assert!(tdu.preserved_structs.contains_key("DeadEnd2"));

        assert_eq!(
            num_structs,
            tdu.typedef_structs.len() + tdu.preserved_structs.len()
        );
    }

    #[test]
    fn test_process_structure_components() {
        let (mut a2l, debug_data, names, mut reclayout) =
            test_setup("tests/update_test2.a2l", "tests/elffiles/update_test.elf");
        let mut msgs = Vec::new();
        let mut tdu = TypedefUpdater::new(
            &mut a2l.project.module[0],
            &debug_data,
            &mut msgs,
            names,
            &mut reclayout,
            HashMap::new(),
        );

        tdu.typedef_names.structure = HashSet::new();
        tdu.calc_structure_category();
        tdu.build_structure_hash();

        // StructA was not placed in typedef_structs, because its SYMBOL_TYPE_LINK is invalid
        assert!(!tdu.typedef_structs.contains_key("StructA"));
        assert!(tdu.preserved_structs.contains_key("StructA"));

        tdu.process_structure_components(false);

        // StructA was restored into typedef_structs, because StructB links to it
        let struct_a = tdu.typedef_structs.get("StructA").unwrap();
        let struct_b = tdu.typedef_structs.get("StructB").unwrap();
        // the "nonexistent_nothing" STRUCTURE_COMPONENTs in both structs were removed
        assert!(!struct_a
            .structure_component
            .iter()
            .any(|sc| sc.component_name == "nonexistent_nothing"));
        assert!(!struct_b
            .structure_component
            .iter()
            .any(|sc| sc.component_name == "nonexistent_nothing"));
    }

    #[test]
    fn test_create_missing_instance_targets() {
        let mut a2l = a2lfile::new();
        let elf_name = OsString::from("tests/elffiles/update_test.elf");
        let debug_data = crate::dwarf::DebugData::load(&elf_name, false).unwrap();
        let typedef_names = TypedefNames::new(&a2l.project.module[0]);
        let mut recordlayout_info = RecordLayoutInfo::build(&a2l.project.module[0]);

        // start with an empty a2l file and create an INSTANCE manually
        let mut instance = a2lfile::Instance::new(
            "struct_b".to_string(),
            String::new(),
            "unknown_type".to_string(),
            0,
        );
        instance.symbol_link = Some(a2lfile::SymbolLink::new("struct_b".to_string(), 0));
        a2l.project.module[0].instance.push(instance);

        let structb_typeinfo = debug_data
            .types
            .get(&debug_data.typenames.get("StructB").unwrap()[0])
            .unwrap();

        // the typedef_ref_info for INSTANCEs is normally collected by the INSTANCE update function
        let mut typedef_ref_info: HashMap<_, Vec<_>> = HashMap::new();
        typedef_ref_info
            .entry("StructB".to_string())
            .or_default()
            .push((Some(structb_typeinfo), TypedefReferrer::Instance(0)));

        let mut msgs = Vec::new();
        let mut tdu = TypedefUpdater::new(
            &mut a2l.project.module[0],
            &debug_data,
            &mut msgs,
            typedef_names,
            &mut recordlayout_info,
            typedef_ref_info,
        );

        tdu.typedef_names.structure = HashSet::new();
        tdu.calc_structure_category();
        tdu.build_structure_hash();

        assert_eq!(tdu.typedef_structs.len(), 0);

        tdu.create_missing_instance_targets();

        // the missing struct StructB and its dependency StructA were created
        assert_eq!(tdu.typedef_structs.len(), 2);
        assert!(tdu.typedef_structs.get("StructA").is_some());
        assert!(tdu.typedef_structs.get("StructB").is_some());
    }

    #[test]
    fn test_create_typedef() {
        let mut a2l = a2lfile::new();
        let elf_name = OsString::from("tests/elffiles/update_test.elf");
        let debug_data = crate::dwarf::DebugData::load(&elf_name, false).unwrap();
        let typedef_names = TypedefNames::new(&a2l.project.module[0]);
        let mut recordlayout_info = RecordLayoutInfo::build(&a2l.project.module[0]);
        let mut msgs = Vec::new();
        let mut tdu = TypedefUpdater::new(
            &mut a2l.project.module[0],
            &debug_data,
            &mut msgs,
            typedef_names,
            &mut recordlayout_info,
            HashMap::new(),
        );
        let mut enum_convlist = HashMap::<String, &TypeInfo>::new();

        tdu.typedef_names.structure = HashSet::new();
        tdu.calc_structure_category();
        tdu.build_structure_hash();
        tdu.process_structure_components(false);

        assert!(tdu.typedef_structs.is_empty());

        // get the typeinfo for StructA
        let typeinfo = debug_data
            .types
            .get(&debug_data.typenames.get("StructA").unwrap()[0])
            .unwrap();
        // create the TYPEDEF_STRUCTURE for StructA - calibration
        let name = tdu
            .create_typedef(typeinfo, true, &mut enum_convlist)
            .unwrap();
        assert_eq!(name, "StructA");
        assert!(tdu.typedef_structs.contains_key("StructA"));

        // get the typeinfo for StructB
        let typeinfo = debug_data
            .types
            .get(&debug_data.typenames.get("StructB").unwrap()[0])
            .unwrap();
        // create the TYPEDEF_STRUCTURE for StructB - calibration
        let name = tdu
            .create_typedef(typeinfo, true, &mut enum_convlist)
            .unwrap();
        assert_eq!(name, "StructB");
        assert!(tdu.typedef_structs.contains_key("StructB"));

        // currently only StructA and StructB exist
        assert_eq!(tdu.typedef_structs.len(), 2);

        // create the TYPEDEF_STRUCTURE for StructB - calibration again
        let name = tdu
            .create_typedef(typeinfo, true, &mut enum_convlist)
            .unwrap();
        assert_eq!(name, "StructB");
        // this only returned the name of the existing copy, and nothing was created
        assert_eq!(tdu.typedef_structs.len(), 2);

        // create the TYPEDEF_STRUCTURE for StructB - measurement
        let name = tdu
            .create_typedef(typeinfo, false, &mut enum_convlist)
            .unwrap();
        assert_eq!(name, "StructB_Copy1");
        assert!(tdu.typedef_structs.contains_key("StructB_Copy1"));
        // a second copy of StructA should also have been created, since the
        // existing one is for calibration, not measurement
        assert!(tdu.typedef_structs.contains_key("StructA_Copy1"));

        assert_eq!(tdu.typedef_structs.len(), 4);
    }

    #[test]
    fn test_create_typedef2() {
        let mut a2l = a2lfile::new();
        let elf_name = OsString::from("tests/elffiles/update_test.elf");
        let debug_data = crate::dwarf::DebugData::load(&elf_name, false).unwrap();
        let typedef_names = TypedefNames::new(&a2l.project.module[0]);
        let mut recordlayout_info = RecordLayoutInfo::build(&a2l.project.module[0]);
        let mut msgs = Vec::new();
        let mut tdu = TypedefUpdater::new(
            &mut a2l.project.module[0],
            &debug_data,
            &mut msgs,
            typedef_names,
            &mut recordlayout_info,
            HashMap::new(),
        );
        let mut enum_convlist = HashMap::<String, &TypeInfo>::new();

        tdu.typedef_names.structure = HashSet::new();
        tdu.calc_structure_category();
        tdu.build_structure_hash();
        tdu.process_structure_components(false);

        assert_eq!(tdu.module.typedef_blob.len(), 0);

        // get the typeinfo for variable val_ptr, a void pointer
        let typeinfo = debug_data
            .types
            .get(&debug_data.variables.get("val_ptr").unwrap().typeref)
            .unwrap();
        // create the TYPEDEF_STRUCTURE for StructA - calibration
        let name = tdu
            .create_typedef(typeinfo, true, &mut enum_convlist)
            .unwrap();
        assert_eq!(name, "LongPointer_void");
        assert_eq!(tdu.typedef_structs.len(), 0);
        assert_eq!(tdu.module.typedef_blob.len(), 1);
    }

    #[test]
    fn test_update() {
        let (mut a2l, debug_data, names, mut reclayout) =
            test_setup("tests/update_test3.a2l", "tests/elffiles/update_test.elf");

        let mut typedef_ref_info: HashMap<String, Vec<_>> = HashMap::new();
        for (idx, inst) in a2l.project.module[0].instance.iter().enumerate() {
            if let Ok(sym_info) =
                get_symbol_info(&inst.name, &inst.symbol_link, &inst.if_data, &debug_data)
            {
                let typeinfo = sym_info
                    .typeinfo
                    .get_pointer(&debug_data.types)
                    .map_or(sym_info.typeinfo, |(_, t)| t);
                let typeinfo = typeinfo.get_arraytype().unwrap_or(typeinfo);
                typedef_ref_info
                    .entry(inst.type_ref.clone())
                    .or_default()
                    .push((Some(typeinfo), TypedefReferrer::Instance(idx)));
            }
        }

        let mut log_msgs = Vec::new();
        update_module_typedefs(
            &mut a2l.project.module[0],
            &debug_data,
            &mut log_msgs,
            false,
            typedef_ref_info,
            names,
            &mut reclayout,
        );

        let mut log_msgs = Vec::new();
        let mut reference_a2l =
            a2lfile::load("tests/update_test4.a2l", None, &mut log_msgs, true).unwrap();

        // ordering is not guaranteed, so sort both files before comparing them
        a2l.sort();
        reference_a2l.sort();

        assert_eq!(a2l, reference_a2l);
    }
}
