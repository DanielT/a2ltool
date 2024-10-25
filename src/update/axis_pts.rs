use crate::dwarf::DwarfDataType;
use crate::dwarf::{DebugData, TypeInfo};
use crate::symbol::SymbolInfo;
use crate::A2lVersion;
use a2lfile::{A2lObject, AxisPts, Module};
use std::collections::HashMap;
use std::collections::HashSet;

use crate::update::{
    adjust_limits,
    enums::{cond_create_enum_conversion, update_enum_compu_methods},
    get_axis_pts_x_memberid, get_inner_type, get_symbol_info,
    ifdata_update::{update_ifdata_address, update_ifdata_type, zero_if_data},
    log_update_errors, set_symbol_link, update_record_layout,
};

use super::{make_symbol_link_string, UpdateInfo};

pub(crate) fn update_module_axis_pts(
    info: &mut UpdateInfo,
    compu_method_index: &HashMap<String, usize>,
) -> (u32, u32) {
    let mut enum_convlist = HashMap::<String, &TypeInfo>::new();
    let mut removed_items = HashSet::<String>::new();
    let mut axis_pts_list = Vec::new();
    let mut axis_pts_updated: u32 = 0;
    let mut axis_pts_not_updated: u32 = 0;

    std::mem::swap(&mut info.module.axis_pts, &mut axis_pts_list);
    for mut axis_pts in axis_pts_list {
        match get_symbol_info(
            &axis_pts.name,
            &axis_pts.symbol_link,
            &axis_pts.if_data,
            info.debug_data,
        ) {
            // match update_axis_pts_address(&mut axis_pts, info.debug_data, info.version) {
            Ok(sym_info) => {
                update_axis_pts_address(&mut axis_pts, info.debug_data, info.version, &sym_info);

                update_ifdata_address(&mut axis_pts.if_data, &sym_info.name, sym_info.address);

                if info.full_update {
                    // update the data type of the AXIS_PTS object
                    update_ifdata_type(&mut axis_pts.if_data, sym_info.typeinfo);

                    update_axis_pts_datatype(
                        &mut axis_pts,
                        info,
                        sym_info,
                        &mut enum_convlist,
                        compu_method_index,
                    );
                } else if info.strict_update {
                    // verify that the data type of the AXIS_PTS object is still correct
                    verify_axis_pts_datatype(info, &axis_pts, sym_info, compu_method_index);
                }

                // put the updated AXIS_PTS back on the module's list
                info.module.axis_pts.push(axis_pts);
                axis_pts_updated += 1;
            }
            Err(errmsgs) => {
                log_update_errors(info.log_msgs, errmsgs, "AXIS_PTS", axis_pts.get_line());

                if info.preserve_unknown {
                    axis_pts.address = 0;
                    zero_if_data(&mut axis_pts.if_data);
                    info.module.axis_pts.push(axis_pts);
                } else {
                    // item is removed implicitly, because it is not added back to the list
                    removed_items.insert(axis_pts.name.clone());
                }
                axis_pts_not_updated += 1;
            }
        }
    }

    // update COMPU_VTABs and COMPU_VTAB_RANGEs based on the data types used in MEASUREMENTs etc.
    update_enum_compu_methods(info.module, &enum_convlist);
    cleanup_removed_axis_pts(info.module, &removed_items);

    (axis_pts_updated, axis_pts_not_updated)
}

// update the address of an AXIS_PTS object
pub(crate) fn update_axis_pts_address<'dbg>(
    axis_pts: &mut AxisPts,
    debug_data: &'dbg DebugData,
    version: A2lVersion,
    sym_info: &SymbolInfo,
) {
    if version >= A2lVersion::V1_6_0 {
        // make sure a valid SYMBOL_LINK exists
        let symbol_link_text = make_symbol_link_string(&sym_info, debug_data);
        set_symbol_link(&mut axis_pts.symbol_link, symbol_link_text);
    } else {
        axis_pts.symbol_link = None;
    }

    if axis_pts.address == 0 {
        // if the address was previously "0" then force it to be displayed as hex after the update
        axis_pts.get_layout_mut().item_location.2 .1 = true;
    }
    axis_pts.address = sym_info.address as u32;
}

