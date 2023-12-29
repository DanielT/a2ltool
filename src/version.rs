use crate::A2lVersion;
use a2lfile::{
    A2lFile, CharacteristicType, Coeffs, CoeffsLinear, ConversionType, DataType, MatrixDim,
    MemoryType,
};

pub fn convert(a2l_file: &mut A2lFile, new_version: A2lVersion) {
    match new_version {
        A2lVersion::V1_5_0 => {
            downgrade_v1_71_to_1_70(a2l_file);
            downgrade_v1_70_to_1_61(a2l_file);
            downgrade_v1_61_to_1_51(a2l_file);
            // don't know what differencs between 1.5.0 and 1.5.1 are, so just set the version and hope for the best
            if let Some(ver) = a2l_file.asap2_version.as_mut() {
                ver.version_no = 1;
                ver.upgrade_no = 50;
            }
        }
        A2lVersion::V1_5_1 => {
            downgrade_v1_71_to_1_70(a2l_file);
            downgrade_v1_70_to_1_61(a2l_file);
            downgrade_v1_61_to_1_51(a2l_file);
            if let Some(ver) = a2l_file.asap2_version.as_mut() {
                ver.version_no = 1;
                ver.upgrade_no = 51;
            }
        }
        A2lVersion::V1_6_0 => {
            downgrade_v1_71_to_1_70(a2l_file);
            downgrade_v1_70_to_1_61(a2l_file);
            if let Some(ver) = a2l_file.asap2_version.as_mut() {
                ver.version_no = 1;
                ver.upgrade_no = 60;
            }
        }
        A2lVersion::V1_6_1 => {
            downgrade_v1_71_to_1_70(a2l_file);
            downgrade_v1_70_to_1_61(a2l_file);
            if let Some(ver) = a2l_file.asap2_version.as_mut() {
                ver.version_no = 1;
                ver.upgrade_no = 61;
            }
        }
        A2lVersion::V1_7_0 => {
            downgrade_v1_71_to_1_70(a2l_file);
            if let Some(ver) = a2l_file.asap2_version.as_mut() {
                ver.version_no = 1;
                ver.upgrade_no = 70;
            }
        }
        A2lVersion::V1_7_1 => {
            if let Some(ver) = a2l_file.asap2_version.as_mut() {
                ver.version_no = 1;
                ver.upgrade_no = 71;
            }
        }
    }
}

// =================== 1.61 -> 1.51 ================================

fn downgrade_v1_61_to_1_51(a2l_file: &mut A2lFile) {
    for module in a2l_file.project.module.iter_mut() {
        for axis_pts in module.axis_pts.iter_mut() {
            if axis_pts.monotony.is_some() {
                match axis_pts.monotony.as_ref().unwrap().monotony {
                    a2lfile::MonotonyType::Monotonous
                    | a2lfile::MonotonyType::StrictMon
                    | a2lfile::MonotonyType::NotMon => axis_pts.monotony = None,
                    _ => {}
                }
            }
            axis_pts.phys_unit = None;
            axis_pts.step_size = None;
        }
        module.characteristic.retain(|ch| {
            ch.characteristic_type != CharacteristicType::Cube4
                && ch.characteristic_type != CharacteristicType::Cube5
        });
        for characteristic in module.characteristic.iter_mut() {
            for axis_descr in characteristic.axis_descr.iter_mut() {
                if axis_descr.monotony.is_some() {
                    match axis_descr.monotony.as_ref().unwrap().monotony {
                        a2lfile::MonotonyType::Monotonous
                        | a2lfile::MonotonyType::StrictMon
                        | a2lfile::MonotonyType::NotMon => axis_descr.monotony = None,
                        _ => {}
                    }
                }
                axis_descr.phys_unit = None;
                axis_descr.step_size = None;
            }
            characteristic.discrete = None;
            characteristic.phys_unit = None;
            characteristic.step_size = None;
            characteristic.symbol_link = None;
        }
        for compu_method in module.compu_method.iter_mut() {
            if compu_method.conversion_type == ConversionType::Identical {
                compu_method.conversion_type = ConversionType::RatFunc;
                // RatFunc: (a*x^2 + b*x + c) / (d*x^2 + e*x + f) -> for RatFunc to behave like Identical, b = 1.0, f = 1.0, all others = 0
                compu_method.coeffs = Some(Coeffs::new(0.0, 1.0, 0.0, 0.0, 0.0, 1.0));
            }
            if compu_method.conversion_type == ConversionType::Linear {
                compu_method.conversion_type = ConversionType::RatFunc;
                if let Some(CoeffsLinear { a, b, .. }) = compu_method.coeffs_linear.as_ref() {
                    compu_method.coeffs = Some(Coeffs::new(0.0, *a, *b, 0.0, 0.0, 1.0));
                }
            }
            compu_method.coeffs_linear = None;
        }
        for compu_tab in module.compu_tab.iter_mut() {
            compu_tab.default_value_numeric = None;
        }
        for function in module.function.iter_mut() {
            function.if_data.truncate(0);
        }
        for group in module.group.iter_mut() {
            group.if_data.truncate(0);
        }
        for measurement in module.measurement.iter_mut() {
            measurement.discrete = None;
            measurement.layout = None;
            measurement.phys_unit = None;
            measurement.symbol_link = None;
        }
        if let Some(mod_common) = module.mod_common.as_mut() {
            mod_common.alignment_int64 = None;
        }
        if let Some(mod_par) = module.mod_par.as_mut() {
            for calmethod in mod_par.calibration_method.iter_mut() {
                if let Some(calhandle) = calmethod.calibration_handle.as_mut() {
                    calhandle.calibration_handle_text = None;
                }
            }
        }
        for rl in module.record_layout.iter_mut() {
            rl.static_record_layout = None;
        }
    }
}

