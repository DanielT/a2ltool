use crate::A2lVersion;
use crate::datatype::get_a2l_datatype;
use crate::debuginfo::DbgDataType;
use crate::debuginfo::{DebugData, TypeInfo};
use crate::symbol::SymbolInfo;
use a2lfile::{
    A2lObject, A2lObjectName, AxisDescr, Characteristic, CharacteristicType, ItemList, Module,
    RecordLayout,
};
use std::collections::HashMap;
use std::collections::HashSet;

use crate::update::{
    A2lUpdateInfo, A2lUpdater, UpdateResult, adjust_limits, cleanup_item_list,
    enums::{cond_create_enum_conversion, update_enum_compu_methods},
    get_fnc_values_memberid, get_inner_type, get_symbol_info,
    ifdata_update::{update_ifdata_address, update_ifdata_type, zero_if_data},
    make_symbol_link_string, set_bitmask, set_matrix_dim, set_symbol_link, update_record_layout,
};

// update all CHARACTERISTICs in the module
pub(crate) fn update_all_module_characteristics(
    data: &mut A2lUpdater,
    info: &A2lUpdateInfo<'_>,
) -> Vec<UpdateResult> {
    let mut enum_convlist = HashMap::<String, &TypeInfo>::new();
    let mut removed_items = HashSet::<String>::new();
    let mut characteristic_list = ItemList::new();
    // let mut characteristic_updated: u32 = 0;
    // let mut characteristic_not_updated: u32 = 0;
    let mut results = vec![];

    // store the max_axis_points of each AXIS_PTS, so that the AXIS_DESCRs inside of CHARACTERISTICS can be updated to match
    let axis_pts_dim: HashMap<String, u16> = data
        .module
        .axis_pts
        .iter()
        .map(|item| (item.get_name().to_string(), item.max_axis_points))
        .collect();

    std::mem::swap(&mut data.module.characteristic, &mut characteristic_list);
    for mut characteristic in characteristic_list {
        let update_result = update_module_characteristic(
            &mut characteristic,
            info,
            data,
            &mut enum_convlist,
            &axis_pts_dim,
        );
        if matches!(update_result, UpdateResult::SymbolNotFound { .. }) {
            if info.preserve_unknown {
                characteristic.address = 0;
                zero_if_data(&mut characteristic.if_data);
                data.module.characteristic.push(characteristic);
            } else {
                removed_items.insert(characteristic.get_name().to_string());
            }
        } else {
            data.module.characteristic.push(characteristic);
        }
        results.push(update_result);
    }

    // update COMPU_VTABs and COMPU_VTAB_RANGEs based on the data types used in CHARACTERISTICs
    update_enum_compu_methods(data.module, &enum_convlist);
    cleanup_removed_characteristics(data.module, &removed_items);

    results
}

// update a single CHARACTERISTIC object
fn update_module_characteristic<'dbg>(
    characteristic: &mut Characteristic,
    info: &A2lUpdateInfo<'dbg>,
    data: &mut A2lUpdater<'_>,
    enum_convlist: &mut HashMap<String, &'dbg TypeInfo>,
    axis_pts_dim: &HashMap<String, u16>,
) -> UpdateResult {
    if characteristic.virtual_characteristic.is_none() {
        // only update the address if the CHARACTERISTIC is not a VIRTUAL_CHARACTERISTIC
        match get_symbol_info(
            characteristic.get_name(),
            &characteristic.symbol_link,
            &characteristic.if_data,
            info.debug_data,
            info.use_new_arrays,
        ) {
            Ok(sym_info) => {
                update_characteristic_address(
                    characteristic,
                    info.debug_data,
                    info.version,
                    &sym_info,
                );

                update_ifdata_address(
                    &mut characteristic.if_data,
                    &sym_info.name,
                    sym_info.address,
                );

                if info.full_update {
                    // update the data type of the CHARACTERISTIC object
                    update_ifdata_type(&mut characteristic.if_data, sym_info.typeinfo);

                    // update as much as possible of the information inside the CHARACTERISTIC
                    update_characteristic_datatype(
                        data,
                        characteristic,
                        sym_info.typeinfo,
                        enum_convlist,
                        axis_pts_dim,
                        info.version >= A2lVersion::V1_7_0,
                    );
                    UpdateResult::Updated
                } else if info.strict_update {
                    // verify that the data type of the CHARACTERISTIC object is still correct
                    verify_characteristic_datatype(
                        data,
                        characteristic,
                        sym_info.typeinfo,
                        info.version >= A2lVersion::V1_7_0,
                    )
                } else {
                    // no type update, but the address was updated
                    UpdateResult::Updated
                }
            }
            Err(errors) => UpdateResult::SymbolNotFound {
                blocktype: "CHARACTERISTIC",
                name: characteristic.get_name().to_string(),
                line: characteristic.get_line(),
                errors,
            },
        }
    } else {
        // computed CHARACTERISTICS with a VIRTUAL_CHARACTERISTIC block shouldn't have an address and don't need to be updated
        UpdateResult::Updated
    }
}

