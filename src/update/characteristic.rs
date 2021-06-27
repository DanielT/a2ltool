use crate::dwarf::*;
use a2lfile::*;
use std::collections::HashMap;
use std::collections::HashSet;

use super::enums::*;
use super::ifdata_update::*;
use super::*;


pub(crate) fn update_module_characteristics(
    module: &mut Module,
    debug_data: &DebugData,
    preserve_unknown: bool,
    recordlayout_info: &mut RecordLayoutInfo
) -> (u32, u32) {
    let mut enum_convlist = HashMap::<String, &TypeInfo>::new();
    let mut removed_items = HashSet::<String>::new();
    let mut characteristic_list = Vec::new();
    let mut characteristic_updated: u32 = 0;
    let mut characteristic_not_updated: u32 = 0;

    // store the max_axis_points of each AXIS_PTS, so that the AXIS_DESCRs inside of CHARACTERISTICS can be updated to match
    let axis_pts_dim: HashMap::<String, u16> = module.axis_pts
        .iter()
        .map(|item| (item.name.to_owned(), item.max_axis_points))
        .collect();

    std::mem::swap(&mut module.characteristic, &mut characteristic_list);
    for mut characteristic in characteristic_list {
        if characteristic.virtual_characteristic.is_none() {
            // only update the address if the CHARACTERISTIC is not a VIRTUAL_CHARACTERISTIC
            if let Some(typeinfo) = update_characteristic_address(&mut characteristic, debug_data) {
                // update as much as possible of the information inside the CHARACTERISTIC
                update_characteristic_information(
                    module,
                    recordlayout_info,
                    &mut characteristic,
                    typeinfo,
                    &mut enum_convlist,
                    &axis_pts_dim
                );

                module.characteristic.push(characteristic);
                characteristic_updated += 1;
            } else {
                if preserve_unknown {
                    characteristic.address = 0;
                    zero_if_data(&mut characteristic.if_data);
                    module.characteristic.push(characteristic);
                } else {
                    // item is removed implicitly, because it is not added back to the list
                    removed_items.insert(characteristic.name.to_owned());
                }
                characteristic_not_updated += 1;
            }
        } else {
            // computed CHARACTERISTICS with a VIRTUAL_CHARACTERISTIC block shouldn't have an address and don't need to be updated
            module.characteristic.push(characteristic);
        }
    }

    // update COMPU_VTABs and COMPU_VTAB_RANGEs based on the data types used in CHARACTERISTICs
    update_enum_compu_methods(module, &enum_convlist);
    cleanup_removed_characteristics(module, &removed_items);

    (characteristic_updated, characteristic_not_updated)
}


// update as much as possible of the information inside the CHARACTERISTIC
fn update_characteristic_information<'enumlist, 'typeinfo: 'enumlist>(
    module: &mut Module,
    recordlayout_info: &mut RecordLayoutInfo,
    characteristic: &mut Characteristic,
    typeinfo: &'typeinfo TypeInfo,
    enum_convlist: &'enumlist mut HashMap<String, &'typeinfo TypeInfo>,
    axis_pts_dim: &HashMap<String, u16>
) {
    let member_id = get_fnc_values_memberid(module, recordlayout_info, &characteristic.deposit);
    if let Some(inner_typeinfo) = get_inner_type(typeinfo, member_id) {
        if let TypeInfo::Enum{typename, enumerators, ..} = inner_typeinfo {
            if characteristic.conversion == "NO_COMPU_METHOD" {
                characteristic.conversion = typename.to_owned();
            }
            cond_create_enum_conversion(module, &characteristic.conversion, enumerators);
            enum_convlist.insert(characteristic.conversion.clone(), typeinfo);
        }

        let (ll, ul) = adjust_limits(
            inner_typeinfo,
            characteristic.lower_limit,
            characteristic.upper_limit
        );
        characteristic.lower_limit = ll;
        characteristic.upper_limit = ul;
    }
    let record_layout = if let Some(idx) = recordlayout_info.idxmap.get(&characteristic.deposit) {
        Some(&module.record_layout[*idx])
    } else {
        None
    };
    update_characteristic_axis(
        &mut characteristic.axis_descr,
        record_layout,
        &axis_pts_dim,
        typeinfo
    );
    characteristic.deposit = update_record_layout(module, recordlayout_info, &characteristic.deposit, typeinfo);
}


// update all the AXIS_DESCRs inside a CHARACTERISTIC
// for the list of AXIS_DESCR the ordering matters: the first AXIS_DESCR describes the x axis, the second describes the y axis, etc.
fn update_characteristic_axis(
    axis_descr: &mut Vec<AxisDescr>,
    record_layout: Option<&RecordLayout>,
    axis_pts_dim: &HashMap<String, u16>,
    typeinfo: &TypeInfo
) {
    let mut axis_positions = Vec::<Option<u16>>::new();
    // record_layout can only be None if the file is damaged. The spec requires a reference to a valid RECORD_LAYOUT
    if let Some(rl) = record_layout {
        let itemrefs = [
            &rl.axis_pts_x,
            &rl.axis_pts_y,
            &rl.axis_pts_z,
            &rl.axis_pts_4,
            &rl.axis_pts_5
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
                if let Some(TypeInfo::Array{dim, ..}) = get_inner_type(typeinfo, position) {
                    axis_descr.max_axis_points = dim[0] as u16;
                }
            }
        }
    }
}


// update the address of a CHARACTERISTIC
fn update_characteristic_address<'a>(
    characteristic: &mut Characteristic,
    debug_data: &'a DebugData
) -> Option<&'a TypeInfo> {
    let (symbol_info, symbol_name) = get_symbol_info(
        &characteristic.name,
        &characteristic.symbol_link,
        &characteristic.if_data,
        debug_data
    );

    if let Some((address, symbol_datatype)) = symbol_info {
        // make sure a valid SYMBOL_LINK exists
        set_symbol_link(&mut characteristic.symbol_link, symbol_name.clone());
        characteristic.address = address as u32;
        set_measurement_bitmask(&mut characteristic.bit_mask, symbol_datatype);
        update_ifdata(&mut characteristic.if_data, symbol_name, symbol_datatype, address);

        Some(symbol_datatype)
    } else {
        None
    }
}


// when update runs without preserve, CHARACTERISTICs could be removed from the module
// these items should also be removed from the identifier lists in GROUPs and FUNCTIONs
pub(crate) fn cleanup_removed_characteristics(
    module: &mut Module,
    removed_items: &HashSet<String>
) {
    if removed_items.len() == 0 {
        return;
    }

    for group in &mut module.group {
        if let Some(ref_characteristic) = &mut group.ref_characteristic {
            cleanup_item_list(&mut ref_characteristic.identifier_list, &removed_items);
            if ref_characteristic.identifier_list.len() == 0 {
                group.ref_characteristic = None;
            }
        }
    }

    for function in &mut module.function {
        if let Some(def_characteristic) = &mut function.def_characteristic {
            cleanup_item_list(&mut def_characteristic.identifier_list, &removed_items);
            if def_characteristic.identifier_list.len() == 0 {
                function.def_characteristic = None;
            }
        }
        if let Some(ref_characteristic) = &mut function.ref_characteristic {
            cleanup_item_list(&mut ref_characteristic.identifier_list, &removed_items);
            if ref_characteristic.identifier_list.len() == 0 {
                function.ref_characteristic = None;
            }
        }
    }
}