// update the data type + associated info of an AXIS_PTS object
fn update_axis_pts_datatype<'dbg>(
    axis_pts: &mut AxisPts,
    info: &mut UpdateInfo<'_, 'dbg, '_>,
    sym_info: SymbolInfo<'dbg>,
    enum_convlist: &mut HashMap<String, &'dbg TypeInfo>,
    compu_method_index: &HashMap<String, usize>,
) {
    // the variable used for the axis should be a 1-dimensional array, or a struct containing a 1-dimensional array
    // if the type is a struct, then the AXIS_PTS_X inside the referenced RECORD_LAYOUT tells us which member of the struct to use.
    let member_id =
        get_axis_pts_x_memberid(info.module, &info.reclayout_info, &axis_pts.deposit_record);
    if let Some(inner_typeinfo) = get_inner_type(sym_info.typeinfo, member_id) {
        match &inner_typeinfo.datatype {
            DwarfDataType::Array { dim, arraytype, .. } => {
                // this is the only reasonable case for an AXIS_PTS object
                // update max_axis_points to match the size of the array
                if !dim.is_empty() {
                    axis_pts.max_axis_points = dim[0] as u16;
                }
                update_axis_pts_conversion(axis_pts, info, arraytype, enum_convlist);
            }
            DwarfDataType::Enum { .. } => {
                // likely not useful, because what purpose would an axis consisting of a single enum value serve?
                // print warning?
                axis_pts.max_axis_points = 1;
                update_axis_pts_conversion(axis_pts, info, inner_typeinfo, enum_convlist);
            }
            _ => {
                // // if the type is not an enum, then the conversion method should be set to "NO_COMPU_METHOD"
                // // print warning?
                // println!(
                //     "Warning: AXIS_PTS {} has a data type that is not an array or enum",
                //     axis_pts.name
                // );
                // println!("    Outer type info: {:#?}", sym_info.typeinfo);
                // println!("    Data type: {:?}", inner_typeinfo.datatype);
                // axis_pts.max_axis_points = 1;
                // axis_pts.conversion = "NO_COMPU_METHOD".to_string();
            }
        }

        let opt_compu_method = compu_method_index
            .get(&axis_pts.conversion)
            .and_then(|idx| info.module.compu_method.get(*idx));
        let (ll, ul) = adjust_limits(
            inner_typeinfo,
            axis_pts.lower_limit,
            axis_pts.upper_limit,
            opt_compu_method,
        );
        axis_pts.lower_limit = ll;
        axis_pts.upper_limit = ul;
    }

    // update the data type in the referenced RECORD_LAYOUT
    axis_pts.deposit_record = update_record_layout(
        info.module,
        &mut info.reclayout_info,
        &axis_pts.deposit_record,
        sym_info.typeinfo,
    );
}

fn update_axis_pts_conversion<'dbg>(
    axis_pts: &mut AxisPts,
    info: &mut UpdateInfo<'_, 'dbg, '_>,
    typeinfo: &'dbg TypeInfo,
    enum_convlist: &mut HashMap<String, &'dbg TypeInfo>,
) {
    if let DwarfDataType::Enum { enumerators, .. } = &typeinfo.datatype {
        if axis_pts.conversion == "NO_COMPU_METHOD" {
            axis_pts.conversion = typeinfo
                .name
                .clone()
                .unwrap_or_else(|| format!("{}_compu_method", axis_pts.name))
                .clone();
        }
        cond_create_enum_conversion(info.module, &axis_pts.conversion, enumerators);
        enum_convlist.insert(axis_pts.conversion.clone(), typeinfo);
    }
    // can't delete existing COMPU_METHODs in an else branch, because they might contain user-defined conversion formulas
}

fn verify_axis_pts_datatype(
    info: &mut UpdateInfo<'_, '_, '_>,
    axis_pts: &AxisPts,
    sym_info: SymbolInfo<'_>,
    compu_method_index: &HashMap<String, usize>,
) {
    let member_id =
        get_axis_pts_x_memberid(info.module, &info.reclayout_info, &axis_pts.deposit_record);
    if let Some(inner_typeinfo) = get_inner_type(sym_info.typeinfo, member_id) {
        let max_axis_pts = if let DwarfDataType::Array { dim, .. } = &inner_typeinfo.datatype {
            *dim.get(0).unwrap_or(&1) as u16
        } else {
            1
        };
        let opt_compu_method = compu_method_index
            .get(&axis_pts.conversion)
            .and_then(|idx| info.module.compu_method.get(*idx));
        let (ll, ul) = adjust_limits(
            inner_typeinfo,
            axis_pts.lower_limit,
            axis_pts.upper_limit,
            opt_compu_method,
        );

        if max_axis_pts != axis_pts.max_axis_points
            || ll != axis_pts.lower_limit
            || ul != axis_pts.upper_limit
        {
            log_update_errors(
                info.log_msgs,
                vec![format!(
                    "Data type of AXIS_PTS {} has changed",
                    axis_pts.name
                )],
                "AXIS_PTS",
                axis_pts.get_line(),
            );
        }
    }
}

// when update runs without preserve, AXIS_PTS be removed from the module
// AXIS_PTS are only referenced through CHARACTERISTIC > AXIS_DESCR > AXIS_PTS_REF
pub(crate) fn cleanup_removed_axis_pts(module: &mut Module, removed_items: &HashSet<String>) {
    if removed_items.is_empty() {
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