// update the address of a CHARACTERISTIC
fn update_characteristic_address<'dbg>(
    characteristic: &mut Characteristic,
    debug_data: &'dbg DebugData,
    version: A2lVersion,
    sym_info: &SymbolInfo<'dbg>,
) {
    if version >= A2lVersion::V1_6_0 {
        // make sure a valid SYMBOL_LINK exists
        let symbol_link_text = make_symbol_link_string(sym_info, debug_data);
        set_symbol_link(&mut characteristic.symbol_link, symbol_link_text);
    } else {
        characteristic.symbol_link = None;
    }

    if characteristic.address == 0 {
        characteristic.get_layout_mut().item_location.3.1 = true;
    }
    characteristic.address = sym_info.address as u32;
}

// update as much as possible of the information inside the CHARACTERISTIC
fn update_characteristic_datatype<'enumlist, 'typeinfo: 'enumlist>(
    data: &mut A2lUpdater,
    characteristic: &mut Characteristic,
    typeinfo: &'typeinfo TypeInfo,
    enum_convlist: &'enumlist mut HashMap<String, &'typeinfo TypeInfo>,
    axis_pts_dim: &HashMap<String, u16>,
    use_new_matrix_dim: bool,
) {
    let member_id = get_fnc_values_memberid(data.module, &characteristic.deposit);
    if let Some(inner_typeinfo) = get_inner_type(typeinfo, member_id) {
        if let DbgDataType::Enum { enumerators, .. } = &inner_typeinfo.datatype {
            let enum_name = inner_typeinfo
                .name
                .clone()
                .unwrap_or_else(|| format!("{}_compu_method", characteristic.get_name()));
            if characteristic.conversion == "NO_COMPU_METHOD" {
                characteristic.conversion = enum_name;
            }
            cond_create_enum_conversion(data.module, &characteristic.conversion, enumerators);
            enum_convlist.insert(characteristic.conversion.clone(), inner_typeinfo);
        }

        let opt_compu_method = data.module.compu_method.get(&characteristic.conversion);
        let (ll, ul) = adjust_limits(
            inner_typeinfo,
            characteristic.lower_limit,
            characteristic.upper_limit,
            opt_compu_method,
        );
        characteristic.lower_limit = ll;
        characteristic.upper_limit = ul;

        set_bitmask(&mut characteristic.bit_mask, inner_typeinfo);
    }

    // Patch up incomplete characteristics: Curve, Map, Cuboid, Cube4 and Cube5 all require AXIS_DESCR to function correctly
    // More extensive validation is possible, but unlikely to be needed. Even this case only occurs in manually edited files
    if characteristic.axis_descr.is_empty()
        && (characteristic.characteristic_type == CharacteristicType::Curve
            || characteristic.characteristic_type == CharacteristicType::Map
            || characteristic.characteristic_type == CharacteristicType::Cuboid
            || characteristic.characteristic_type == CharacteristicType::Cube4
            || characteristic.characteristic_type == CharacteristicType::Cube5)
    {
        // AXIS_DESCR is missing, so try to use the characteristic as a VALUE (or VAL_BLK) instead
        characteristic.characteristic_type = CharacteristicType::Value;
    }

    // if the characteristic does not have any axes, update MATRIX_DIM and switch between types VALUE and VAL_BLK as needed
    if characteristic.characteristic_type == CharacteristicType::Value
        || characteristic.characteristic_type == CharacteristicType::ValBlk
    {
        set_matrix_dim(&mut characteristic.matrix_dim, typeinfo, use_new_matrix_dim);
        // arrays of values should have the type ValBlk, while single values should NOT have the type ValBlk
        if characteristic.characteristic_type == CharacteristicType::Value
            && characteristic.matrix_dim.is_some()
        {
            // change Value -> ValBlk
            characteristic.characteristic_type = CharacteristicType::ValBlk;
        } else if characteristic.characteristic_type == CharacteristicType::ValBlk
            && characteristic.matrix_dim.is_none()
        {
            // change ValBlk -> Value
            characteristic.characteristic_type = CharacteristicType::Value;
        }
        characteristic.number = None;
    } else {
        characteristic.matrix_dim = None;
    }

    let record_layout = if let Some(idx) = data.module.record_layout.index(&characteristic.deposit)
    {
        Some(&data.module.record_layout[idx])
    } else {
        None
    };

    update_characteristic_axis(
        &mut characteristic.axis_descr,
        record_layout,
        axis_pts_dim,
        typeinfo,
    );
    characteristic.deposit = update_record_layout(
        data.module,
        &mut data.reclayout_info,
        &characteristic.deposit,
        typeinfo,
    );
}

