use crate::debuginfo::{DbgDataType, TypeInfo};
use crate::update::get_a2l_datatype;
use a2lfile::{A2lObjectName, A2lObjectNameSetter, ItemList, Module, RecordLayout};

#[derive(Debug)]
pub(crate) struct RecordLayoutInfo {
    pub(crate) refcount: Vec<usize>,
}

pub(crate) fn get_axis_pts_x_memberid(module: &Module, recordlayout_name: &str) -> u16 {
    let mut memberid = 1;
    if let Some(rl_idx) = module.record_layout.index(recordlayout_name) {
        if let Some(axis_pts_x) = &module.record_layout[rl_idx].axis_pts_x {
            memberid = axis_pts_x.position;
        }
    }
    memberid
}

pub(crate) fn get_fnc_values_memberid(module: &Module, recordlayout_name: &str) -> u16 {
    let mut memberid = 1;
    if let Some(rl_idx) = module.record_layout.index(recordlayout_name) {
        if let Some(fnc_values) = &module.record_layout[rl_idx].fnc_values {
            memberid = fnc_values.position;
        }
    }
    memberid
}

pub(crate) fn get_inner_type(typeinfo: &TypeInfo, memberid: u16) -> Option<&TypeInfo> {
    // memberid is (supposed to) start counting at 1, but array indexing is based on 0
    let id = if memberid > 0 {
        (memberid - 1) as usize
    } else {
        0
    };

    match &typeinfo.datatype {
        DbgDataType::Struct { members, .. } => {
            if id < members.len() {
                Some(&members[id].0)
            } else {
                None
            }
        }
        _ => {
            if id == 0 {
                Some(typeinfo)
            } else {
                None
            }
        }
    }
}