// =================== 1.70 -> 1.61 ================================

fn downgrade_v1_70_to_1_61(a2l_file: &mut A2lFile) {
    for module in a2l_file.project.module.iter_mut() {
        for axis_pts in module.axis_pts.iter_mut() {
            axis_pts.max_refresh = None;
            axis_pts.model_link = None;
        }
        module.blob.truncate(0);
        for characteristic in module.characteristic.iter_mut() {
            characteristic.encoding = None;
            characteristic.model_link = None;
            if let Some(matrix_dim) = characteristic.matrix_dim.as_mut() {
                downgrade_matrix_dim(matrix_dim);
            }
        }
        for function in module.function.iter_mut() {
            function.ar_component = None;
        }
        module.instance.truncate(0);
        for measurement in module.measurement.iter_mut() {
            measurement.address_type = None;
            if let Some(matrix_dim) = measurement.matrix_dim.as_mut() {
                downgrade_matrix_dim(matrix_dim);
            }
            measurement.model_link = None;
        }
        if let Some(mod_par) = module.mod_par.as_mut() {
            // remove all MEMORY_SEGMENTS with memory type NOT_IN_ECU
            mod_par
                .memory_segment
                .retain(|memseg| memseg.memory_type != MemoryType::NotInEcu);
        }
        for rl in module.record_layout.iter_mut() {
            rl.static_address_offsets = None;
        }
        module.transformer.truncate(0);
        module.typedef_axis.truncate(0);
        module.typedef_blob.truncate(0);
        module.typedef_characteristic.truncate(0);
        module.typedef_measurement.truncate(0);
        module.typedef_structure.truncate(0);
    }
}

fn downgrade_matrix_dim(matrix_dim: &mut MatrixDim) {
    // if MATRIX_DIM has less than 3 dimensions, extend with 1: [42] -> [42, 1, 1]
    while matrix_dim.dim_list.len() < 3 {
        matrix_dim.dim_list.push(1);
    }
    if matrix_dim.dim_list.len() > 3 {
        // flatten all extra dimensions, e.g. [2, 3, 4, 5, 6] -> [2, 3, (4 * 5 * 6)]
        let last_dim = matrix_dim.dim_list[2..]
            .iter()
            .fold(1u16, |acc, dimval| acc * dimval);
        matrix_dim.dim_list.truncate(2);
        matrix_dim.dim_list.push(last_dim);
    }
}

// =================== 1.71 -> 1.70 ================================