// update all the AXIS_DESCRs inside a CHARACTERISTIC (or TYPEDEF_CHARACTERISTIC)
// for the list of AXIS_DESCR the ordering matters: the first AXIS_DESCR describes the x axis, the second describes the y axis, etc.
pub(crate) fn update_characteristic_axis(
    axis_descr: &mut [AxisDescr],
    record_layout: Option<&RecordLayout>,
    axis_pts_dim: &HashMap<String, u16>,
    typeinfo: &TypeInfo,
) {
    let mut axis_positions = Vec::<Option<u16>>::new();
    // record_layout can only be None if the file is damaged. The spec requires a reference to a valid RECORD_LAYOUT
    if let Some(rl) = record_layout {
        let itemrefs = [
            &rl.axis_pts_x,
            &rl.axis_pts_y,
            &rl.axis_pts_z,
            &rl.axis_pts_4,
            &rl.axis_pts_5,
        ];
        for itemref in &itemrefs {
            // the record_layout only describes axes that are internal, i.e. part of the characteristic data structure
            // external axes are described by AXIS_PTS records instead
            if let Some(axisinfo) = itemref {
                // axis information for this axis exists
                axis_positions.push(Some(axisinfo.position));
            } else {
                // no axis information found
                axis_positions.push(None);
            }
        }
    }
    for (idx, axis_descr) in axis_descr.iter_mut().enumerate() {
        if let Some(axis_pts_ref) = &axis_descr.axis_pts_ref {
            // external axis, using AXIS_PTS_REF
            if let Some(max_axis_pts) = axis_pts_dim.get(&axis_pts_ref.axis_points) {
                axis_descr.max_axis_points = *max_axis_pts;
            }
        } else if idx <= 5 {
            // an internal axis, using info from the typeinfo and the record layout
            if let Some(&Some(position)) = axis_positions.get(idx)
                && let Some(TypeInfo {
                    datatype: DbgDataType::Array { dim, .. },
                    ..
                }) = get_inner_type(typeinfo, position)
            {
                axis_descr.max_axis_points = dim[0] as u16;
            }
        }
    }
}

