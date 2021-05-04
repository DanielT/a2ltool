use gimli::{DebuggingInformationEntry, EndianSlice, RunTimeEndian, UnitHeader};
use super::UnitList;

type SliceType<'a> = EndianSlice<'a, RunTimeEndian>;
type OptionalAttribute<'data> = Option<gimli::AttributeValue<SliceType<'data>>>;


// try to get the attribute of the type attrtype for the DIE
pub(crate) fn get_attr_value<'abbrev, 'unit>(entry: &DebuggingInformationEntry<'abbrev, 'unit, SliceType, usize>, attrtype: gimli::DwAt) -> OptionalAttribute<'unit> {
    entry
        .attr_value(attrtype)
        .or_else(|_| -> gimli::Result<OptionalAttribute> { Ok(None) })
        .unwrap()
}


// get a name as a String from a DW_AT_name attribute
pub(crate) fn get_name_attribute(entry: &DebuggingInformationEntry<SliceType, usize>) -> Option<String> {
    let name_attr = get_attr_value(&entry, gimli::constants::DW_AT_name)?;
    if let gimli::AttributeValue::String(stringval) = name_attr {
        if let Ok(utf8string) = stringval.to_string() {
            Some(utf8string.to_owned())
        } else {
            None
        }
    } else {
        None
    }
}


// get a type reference as an offset relative to the start of .debug_info from a DW_AT_type attribute
// it the type reference is a UnitRef (relative to the unit header) it will be converted first
pub(crate) fn get_typeref_attribute(entry: &DebuggingInformationEntry<SliceType, usize>, unit: &UnitHeader<SliceType>) -> Option<usize> {
    let type_attr = get_attr_value(entry, gimli::constants::DW_AT_type)?;
    match type_attr {
        gimli::AttributeValue::UnitRef(unitoffset) => {
            Some(unitoffset.to_debug_info_offset(unit).unwrap().0)
        }
        gimli::AttributeValue::DebugInfoRef(infooffset) => {
            Some(infooffset.0)
        }
        gimli::AttributeValue::DebugTypesRef(_) => {
            // .debug_types was added in DWARF v4 and removed again in v5.
            // silently ignore references to the .debug_types section
            // this is unlikely to matter as few compilers ever bothered with .debug_types
            // (for example gcc supports this, but support is only enabled if the user requests this explicitly)
            None
        }
        _ => {
            None
        }
    }
}


// get the address of a variable from a DW_AT_location attribute
// The DW_AT_location contains an Exprloc expression that allows the address to be calculated
// in complex ways, so the expression must be evaluated in order to get the address
pub(crate) fn get_location_attribute(entry: &DebuggingInformationEntry<SliceType, usize>, encoding: gimli::Encoding) -> Option<u64> {
    let loc_attr = get_attr_value(entry, gimli::constants::DW_AT_location)?;
    if let gimli::AttributeValue::Exprloc(expression) = loc_attr {
        evaluate_exprloc(expression, encoding)
    } else {
        None
    }
}


// get the address offset of a struct member from a DW_AT_data_member_location attribute
pub(crate) fn get_data_member_location_attribute(entry: &DebuggingInformationEntry<SliceType, usize>, encoding: gimli::Encoding) -> Option<u64> {
    let loc_attr = get_attr_value(entry, gimli::constants::DW_AT_data_member_location)?;
    if let gimli::AttributeValue::Exprloc(expression) = loc_attr {
        evaluate_exprloc(expression, encoding)
    } else {
        None
    }
}


// get the element size stored in the DW_AT_byte_size attribute
pub(crate) fn get_byte_size_attribute(entry: &DebuggingInformationEntry<SliceType, usize>) -> Option<u64> {
    let byte_size_attr = get_attr_value(entry, gimli::constants::DW_AT_byte_size)?;
    if let gimli::AttributeValue::Udata(byte_size) = byte_size_attr {
        Some(byte_size)
    } else {
        None
    }
}


// get the encoding of a variable from the DW_AT_encoding attribute
pub(crate) fn get_encoding_attribute(entry: &DebuggingInformationEntry<SliceType, usize>) -> Option<gimli::DwAte> {
    let encoding_attr = get_attr_value(entry, gimli::constants::DW_AT_encoding)?;
    if let gimli::AttributeValue::Encoding(enc) = encoding_attr {
        Some(enc)
    } else {
        None
    }
}


// get the upper bound of an array from the DW_AT_upper_bound attribute
pub(crate) fn get_upper_bound_attribute(entry: &DebuggingInformationEntry<SliceType, usize>) -> Option<u64> {
    let ubound_attr = get_attr_value(entry, gimli::constants::DW_AT_upper_bound)?;
    if let gimli::AttributeValue::Udata(ubound) = ubound_attr {
        Some(ubound)
    } else {
        None
    }
}


