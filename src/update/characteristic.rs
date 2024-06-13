use crate::dwarf::DwarfDataType;
use crate::dwarf::{DebugData, TypeInfo};
use crate::A2lVersion;
use a2lfile::{A2lObject, AxisDescr, Characteristic, CharacteristicType, Module, RecordLayout};
use std::collections::HashMap;
use std::collections::HashSet;

use crate::update::{
    adjust_limits, cleanup_item_list,
    enums::{cond_create_enum_conversion, update_enum_compu_methods},
    get_fnc_values_memberid, get_inner_type, get_symbol_info,
    ifdata_update::{update_ifdata, zero_if_data},
    log_update_errors, make_symbol_link_string, set_bitmask, set_matrix_dim, set_symbol_link,
    update_record_layout, RecordLayoutInfo, UpdateInfo,
};

pub(crate) fn update_module_characteristics(
    info: &mut UpdateInfo,
    compu_method_index: &HashMap<String, usize>,
) -> (u32, u32) {
    let mut enum_convlist = HashMap::<String, &TypeInfo>::new();
    let mut removed_items = HashSet::<String>::new();
    let mut characteristic_list = Vec::new();
    let mut characteristic_updated: u32 = 0;
    let mut characteristic_not_updated: u32 = 0;

    // store the max_axis_points of each AXIS_PTS, so that the AXIS_DESCRs inside of CHARACTERISTICS can be updated to match
    let axis_pts_dim: HashMap<String, u16> = info
        .module
        .axis_pts
        .iter()
        .map(|item| (item.name.clone(), item.max_axis_points))
        .collect();

    std::mem::swap(&mut info.module.characteristic, &mut characteristic_list);
    for mut characteristic in characteristic_list {
        if characteristic.virtual_characteristic.is_none() {
            // only update the address if the CHARACTERISTIC is not a VIRTUAL_CHARACTERISTIC
            match update_characteristic_address(&mut characteristic, info.debug_data, info.version)
            {
                Ok(typeinfo) => {
                    // update as much as possible of the information inside the CHARACTERISTIC
                    update_characteristic_information(
                        info.module,
                        &mut info.reclayout_info,
                        &mut characteristic,
                        typeinfo,
                        &mut enum_convlist,
                        &axis_pts_dim,
                        info.version >= A2lVersion::V1_7_0,
                        compu_method_index,
                    );

                    info.module.characteristic.push(characteristic);
                    characteristic_updated += 1;
                }
                Err(errmsgs) => {
                    log_update_errors(
                        info.log_msgs,
                        errmsgs,
                        "CHARACTERISTIC",
                        characteristic.get_line(),
                    );

                    if info.preserve_unknown {
                        characteristic.address = 0;
                        zero_if_data(&mut characteristic.if_data);
                        info.module.characteristic.push(characteristic);
                    } else {
                        // item is removed implicitly, because it is not added back to the list
                        removed_items.insert(characteristic.name.clone());
                    }
                    characteristic_not_updated += 1;
                }
            }
        } else {
            // computed CHARACTERISTICS with a VIRTUAL_CHARACTERISTIC block shouldn't have an address and don't need to be updated
            info.module.characteristic.push(characteristic);
        }
    }

    // update COMPU_VTABs and COMPU_VTAB_RANGEs based on the data types used in CHARACTERISTICs
    update_enum_compu_methods(info.module, &enum_convlist);
    cleanup_removed_characteristics(info.module, &removed_items);

    (characteristic_updated, characteristic_not_updated)
}

// update as much as possible of the information inside the CHARACTERISTIC
#[allow(clippy::too_many_arguments)]
fn update_characteristic_information<'enumlist, 'typeinfo: 'enumlist>(
    module: &mut Module,
    recordlayout_info: &mut RecordLayoutInfo,
    characteristic: &mut Characteristic,
    typeinfo: &'typeinfo TypeInfo,
    enum_convlist: &'enumlist mut HashMap<String, &'typeinfo TypeInfo>,
    axis_pts_dim: &HashMap<String, u16>,
    use_new_matrix_dim: bool,
    compu_method_index: &HashMap<String, usize>,
) {
    let member_id = get_fnc_values_memberid(module, recordlayout_info, &characteristic.deposit);
    if let Some(inner_typeinfo) = get_inner_type(typeinfo, member_id) {
        if let DwarfDataType::Enum { enumerators, .. } = &inner_typeinfo.datatype {
            let enum_name = inner_typeinfo
                .name
                .clone()
                .unwrap_or_else(|| format!("{}_compu_method", characteristic.name));
            if characteristic.conversion == "NO_COMPU_METHOD" {
                characteristic.conversion = enum_name;
            }
            cond_create_enum_conversion(module, &characteristic.conversion, enumerators);
            enum_convlist.insert(characteristic.conversion.clone(), inner_typeinfo);
        }

        let opt_compu_method = compu_method_index
            .get(&characteristic.conversion)
            .and_then(|idx| module.compu_method.get(*idx));
        let (ll, ul) = adjust_limits(
            inner_typeinfo,
            characteristic.lower_limit,
            characteristic.upper_limit,
            opt_compu_method,
        );
        characteristic.lower_limit = ll;
        characteristic.upper_limit = ul;
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

    let record_layout = if let Some(idx) = recordlayout_info.idxmap.get(&characteristic.deposit) {
        Some(&module.record_layout[*idx])
    } else {
        None
    };
    update_characteristic_axis(
        &mut characteristic.axis_descr,
        record_layout,
        axis_pts_dim,
        typeinfo,
    );
    characteristic.deposit =
        update_record_layout(module, recordlayout_info, &characteristic.deposit, typeinfo);
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
            if let Some(position) = axis_positions[idx] {
                if let Some(TypeInfo {
                    datatype: DwarfDataType::Array { dim, .. },
                    ..
                }) = get_inner_type(typeinfo, position)
                {
                    axis_descr.max_axis_points = dim[0] as u16;
                }
            }
        }
    }
}

// update the address of a CHARACTERISTIC
fn update_characteristic_address<'a>(
    characteristic: &mut Characteristic,
    debug_data: &'a DebugData,
    version: A2lVersion,
) -> Result<&'a TypeInfo, Vec<String>> {
    match get_symbol_info(
        &characteristic.name,
        &characteristic.symbol_link,
        &characteristic.if_data,
        debug_data,
    ) {
        Ok(sym_info) => {
            if version >= A2lVersion::V1_6_0 {
                // make sure a valid SYMBOL_LINK exists
                let symbol_link_text = make_symbol_link_string(&sym_info, debug_data);
                set_symbol_link(&mut characteristic.symbol_link, symbol_link_text);
            } else {
                characteristic.symbol_link = None;
            }

            characteristic.address = sym_info.address as u32;
            set_bitmask(&mut characteristic.bit_mask, sym_info.typeinfo);
            update_ifdata(
                &mut characteristic.if_data,
                &sym_info.name,
                sym_info.typeinfo,
                sym_info.address,
            );

            Ok(sym_info.typeinfo)
        }
        Err(errmsgs) => Err(errmsgs),
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
