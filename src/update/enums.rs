use std::collections::HashMap;
use a2lfile::*;
use crate::dwarf::TypeInfo;

// create a COMPU_METHOD and a COMPU_VTAB for the typename of an enum
pub(crate) fn cond_create_enum_conversion(module: &mut Module, typename: &str) {
    let compu_method_find = module.compu_method.iter().find(|item| item.name == typename);
    if compu_method_find.is_none() {
        let mut new_compu_method = CompuMethod::new(
            typename.to_string(),
            format!("Conversion table for enum {}", typename),
            ConversionType::TabVerb,
            "%.4".to_string(),
            "".to_string()
        );
        new_compu_method.compu_tab_ref = Some(CompuTabRef::new(typename.to_string()));
        module.compu_method.push(new_compu_method);

        let compu_vtab_find = module.compu_vtab.iter().find(|item| item.name == typename);
        let compu_vtab_range_find = module.compu_vtab_range.iter().find(|item| item.name == typename);

        if compu_vtab_find.is_none() && compu_vtab_range_find.is_none() {
            module.compu_vtab.push(CompuVtab::new(
                typename.to_string(),
                format!("Conversion table for enum {}", typename),
                ConversionType::TabVerb,
                0 // will be updated by update_enum_compu_methods, which will also add the actual enum values
            ));
        }
    }
}


// every MEASUREMENT, CHARACTERISTIC and AXIS_PTS object can reference a COMPU_METHOD which describes the conversion of values
// in some cases the the COMPU_METHOS in turn references a COMPU_VTAB to provide number to string mapping and display named values
// These COMPU_VTAB objects are typically based on an enum in the original software.
// By following the chain from the MEASUREMENT (etc.), we know what type is associated with the COMPU_VTAB and can add or
// remove enumerators to match the software
pub(crate) fn update_enum_compu_methods(module: &mut Module, enum_convlist: &HashMap<String, &TypeInfo>) {
    // enum_convlist: a table of COMPU_METHODS and the associated types (filtered to contain only enums)
    // if the list is empty then there is nothing to do
    if enum_convlist.len() == 0 {
        return;
    }

    // follow the chain of objects and build a list of COMPU_TAB_REF references with their associated enum types
    let mut enum_compu_tab = HashMap::new();
    for compu_method in &module.compu_method {
        if let Some(typeinfo) = enum_convlist.get(&compu_method.name) {
            if let Some(compu_tab) = &compu_method.compu_tab_ref {
                enum_compu_tab.insert(compu_tab.conversion_table.clone(), *typeinfo);
            }
        }
    }

    // check all COMPU_VTABs in the module to see if we know of an associated enum type
    for compu_vtab in &mut module.compu_vtab {
        if let Some(TypeInfo::Enum{enumerators, ..}) = enum_compu_tab.get(&compu_vtab.name) {
            // TabVerb is the only permitted conversion type for a compu_vtab
            compu_vtab.conversion_type = ConversionType::TabVerb;

            // if compu_vtab has more entries than the enum, delete the extras
            while compu_vtab.value_pairs.len() > enumerators.len() {
                compu_vtab.value_pairs.pop();
            }
            // if compu_vtab has less entries than the enum, append some dummy entries
            while compu_vtab.value_pairs.len() < enumerators.len() {
                compu_vtab.value_pairs.push(ValuePairsStruct::new(0f64, "dummy".to_string()));
            }
            compu_vtab.number_value_pairs = enumerators.len() as u16;

            // overwrite the current compu_vtab entries with the values from the enum
            for (idx, (name, value)) in enumerators.iter().enumerate() {
                compu_vtab.value_pairs[idx].in_val = *value as f64;
                compu_vtab.value_pairs[idx].out_val = name.clone();
            }
        }
    }

    // do the same for COMPU_VTAB_RANGE, because the enum could also be stored as a COMPU_VTAB_RANGE where min = max for all entries
    for compu_vtab_range in &mut module.compu_vtab_range {
        if let Some(TypeInfo::Enum{enumerators, ..}) = enum_compu_tab.get(&compu_vtab_range.name) {
            // if compu_vtab_range has more entries than the enum, delete the extras
            while compu_vtab_range.value_triples.len() > enumerators.len() {
                compu_vtab_range.value_triples.pop();
            }
            // if compu_vtab_range has less entries than the enum, append some dummy entries
            while compu_vtab_range.value_triples.len() < enumerators.len() {
                compu_vtab_range.value_triples.push(ValueTriplesStruct::new(0f64, 0f64, "dummy".to_string()));
            }
            compu_vtab_range.number_value_triples = enumerators.len() as u16;

            // overwrite the current compu_vtab_range entries with the values from the enum
            for (idx, (name, value)) in enumerators.iter().enumerate() {
                compu_vtab_range.value_triples[idx].in_val_min = *value as f64;
                compu_vtab_range.value_triples[idx].in_val_max = *value as f64;
                compu_vtab_range.value_triples[idx].out_val = name.clone();
            }
        }
    }
}