pub(crate) fn update_record_layout(
    module: &mut Module,
    recordlayout_info: &mut RecordLayoutInfo,
    name: &str,
    typeinfo: &TypeInfo,
) -> String {
    if let Some(idx) = module.record_layout.index(name) {
        let mut new_reclayout = module.record_layout[idx].clone();
        let mut new_name = None;

        // FNC_VALUES - required in record layouts used by a CHARACTERISTIC
        if let Some(fnc_values) = &mut new_reclayout.fnc_values {
            if let Some(itemtype) = get_inner_type(typeinfo, fnc_values.position) {
                let new_datatype = get_a2l_datatype(itemtype);
                if new_datatype != fnc_values.datatype {
                    // try to update the name based on the datatype, e.g. __UBYTE_S to __ULONG_S
                    new_name = Some(module.record_layout[idx].get_name().replacen(
                        &fnc_values.datatype.to_string(),
                        &new_datatype.to_string(),
                        1,
                    ));
                    fnc_values.datatype = new_datatype;
                }
            }
        }
        if let Some(new_name) = new_name {
            new_reclayout.set_name(new_name);
        }

        // AXIS_PTS_X - required in record layouts used by an AXIS_PTS, optional for CHARACTERISTIC
        if let Some(axis_pts_x) = &mut new_reclayout.axis_pts_x {
            if let Some(itemtype) = get_inner_type(typeinfo, axis_pts_x.position) {
                axis_pts_x.datatype = get_a2l_datatype(itemtype);
                if let DbgDataType::Array { dim, .. } = &itemtype.datatype {
                    // FIX_NO_AXIS_PTS_X
                    if let Some(fix_no_axis_pts_x) = &mut new_reclayout.fix_no_axis_pts_x {
                        fix_no_axis_pts_x.number_of_axis_points = dim[0] as u16;
                    }
                }
            }
        }
        // NO_AXIS_PTS_X
        if let Some(no_axis_pts_x) = &mut new_reclayout.no_axis_pts_x {
            if let Some(itemtype) = get_inner_type(typeinfo, no_axis_pts_x.position) {
                no_axis_pts_x.datatype = get_a2l_datatype(itemtype);
            }
        }

        // AXIS_PTS_Y
        if let Some(axis_pts_y) = &mut new_reclayout.axis_pts_y {
            if let Some(itemtype) = get_inner_type(typeinfo, axis_pts_y.position) {
                axis_pts_y.datatype = get_a2l_datatype(itemtype);
                if let DbgDataType::Array { dim, .. } = &itemtype.datatype {
                    // FIX_NO_AXIS_PTS_Y
                    if let Some(fix_no_axis_pts_y) = &mut new_reclayout.fix_no_axis_pts_y {
                        fix_no_axis_pts_y.number_of_axis_points = dim[0] as u16;
                    }
                }
            }
        }
        // NO_AXIS_PTS_Y
        if let Some(no_axis_pts_y) = &mut new_reclayout.no_axis_pts_y {
            if let Some(itemtype) = get_inner_type(typeinfo, no_axis_pts_y.position) {
                no_axis_pts_y.datatype = get_a2l_datatype(itemtype);
            }
        }

        // AXIS_PTS_Z
        if let Some(axis_pts_z) = &mut new_reclayout.axis_pts_z {
            if let Some(itemtype) = get_inner_type(typeinfo, axis_pts_z.position) {
                axis_pts_z.datatype = get_a2l_datatype(itemtype);
                if let DbgDataType::Array { dim, .. } = &itemtype.datatype {
                    // FIX_NO_AXIS_PTS_Z
                    if let Some(fix_no_axis_pts_z) = &mut new_reclayout.fix_no_axis_pts_z {
                        fix_no_axis_pts_z.number_of_axis_points = dim[0] as u16;
                    }
                }
            }
        }
        // NO_AXIS_PTS_Z
        if let Some(no_axis_pts_z) = &mut new_reclayout.no_axis_pts_z {
            if let Some(itemtype) = get_inner_type(typeinfo, no_axis_pts_z.position) {
                no_axis_pts_z.datatype = get_a2l_datatype(itemtype);
            }
        }

        // AXIS_PTS_4
        if let Some(axis_pts_4) = &mut new_reclayout.axis_pts_4 {
            if let Some(itemtype) = get_inner_type(typeinfo, axis_pts_4.position) {
                axis_pts_4.datatype = get_a2l_datatype(itemtype);
                if let DbgDataType::Array { dim, .. } = &itemtype.datatype {
                    // FIX_NO_AXIS_PTS_4
                    if let Some(fix_no_axis_pts_4) = &mut new_reclayout.fix_no_axis_pts_4 {
                        fix_no_axis_pts_4.number_of_axis_points = dim[0] as u16;
                    }
                }
            }
        }
        // NO_AXIS_PTS_4
        if let Some(no_axis_pts_4) = &mut new_reclayout.no_axis_pts_4 {
            if let Some(itemtype) = get_inner_type(typeinfo, no_axis_pts_4.position) {
                no_axis_pts_4.datatype = get_a2l_datatype(itemtype);
            }
        }

        // AXIS_PTS_5
        if let Some(axis_pts_5) = &mut new_reclayout.axis_pts_5 {
            if let Some(itemtype) = get_inner_type(typeinfo, axis_pts_5.position) {
                axis_pts_5.datatype = get_a2l_datatype(itemtype);
                if let DbgDataType::Array { dim, .. } = &itemtype.datatype {
                    // FIX_NO_AXIS_PTS_5
                    if let Some(fix_no_axis_pts_5) = &mut new_reclayout.fix_no_axis_pts_5 {
                        fix_no_axis_pts_5.number_of_axis_points = dim[0] as u16;
                    }
                }
            }
        }
        // NO_AXIS_PTS_5
        if let Some(no_axis_pts_5) = &mut new_reclayout.no_axis_pts_5 {
            if let Some(itemtype) = get_inner_type(typeinfo, no_axis_pts_5.position) {
                no_axis_pts_5.datatype = get_a2l_datatype(itemtype);
            }
        }

        if module.record_layout[idx] == new_reclayout {
            // no changes were made, return the name of the existing record layout and don't use the cloned version
            name.to_owned()
        } else {
            // try to find an existing record_layout with these parameters
            if let Some((existing_idx, existing_reclayout)) = module
                .record_layout
                .iter()
                .enumerate()
                .find(|&(_, item)| compare_rl_content(&new_reclayout, item))
            {
                // there already is a record layout with these parameters
                recordlayout_info.refcount[idx] -= 1;
                recordlayout_info.refcount[existing_idx] += 1;
                existing_reclayout.get_name().to_string()
            } else if recordlayout_info.refcount[idx] == 1 {
                // the original record layout only has one reference; that means we can replace it
                // append the new record layout to the and of the list, and then move it to idx using swap_remove
                module.record_layout.push(new_reclayout);
                module.record_layout.swap_remove_idx(idx);
                module.record_layout[idx].get_name().to_string()
            } else {
                // the original record layout has multiple users, so it's reference count
                // decreases by one and the new record layout is added to the list
                recordlayout_info.refcount[idx] -= 1;
                new_reclayout.set_name(make_unique_reclayout_name(
                    new_reclayout.get_name().to_string(),
                    &module.record_layout,
                ));
                recordlayout_info.refcount.push(1);
                module.record_layout.push(new_reclayout);
                module.record_layout.last().unwrap().get_name().to_string()
            }
        }
    } else {
        // the record layout name used in the CHARACTERISTIC does not refer to a valid record layout
        // this can only be fixed manually, so continue using the invalid name here
        name.to_owned()
    }
}

fn make_unique_reclayout_name(
    initial_name: String,
    record_layout: &ItemList<RecordLayout>,
) -> String {
    if record_layout.contains_key(&initial_name) {
        // the record layout name already exists. Now we want to extend the name to make it unique
        // e.g. BASIC_RECORD_LAYOUT to BASIC_RECORD_LAYOUT_UPDATED
        // if there are multiple BASIC_RECORD_LAYOUT_UPDATED we want to continue with BASIC_RECORD_LAYOUT_UPDATED.2, .3 , etc
        // instead of BASIC_RECORD_LAYOUT_UPDATED_UPDATED
        let basename = if let Some(pos) = initial_name.find("_UPDATED") {
            let end_of_updated = pos + "_UPDATED".len();
            if end_of_updated == initial_name.len()
                || initial_name[end_of_updated..].starts_with('.')
            {
                initial_name[..end_of_updated].to_string()
            } else {
                format!("{initial_name}_UPDATED")
            }
        } else {
            format!("{initial_name}_UPDATED")
        };
        let mut outname = basename.clone();
        let mut counter = 1;
        while record_layout.contains_key(&outname) {
            counter += 1;
            outname = format!("{basename}.{counter}");
        }
        outname
    } else {
        initial_name
    }
}