// get the byte stride of an array from the DW_AT_upper_bound attribute
// this attribute is only present if the stride is different from the element size
pub(crate) fn get_byte_stride_attribute(entry: &DebuggingInformationEntry<SliceType, usize>) -> Option<u64> {
    let stride_attr = get_attr_value(entry, gimli::constants::DW_AT_byte_stride)?;
    if let gimli::AttributeValue::Udata(stride) = stride_attr {
        Some(stride)
    } else {
        None
    }
}


// get the const value of an enumerator from the DW_AT_const_value attribute
pub(crate) fn get_const_value_attribute(entry: &DebuggingInformationEntry<SliceType, usize>) -> Option<i64> {
    let constval_attr = get_attr_value(entry, gimli::constants::DW_AT_const_value)?;
    if let gimli::AttributeValue::Sdata(value) = constval_attr {
        Some(value)
    } else {
        None
    }
}


// get the bit size of a variable from the DW_AT_bit_size attribute
// this attribute is only present if the variable is in a bitfield
pub(crate) fn get_bit_size_attribute(entry: &DebuggingInformationEntry<SliceType, usize>) -> Option<u64> {
    let bit_size_attr = get_attr_value(entry, gimli::constants:: DW_AT_bit_size)?;
    if let gimli::AttributeValue::Udata(bit_size) = bit_size_attr {
        Some(bit_size)
    } else {
        None
    }
}


// get the bit offset of a variable from the DW_AT_data_bit_offset attribute
// this attribute is only present if the variable is in a bitfield
pub(crate) fn get_bit_offset_attribute(entry: &DebuggingInformationEntry<SliceType, usize>) -> Option<u64> {
    let bit_offset_attr = get_attr_value(entry, gimli::constants::DW_AT_data_bit_offset)?;
    if let gimli::AttributeValue::Udata(bit_offset) = bit_offset_attr {
        Some(bit_offset)
    } else {
        None
    }
}


// evaluate an exprloc expression to get a variable address or struct member offset
fn evaluate_exprloc(expression: gimli::Expression<EndianSlice<RunTimeEndian>>, encoding: gimli::Encoding) -> Option<u64> {
    let mut evaluation = expression.evaluation(encoding);
    evaluation.set_object_address(0);
    evaluation.set_initial_value(0);
    evaluation.set_max_iterations(100);
    let mut eval_result = evaluation.evaluate().unwrap();
    while eval_result != gimli::EvaluationResult::Complete {
        match eval_result {
            gimli::EvaluationResult::RequiresRelocatedAddress(address) => {
                // assume that there is no relocation
                // this would be a bad bet on PC, but on embedded controllers where A2l files are used this is the standard
                eval_result = evaluation.resume_with_relocated_address(address).unwrap();
            },
            other => {
                panic!("eval result is unhandled: {:#?}", other);
                //return None
            }
        };
    }
    let result = evaluation.result();
    if let gimli::Piece {location: gimli::Location::Address {address}, ..} = result[0] {
        Some(address)
    } else {
        None
    }
}


// get a DW_AT_type attribute and return the number of the unit in which the type is located
// as well as an entries_tree iterator that can iterate over the DIEs of the type
pub(crate) fn get_entries_tree_from_attribute<'input, 'b>(
    entry: &DebuggingInformationEntry<SliceType, usize>,
    unit_list: &'b UnitList<'input>,
    current_unit: usize
) -> Option<(usize, gimli::EntriesTree<'b, 'b, EndianSlice<'input, RunTimeEndian>>)> {
    let type_attr = get_attr_value(&entry, gimli::constants::DW_AT_type)?;

    match type_attr {
        gimli::AttributeValue::DebugInfoRef(dbginfo_offset) => {
            if let Some(unit_idx) = unit_list.get_unit(dbginfo_offset.0) {
                let (unit, abbrev) = &unit_list[unit_idx];
                let unit_offset = dbginfo_offset.to_unit_offset(unit).unwrap();
                if let Ok(entries_tree) = unit.entries_tree(&abbrev, Some(unit_offset)) {
                    Some((current_unit, entries_tree))
                } else {
                    None
                }
            } else {
                None
            }
        }
        gimli::AttributeValue::UnitRef(unit_offset) => {
            let (unit, abbrev) = &unit_list[current_unit];
            if let Ok(entries_tree) = unit.entries_tree(&abbrev, Some(unit_offset)) {
                Some((current_unit, entries_tree))
            } else {
                None
            }
        }
        _ => None
    }
}
