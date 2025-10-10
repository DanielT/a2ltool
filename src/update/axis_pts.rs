use crate::A2lVersion;
use crate::datatype::get_a2l_datatype;
use crate::debuginfo::DbgDataType;
use crate::debuginfo::{DebugData, TypeInfo};
use crate::symbol::SymbolInfo;
use a2lfile::{A2lObject, A2lObjectName, AxisPts, ItemList, Module};
use std::collections::HashMap;
use std::collections::HashSet;
use std::vec;

use crate::update::{
    A2lUpdateInfo, A2lUpdater, adjust_limits,
    enums::{cond_create_enum_conversion, update_enum_compu_methods},
    get_axis_pts_x_memberid, get_inner_type, get_symbol_info,
    ifdata_update::{update_ifdata_address, update_ifdata_type, zero_if_data},
    make_symbol_link_string, set_symbol_link, update_record_layout,
};

use super::UpdateResult;

pub(crate) fn update_all_module_axis_pts(
    data: &mut A2lUpdater,
    info: &A2lUpdateInfo,
) -> Vec<UpdateResult> {
    let mut enum_convlist = HashMap::<String, &TypeInfo>::new();
    let mut removed_items = HashSet::<String>::new();
    let mut axis_pts_list = ItemList::new();
    let mut results = vec![];

    std::mem::swap(&mut data.module.axis_pts, &mut axis_pts_list);
    for mut axis_pts in axis_pts_list {
        let update_result = update_module_axis_pts(&mut axis_pts, info, data, &mut enum_convlist);
        if matches!(update_result, UpdateResult::SymbolNotFound { .. }) {
            if info.preserve_unknown {
                axis_pts.address = 0;
                zero_if_data(&mut axis_pts.if_data);
                data.module.axis_pts.push(axis_pts);
            } else {
                removed_items.insert(axis_pts.get_name().to_string());
            }
        } else {
            data.module.axis_pts.push(axis_pts);
        }
        results.push(update_result);
    }

    // update COMPU_VTABs and COMPU_VTAB_RANGEs based on the data types used in MEASUREMENTs etc.
    update_enum_compu_methods(data.module, &enum_convlist);
    cleanup_removed_axis_pts(data.module, &removed_items);

    results
}

fn update_module_axis_pts<'dbg>(
    axis_pts: &mut AxisPts,
    info: &A2lUpdateInfo<'dbg>,
    data: &mut A2lUpdater<'_>,
    enum_convlist: &mut HashMap<String, &'dbg TypeInfo>,
) -> UpdateResult {
    match get_symbol_info(
        axis_pts.get_name(),
        &axis_pts.symbol_link,
        &axis_pts.if_data,
        info.debug_data,
        info.use_new_arrays,
    ) {
        // match update_axis_pts_address(&mut axis_pts, info.debug_data, info.version) {
        Ok(sym_info) => {
            update_axis_pts_address(axis_pts, info.debug_data, info.version, &sym_info);
            update_ifdata_address(&mut axis_pts.if_data, &sym_info.name, sym_info.address);

            if info.full_update {
                // update the data type of the AXIS_PTS object
                update_ifdata_type(&mut axis_pts.if_data, sym_info.typeinfo);
                update_axis_pts_datatype(data, axis_pts, &sym_info, enum_convlist);

                UpdateResult::Updated
            } else if info.strict_update {
                // verify that the data type of the AXIS_PTS object is still correct
                verify_axis_pts_datatype(data, axis_pts, sym_info)
            } else {
                // The address of the AXIS_PTS object has been updated, and no update of the data type was requested
                UpdateResult::Updated
            }
        }
        Err(errmsgs) => UpdateResult::SymbolNotFound {
            blocktype: "AXIS_PTS",
            name: axis_pts.get_name().to_string(),
            line: axis_pts.get_line(),
            errors: errmsgs,
        },
    }
}

// update the address of an AXIS_PTS object
pub(crate) fn update_axis_pts_address(
    axis_pts: &mut AxisPts,
    debug_data: &DebugData,
    version: A2lVersion,
    sym_info: &SymbolInfo,
) {
    if version >= A2lVersion::V1_6_0 {
        // make sure a valid SYMBOL_LINK exists
        let symbol_link_text = make_symbol_link_string(sym_info, debug_data);
        set_symbol_link(&mut axis_pts.symbol_link, symbol_link_text);
    } else {
        axis_pts.symbol_link = None;
    }

    if axis_pts.address == 0 {
        // if the address was previously "0" then force it to be displayed as hex after the update
        axis_pts.get_layout_mut().item_location.2.1 = true;
    }
    axis_pts.address = sym_info.address as u32;
}