// compare two record layouts, but without considering the name
fn compare_rl_content(a: &RecordLayout, b: &RecordLayout) -> bool {
    a.alignment_byte == b.alignment_byte
        && a.alignment_float16_ieee == b.alignment_float16_ieee
        && a.alignment_float32_ieee == b.alignment_float32_ieee
        && a.alignment_float64_ieee == b.alignment_float64_ieee
        && a.alignment_int64 == b.alignment_int64
        && a.alignment_long == b.alignment_long
        && a.alignment_word == b.alignment_word
        && a.axis_pts_x == b.axis_pts_x
        && a.axis_pts_y == b.axis_pts_y
        && a.axis_pts_z == b.axis_pts_z
        && a.axis_pts_4 == b.axis_pts_4
        && a.axis_pts_5 == b.axis_pts_5
        && a.axis_rescale_x == b.axis_rescale_x
        && a.axis_rescale_y == b.axis_rescale_y
        && a.axis_rescale_z == b.axis_rescale_z
        && a.axis_rescale_4 == b.axis_rescale_4
        && a.axis_rescale_5 == b.axis_rescale_5
        && a.dist_op_x == b.dist_op_x
        && a.dist_op_y == b.dist_op_y
        && a.dist_op_z == b.dist_op_z
        && a.dist_op_4 == b.dist_op_4
        && a.dist_op_5 == b.dist_op_5
        && a.fix_no_axis_pts_x == b.fix_no_axis_pts_x
        && a.fix_no_axis_pts_y == b.fix_no_axis_pts_y
        && a.fix_no_axis_pts_z == b.fix_no_axis_pts_z
        && a.fix_no_axis_pts_4 == b.fix_no_axis_pts_4
        && a.fix_no_axis_pts_5 == b.fix_no_axis_pts_5
        && a.fnc_values == b.fnc_values
        && a.identification == b.identification
        && a.no_axis_pts_x == b.no_axis_pts_x
        && a.no_axis_pts_y == b.no_axis_pts_y
        && a.no_axis_pts_z == b.no_axis_pts_z
        && a.no_axis_pts_4 == b.no_axis_pts_4
        && a.no_axis_pts_5 == b.no_axis_pts_5
        && a.no_rescale_x == b.no_rescale_x
        && a.no_rescale_y == b.no_rescale_y
        && a.no_rescale_z == b.no_rescale_z
        && a.no_rescale_4 == b.no_rescale_4
        && a.no_rescale_5 == b.no_rescale_5
        && a.offset_x == b.offset_x
        && a.offset_y == b.offset_y
        && a.offset_z == b.offset_z
        && a.offset_4 == b.offset_4
        && a.offset_5 == b.offset_5
        && a.reserved == b.reserved
        && a.rip_addr_w == b.rip_addr_w
        && a.rip_addr_x == b.rip_addr_x
        && a.rip_addr_y == b.rip_addr_y
        && a.rip_addr_z == b.rip_addr_z
        && a.rip_addr_4 == b.rip_addr_4
        && a.rip_addr_5 == b.rip_addr_5
        && a.shift_op_x == b.shift_op_x
        && a.shift_op_y == b.shift_op_y
        && a.shift_op_z == b.shift_op_z
        && a.shift_op_4 == b.shift_op_4
        && a.shift_op_5 == b.shift_op_5
        && a.src_addr_x == b.src_addr_x
        && a.src_addr_y == b.src_addr_y
        && a.src_addr_z == b.src_addr_z
        && a.src_addr_4 == b.src_addr_4
        && a.src_addr_5 == b.src_addr_5
        && a.static_address_offsets == b.static_address_offsets
        && a.static_record_layout == b.static_record_layout
}

impl RecordLayoutInfo {
    pub(crate) fn build(module: &Module) -> Self {
        let mut refcount = vec![0; module.record_layout.len()];
        refcount.resize(module.record_layout.len(), 0);
        for ap in &module.axis_pts {
            if let Some(idx) = module.record_layout.index(&ap.deposit_record) {
                refcount[idx] += 1;
            }
        }
        for chr in &module.characteristic {
            if let Some(idx) = module.record_layout.index(&chr.deposit) {
                refcount[idx] += 1;
            }
        }
        for td_axis in &module.typedef_axis {
            if let Some(idx) = module.record_layout.index(&td_axis.record_layout) {
                refcount[idx] += 1;
            }
        }
        for td_chr in &module.typedef_characteristic {
            if let Some(idx) = module.record_layout.index(&td_chr.record_layout) {
                refcount[idx] += 1;
            }
        }

        Self { refcount }
    }
}