fn downgrade_v1_71_to_1_70(a2l_file: &mut A2lFile) {
    for module in a2l_file.project.module.iter_mut() {
        if let Some(mod_common) = module.mod_common.as_mut() {
            mod_common.alignment_float16_ieee = None;
        }
        for rl in module.record_layout.iter_mut() {
            rl.alignment_float16_ieee = None;
            //axis_pts
            if let Some(axis_pts_x) = rl.axis_pts_x.as_mut() {
                datatype_float16_compat(&mut axis_pts_x.datatype);
            }
            if let Some(axis_pts_y) = rl.axis_pts_y.as_mut() {
                datatype_float16_compat(&mut axis_pts_y.datatype);
            }
            if let Some(axis_pts_z) = rl.axis_pts_z.as_mut() {
                datatype_float16_compat(&mut axis_pts_z.datatype);
            }
            if let Some(axis_pts_4) = rl.axis_pts_4.as_mut() {
                datatype_float16_compat(&mut axis_pts_4.datatype);
            }
            if let Some(axis_pts_5) = rl.axis_pts_5.as_mut() {
                datatype_float16_compat(&mut axis_pts_5.datatype);
            }
            // axis_rescale
            if let Some(axis_rescale_x) = rl.axis_rescale_x.as_mut() {
                datatype_float16_compat(&mut axis_rescale_x.datatype);
            }
            if let Some(axis_rescale_y) = rl.axis_rescale_y.as_mut() {
                datatype_float16_compat(&mut axis_rescale_y.datatype);
            }
            if let Some(axis_rescale_z) = rl.axis_rescale_z.as_mut() {
                datatype_float16_compat(&mut axis_rescale_z.datatype);
            }
            if let Some(axis_rescale_4) = rl.axis_rescale_4.as_mut() {
                datatype_float16_compat(&mut axis_rescale_4.datatype);
            }
            if let Some(axis_rescale_5) = rl.axis_rescale_5.as_mut() {
                datatype_float16_compat(&mut axis_rescale_5.datatype);
            }
            // dist_op
            if let Some(dist_op_x) = rl.dist_op_x.as_mut() {
                datatype_float16_compat(&mut dist_op_x.datatype);
            }
            if let Some(dist_op_y) = rl.dist_op_y.as_mut() {
                datatype_float16_compat(&mut dist_op_y.datatype);
            }
            if let Some(dist_op_z) = rl.dist_op_z.as_mut() {
                datatype_float16_compat(&mut dist_op_z.datatype);
            }
            if let Some(dist_op_4) = rl.dist_op_4.as_mut() {
                datatype_float16_compat(&mut dist_op_4.datatype);
            }
            if let Some(dist_op_5) = rl.dist_op_5.as_mut() {
                datatype_float16_compat(&mut dist_op_5.datatype);
            }
            // fnc_values
            if let Some(fnc_values) = rl.fnc_values.as_mut() {
                datatype_float16_compat(&mut fnc_values.datatype);
            }
            // identification
            if let Some(identification) = rl.identification.as_mut() {
                datatype_float16_compat(&mut identification.datatype);
            }
            // no_axis_pts
            if let Some(no_axis_pts_x) = rl.no_axis_pts_x.as_mut() {
                datatype_float16_compat(&mut no_axis_pts_x.datatype);
            }
            if let Some(no_axis_pts_y) = rl.no_axis_pts_y.as_mut() {
                datatype_float16_compat(&mut no_axis_pts_y.datatype);
            }
            if let Some(no_axis_pts_z) = rl.no_axis_pts_z.as_mut() {
                datatype_float16_compat(&mut no_axis_pts_z.datatype);
            }
            if let Some(no_axis_pts_4) = rl.no_axis_pts_4.as_mut() {
                datatype_float16_compat(&mut no_axis_pts_4.datatype);
            }
            if let Some(no_axis_pts_5) = rl.no_axis_pts_5.as_mut() {
                datatype_float16_compat(&mut no_axis_pts_5.datatype);
            }
            // no_rescale
            if let Some(no_rescale_x) = rl.no_rescale_x.as_mut() {
                datatype_float16_compat(&mut no_rescale_x.datatype);
            }
            if let Some(no_rescale_y) = rl.no_rescale_y.as_mut() {
                datatype_float16_compat(&mut no_rescale_y.datatype);
            }
            if let Some(no_rescale_z) = rl.no_rescale_z.as_mut() {
                datatype_float16_compat(&mut no_rescale_z.datatype);
            }
            if let Some(no_rescale_4) = rl.no_rescale_4.as_mut() {
                datatype_float16_compat(&mut no_rescale_4.datatype);
            }
            if let Some(no_rescale_5) = rl.no_rescale_5.as_mut() {
                datatype_float16_compat(&mut no_rescale_5.datatype);
            }
            // offset
            if let Some(offset_x) = rl.offset_x.as_mut() {
                datatype_float16_compat(&mut offset_x.datatype);
            }
            if let Some(offset_y) = rl.offset_y.as_mut() {
                datatype_float16_compat(&mut offset_y.datatype);
            }
            if let Some(offset_z) = rl.offset_z.as_mut() {
                datatype_float16_compat(&mut offset_z.datatype);
            }
            if let Some(offset_4) = rl.offset_4.as_mut() {
                datatype_float16_compat(&mut offset_4.datatype);
            }
            if let Some(offset_5) = rl.offset_5.as_mut() {
                datatype_float16_compat(&mut offset_5.datatype);
            }
            // rip_addr
            if let Some(rip_addr_w) = rl.rip_addr_w.as_mut() {
                datatype_float16_compat(&mut rip_addr_w.datatype);
            }
            if let Some(rip_addr_x) = rl.rip_addr_x.as_mut() {
                datatype_float16_compat(&mut rip_addr_x.datatype);
            }
            if let Some(rip_addr_y) = rl.rip_addr_y.as_mut() {
                datatype_float16_compat(&mut rip_addr_y.datatype);
            }
            if let Some(rip_addr_z) = rl.rip_addr_z.as_mut() {
                datatype_float16_compat(&mut rip_addr_z.datatype);
            }
            if let Some(rip_addr_4) = rl.rip_addr_4.as_mut() {
                datatype_float16_compat(&mut rip_addr_4.datatype);
            }
            if let Some(rip_addr_5) = rl.rip_addr_5.as_mut() {
                datatype_float16_compat(&mut rip_addr_5.datatype);
            }
            // shift_op
            if let Some(shift_op_x) = rl.shift_op_x.as_mut() {
                datatype_float16_compat(&mut shift_op_x.datatype);
            }
            if let Some(shift_op_y) = rl.shift_op_y.as_mut() {
                datatype_float16_compat(&mut shift_op_y.datatype);
            }
            if let Some(shift_op_z) = rl.shift_op_z.as_mut() {
                datatype_float16_compat(&mut shift_op_z.datatype);
            }
            if let Some(shift_op_4) = rl.shift_op_4.as_mut() {
                datatype_float16_compat(&mut shift_op_4.datatype);
            }
            if let Some(shift_op_5) = rl.shift_op_5.as_mut() {
                datatype_float16_compat(&mut shift_op_5.datatype);
            }
            // src_addr
            if let Some(src_addr_x) = rl.src_addr_x.as_mut() {
                datatype_float16_compat(&mut src_addr_x.datatype);
            }
            if let Some(src_addr_y) = rl.src_addr_y.as_mut() {
                datatype_float16_compat(&mut src_addr_y.datatype);
            }
            if let Some(src_addr_z) = rl.src_addr_z.as_mut() {
                datatype_float16_compat(&mut src_addr_z.datatype);
            }
            if let Some(src_addr_4) = rl.src_addr_4.as_mut() {
                datatype_float16_compat(&mut src_addr_4.datatype);
            }
            if let Some(src_addr_5) = rl.src_addr_5.as_mut() {
                datatype_float16_compat(&mut src_addr_5.datatype);
            }
        }
        for meas in module.measurement.iter_mut() {
            datatype_float16_compat(&mut meas.datatype);
        }
        for tmeas in module.typedef_measurement.iter_mut() {
            datatype_float16_compat(&mut tmeas.datatype);
        }
        for inst in module.instance.iter_mut() {
            inst.address_type = None;
        }
        for tblob in module.typedef_blob.iter_mut() {
            tblob.address_type = None;
        }
        for tstruct in module.typedef_structure.iter_mut() {
            for sc in tstruct.structure_component.iter_mut() {
                sc.address_type = None;
            }
        }
    }
}

// replace data type float16 in v1.7.1 with uword (same length) when going back to an older version
fn datatype_float16_compat(datatype: &mut DataType) {
    if *datatype == DataType::Float16Ieee {
        *datatype = DataType::Uword;
    }
}
