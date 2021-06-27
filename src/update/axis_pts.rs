use std::collections::HashMap;
use std::collections::HashSet;
use a2lfile::*;
use crate::dwarf::*;

use super::enums::*;
use super::ifdata_update::*;
use super::*;


pub(crate) fn update_module_axis_pts(module: &mut Module, debug_data: &DebugData, preserve_unknown: bool, recordlayout_info: &mut RecordLayoutInfo) -> (u32, u32) {
    let mut enum_convlist = HashMap::<String, &TypeInfo>::new();
    let mut removed_items = HashSet::<String>::new();
    let mut axis_pts_list = Vec::new();
    let mut axis_pts_updated: u32 = 0;
    let mut axis_pts_not_updated: u32 = 0;

    std::mem::swap(&mut module.axis_pts, &mut axis_pts_list);
    for mut axis_pts in axis_pts_list {
        if let Some(typeinfo) = update_axis_pts_address(&mut axis_pts, debug_data) {
            // the variable used for the axis should be a 1-dimensional array, or a struct containing a 1-dimensional array
            // if the type is a struct, then the AXIS_PTS_X inside the referenced RECORD_LAYOUT tells us which member of the struct to use.
            let member_id = get_axis_pts_x_memberid(module, recordlayout_info, &axis_pts.deposit_record);
            if let Some(inner_typeinfo) = get_inner_type(typeinfo, member_id) {

                match inner_typeinfo {
                    TypeInfo::Array{dim, arraytype, ..} => {
                        // update max_axis_points to match the size of the array
                        if dim.len() >= 1 {
                            axis_pts.max_axis_points = dim[0] as u16;
                        }
                        if let TypeInfo::Enum{typename, enumerators, ..} = &**arraytype {
                            // an array of enums? it could be done...
                            if axis_pts.conversion == "NO_COMPU_METHOD" {
                                axis_pts.conversion = typename.to_owned();
                            }
                            cond_create_enum_conversion(module, &axis_pts.conversion, enumerators);
                            enum_convlist.insert(axis_pts.conversion.clone(), arraytype);
                        }
                    }
                    TypeInfo::Enum{..} => {
                        // likely not useful, because what purpose would an axis consisting of a single enum value serve?
                        enum_convlist.insert(axis_pts.conversion.clone(), typeinfo);
                    }
                    _ => {}
                }

                let (ll, ul) = adjust_limits(inner_typeinfo, axis_pts.lower_limit, axis_pts.upper_limit);
                axis_pts.lower_limit = ll;
                axis_pts.upper_limit = ul;
            }

            // update the data type in the referenced RECORD_LAYOUT
            axis_pts.deposit_record = update_record_layout(module, recordlayout_info, &axis_pts.deposit_record, typeinfo);

            // put the updated AXIS_PTS back on the module's list
            module.axis_pts.push(axis_pts);
            axis_pts_updated += 1;
        } else {
            if preserve_unknown {
                axis_pts.address = 0;
                zero_if_data(&mut axis_pts.if_data);
                module.axis_pts.push(axis_pts);
            } else {
                // item is removed implicitly, because it is not added back to the list
                removed_items.insert(axis_pts.name.to_owned());
            }
            axis_pts_not_updated += 1;
        }
    }

    // update COMPU_VTABs and COMPU_VTAB_RANGEs based on the data types used in MEASUREMENTs etc.
    update_enum_compu_methods(module, &enum_convlist);
    cleanup_removed_axis_pts(module, &removed_items);

    (axis_pts_updated, axis_pts_not_updated)
}


// update the address of an AXIS_PTS object
fn update_axis_pts_address<'a>(axis_pts: &mut AxisPts, debug_data: &'a DebugData) -> Option<&'a TypeInfo> {
    let (symbol_info, symbol_name) =
        get_symbol_info(&axis_pts.name, &axis_pts.symbol_link, &axis_pts.if_data, debug_data);

    if let Some((address, symbol_datatype)) = symbol_info {
        // make sure a valid SYMBOL_LINK exists
        set_symbol_link(&mut axis_pts.symbol_link, symbol_name.clone());
        axis_pts.address = address as u32;
        update_ifdata(&mut axis_pts.if_data, symbol_name, symbol_datatype, address);

        Some(symbol_datatype)
    } else {
        None
    }
}


// when update runs without preserve, AXIS_PTS be removed from the module
// AXIS_PTS are only referenced through CHARACTERISTIC > AXIS_DESCR > AXIS_PTS_REF
pub(crate) fn cleanup_removed_axis_pts(module: &mut Module, removed_items: &HashSet<String>) {
    if removed_items.len() == 0 {
        return;
    }

    for characteristic in &mut module.characteristic {
        for axis_descr in &mut characteristic.axis_descr {
            if let Some(axis_pts_ref) = &axis_descr.axis_pts_ref {
                if removed_items.get(&axis_pts_ref.axis_points).is_some() {
                    axis_descr.axis_pts_ref = None;
                }
            }
        }
    }

    for typedef_characteristic in &mut module.typedef_characteristic {
        for axis_descr in &mut typedef_characteristic.axis_descr {
            if let Some(axis_pts_ref) = &axis_descr.axis_pts_ref {
                if removed_items.get(&axis_pts_ref.axis_points).is_some() {
                    axis_descr.axis_pts_ref = None;
                }
            }
        }
    }
}