fn verify_characteristic_datatype(
    data: &mut A2lUpdater,
    characteristic: &Characteristic,
    typeinfo: &TypeInfo,
    use_new_matrix_dim: bool,
) -> UpdateResult {
    let mut bad_characteristic = false;
    let member_id = get_fnc_values_memberid(data.module, &characteristic.deposit);
    if let Some(inner_typeinfo) = get_inner_type(typeinfo, member_id) {
        if let DbgDataType::Enum { .. } = &inner_typeinfo.datatype
            && characteristic.conversion == "NO_COMPU_METHOD"
        {
            bad_characteristic = true;
        }

        let opt_compu_method = data.module.compu_method.get(&characteristic.conversion);
        let (ll, ul) = adjust_limits(
            inner_typeinfo,
            characteristic.lower_limit,
            characteristic.upper_limit,
            opt_compu_method,
        );
        if ll != characteristic.lower_limit || ul != characteristic.upper_limit {
            bad_characteristic = true;
        }

        let mut dummy_bitmask = characteristic.bit_mask.clone();
        set_bitmask(&mut dummy_bitmask, inner_typeinfo);

        let mut dummy_matrix_dim = characteristic.matrix_dim.clone();
        match characteristic.characteristic_type {
            CharacteristicType::Value => {
                // a scalar value should not have a matrix dimension, either before or after the update
                set_matrix_dim(&mut dummy_matrix_dim, inner_typeinfo, use_new_matrix_dim);
                if dummy_matrix_dim.is_some()
                    || characteristic.matrix_dim.is_some()
                    || characteristic.number.is_some()
                {
                    bad_characteristic = true;
                }
            }
            CharacteristicType::ValBlk => {
                // the matrix dim of a ValBlk must exist and remain unchanged
                set_matrix_dim(&mut dummy_matrix_dim, inner_typeinfo, use_new_matrix_dim);
                if characteristic.matrix_dim.is_none()
                    || dummy_matrix_dim != characteristic.matrix_dim
                {
                    bad_characteristic = true;
                }
            }
            CharacteristicType::Map
            | CharacteristicType::Curve
            | CharacteristicType::Cuboid
            | CharacteristicType::Cube4
            | CharacteristicType::Cube5 => {
                // map ... cube5 should each have axis_descr describing their axes
                if characteristic.axis_descr.is_empty() {
                    bad_characteristic = true;
                }
            }
            CharacteristicType::Ascii => {
                // no extra checks for ASCII
            }
        }

        // check if the data type of the deposit record is correct
        // to do this, we need to look up the record layout, and get its fnc_values
        if let Some(fnc_values) = data
            .module
            .record_layout
            .get(&characteristic.deposit)
            .and_then(|rl| rl.fnc_values.as_ref())
        {
            let a2l_datatype = get_a2l_datatype(inner_typeinfo);
            if a2l_datatype != fnc_values.datatype {
                bad_characteristic = true;
            }
        } else {
            // no record layout found, or no fnc_values in the record layout: the characteristic is invalid
            bad_characteristic = true;
        }
    } else {
        // no inner type found: the characteristic is invalid
        bad_characteristic = true;
    }

    if bad_characteristic {
        UpdateResult::InvalidDataType {
            blocktype: "CHARACTERISTIC",
            name: characteristic.get_name().to_string(),
            line: characteristic.get_line(),
        }
    } else {
        UpdateResult::Updated
    }
}

// when update runs without preserve, CHARACTERISTICs could be removed from the module
// these items should also be removed from the identifier lists in GROUPs and FUNCTIONs
pub(crate) fn cleanup_removed_characteristics(
    module: &mut Module,
    removed_items: &HashSet<String>,
) {
    if removed_items.is_empty() {
        return;
    }

    for group in &mut module.group {
        if let Some(ref_characteristic) = &mut group.ref_characteristic {
            cleanup_item_list(&mut ref_characteristic.identifier_list, removed_items);
            if ref_characteristic.identifier_list.is_empty() {
                group.ref_characteristic = None;
            }
        }
    }

    for function in &mut module.function {
        if let Some(def_characteristic) = &mut function.def_characteristic {
            cleanup_item_list(&mut def_characteristic.identifier_list, removed_items);
            if def_characteristic.identifier_list.is_empty() {
                function.def_characteristic = None;
            }
        }
        if let Some(ref_characteristic) = &mut function.ref_characteristic {
            cleanup_item_list(&mut ref_characteristic.identifier_list, removed_items);
            if ref_characteristic.identifier_list.is_empty() {
                function.ref_characteristic = None;
            }
        }
    }
}