// update the data type + associated info of an AXIS_PTS object
fn update_axis_pts_datatype<'dbg>(
    data: &mut A2lUpdater,
    axis_pts: &mut AxisPts,
    sym_info: &SymbolInfo<'dbg>,
    enum_convlist: &mut HashMap<String, &'dbg TypeInfo>,
) {
    // the variable used for the axis should be a 1-dimensional array, or a struct containing a 1-dimensional array
    // if the type is a struct, then the AXIS_PTS_X inside the referenced RECORD_LAYOUT tells us which member of the struct to use.
    let member_id = get_axis_pts_x_memberid(data.module, &axis_pts.deposit_record);
    if let Some(inner_typeinfo) = get_inner_type(sym_info.typeinfo, member_id) {
        match &inner_typeinfo.datatype {
            DbgDataType::Array { dim, arraytype, .. } => {
                // this is the only reasonable case for an AXIS_PTS object
                // update max_axis_points to match the size of the array
                if !dim.is_empty() {
                    axis_pts.max_axis_points = dim[0] as u16;
                }
                update_axis_pts_conversion(data.module, axis_pts, arraytype, enum_convlist);
            }
            DbgDataType::Enum { .. } => {
                // likely not useful, because what purpose would an axis consisting of a single enum value serve?
                // print warning?
                axis_pts.max_axis_points = 1;
                update_axis_pts_conversion(data.module, axis_pts, inner_typeinfo, enum_convlist);
            }
            _ => {
                // this is a very strange AXIS_PTS object
                // skip updating the data type, since there is no safe way to proceed
            }
        }

        let opt_compu_method = data.module.compu_method.get(&axis_pts.conversion);
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
        data.module,
        &mut data.reclayout_info,
        &axis_pts.deposit_record,
        sym_info.typeinfo,
    );
}

fn update_axis_pts_conversion<'dbg>(
    module: &mut Module,
    axis_pts: &mut AxisPts,
    typeinfo: &'dbg TypeInfo,
    enum_convlist: &mut HashMap<String, &'dbg TypeInfo>,
) {
    if let DbgDataType::Enum { enumerators, .. } = &typeinfo.datatype {
        if axis_pts.conversion == "NO_COMPU_METHOD" {
            axis_pts.conversion = typeinfo
                .name
                .clone()
                .unwrap_or_else(|| format!("{}_compu_method", axis_pts.get_name()))
                .clone();
        }
        cond_create_enum_conversion(module, &axis_pts.conversion, enumerators);
        enum_convlist.insert(axis_pts.conversion.clone(), typeinfo);
    }
    // can't delete existing COMPU_METHODs in an else branch, because they might contain user-defined conversion formulas
}

fn verify_axis_pts_datatype(
    data: &mut A2lUpdater,
    axis_pts: &AxisPts,
    sym_info: SymbolInfo<'_>,
) -> UpdateResult {
    let member_id = get_axis_pts_x_memberid(data.module, &axis_pts.deposit_record);
    if let Some(inner_typeinfo) = get_inner_type(sym_info.typeinfo, member_id) {
        let max_axis_pts = if let DbgDataType::Array { dim, .. } = &inner_typeinfo.datatype {
            *dim.first().unwrap_or(&1) as u16
        } else {
            1
        };
        let opt_compu_method = data.module.compu_method.get(&axis_pts.conversion);
        let (ll, ul) = adjust_limits(
            inner_typeinfo,
            axis_pts.lower_limit,
            axis_pts.upper_limit,
            opt_compu_method,
        );

        let mut bad_datatype = false;
        if let Some(axis_pts_x) = data
            .module
            .record_layout
            .get(&axis_pts.deposit_record)
            .and_then(|rl| rl.axis_pts_x.as_ref())
        {
            let calc_datatype = get_a2l_datatype(inner_typeinfo);
            if axis_pts_x.datatype != calc_datatype {
                bad_datatype = true;
            }
        }

        if max_axis_pts != axis_pts.max_axis_points
            || ll != axis_pts.lower_limit
            || ul != axis_pts.upper_limit
            || bad_datatype
        {
            UpdateResult::InvalidDataType {
                blocktype: "AXIS_PTS",
                name: axis_pts.get_name().to_string(),
                line: axis_pts.get_line(),
            }
        } else {
            UpdateResult::Updated
        }
    } else {
        // returning "Updated" is very questionable - the AXIS_PTS is basically nonsense get_inner_type fails.
        // But: There would definitely not be a data type change
        UpdateResult::Updated
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
            if let Some(axis_pts_ref) = &axis_descr.axis_pts_ref
                && removed_items.get(&axis_pts_ref.axis_points).is_some()
            {
                axis_descr.axis_pts_ref = None;
            }
        }
    }

    for typedef_characteristic in &mut module.typedef_characteristic {
        for axis_descr in &mut typedef_characteristic.axis_descr {
            if let Some(axis_pts_ref) = &axis_descr.axis_pts_ref
                && removed_items.get(&axis_pts_ref.axis_points).is_some()
            {
                axis_descr.axis_pts_ref = None;
            }
        }
    }
}
