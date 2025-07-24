use crate::update::{A2lUpdateInfo, A2lUpdater, UpdateResult};
use a2lfile::{A2lObject, A2lObjectName, AxisPts, Characteristic, ItemList, VarCharacteristic};

// update all VAR_CHARACTERISTICs in the VARIANT_CODING section
pub(crate) fn update_variant_coding(
    data: &mut A2lUpdater,
    info: &A2lUpdateInfo<'_>,
) -> Vec<UpdateResult> {
    let mut results = Vec::new();

    // update VARIANT_CODING
    if let Some(variant) = &mut data.module.variant_coding {
        let mut var_characteristics_list = ItemList::new();
        std::mem::swap(
            &mut variant.var_characteristic,
            &mut var_characteristics_list,
        );

        for mut var_characteristic in var_characteristics_list {
            let update_result = update_var_characteristic(
                &mut var_characteristic,
                &data.module.characteristic,
                &data.module.axis_pts,
            );
            if matches!(update_result, UpdateResult::SymbolNotFound { .. }) {
                if info.preserve_unknown {
                    // in this case the addresses inside the VAR_CHARACTERISTIC are reset.
                    // The first address is set to 0 and the following ones only contain the offset.
                    variant.var_characteristic.push(var_characteristic);
                }
            } else {
                variant.var_characteristic.push(var_characteristic);
            }
            results.push(update_result);
        }
    }

    results
}

fn update_var_characteristic(
    var_characteristic: &mut VarCharacteristic,
    characteristics: &ItemList<Characteristic>,
    axis_pts: &ItemList<AxisPts>,
) -> UpdateResult {
    // the name of the VAR_CHARACTERISTIC is also the name of a CHARACTERISTIC
    if let Some(characteristic) = characteristics.get(var_characteristic.get_name()) {
        if let Some(var_address) = &mut var_characteristic.var_address {
            update_var_address(var_address, characteristic.address);
        }

        UpdateResult::Updated
    } else if let Some(axis_pts) = axis_pts.get(var_characteristic.get_name()) {
        // the name of the VAR_CHARACTERISTIC is also the name of an AXIS_PTS
        if let Some(var_address) = &mut var_characteristic.var_address {
            update_var_address(var_address, axis_pts.address);
        }

        UpdateResult::Updated
    } else {
        if let Some(var_address) = &mut var_characteristic.var_address {
            update_var_address(var_address, 0);
        }
        UpdateResult::SymbolNotFound {
            blocktype: "VAR_CHARACTERISTIC",
            name: var_characteristic.get_name().to_string(),
            line: var_characteristic.get_line(),
            errors: vec![format!(
                "VAR_CHARACTERISTIC '{}' has no matching CHARACTERISTIC or AXIS_PTS",
                var_characteristic.get_name()
            )],
        }
    }
}

fn update_var_address(var_address: &mut a2lfile::VarAddress, char_address: u32) {
    if !var_address.address_list.is_empty() {
        // The address list in a VAR_ADDRESS block is an array of addresses of the VAR_CHARACTERISTIC in various tuning blocks
        // The addresses are ordered, so the first address is the smallest (often zero in files that are not updated)
        // After updateing based on the address of the CHARACTERISTIC, the first address is set to the address of the CHARACTERISTIC
        // and the following addresses retain their offsets.
        // e.g. 0x1000 0x2000 0x3000 -> 0xabc 0x1abc 0x2abc 0x3abc
        let base_address = var_address.address_list[0];
        let difference: i64 = i64::from(char_address) - i64::from(base_address);

        for address in &mut var_address.address_list {
            // Update the address by adding the difference to the base address
            // If the address is larger than u32::MAX, the address is invalidated
            *address =
                u32::try_from(i64::from(*address) + difference).unwrap_or(*address - base_address);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use a2lfile::{CharacteristicType, VarAddress};

    #[test]
    fn test_update_var_characteristic() {
        let mut var_address = VarAddress::new();
        var_address.address_list = vec![0x1000, 0x2000, 0x3000];
        let mut var_characteristic = VarCharacteristic::new("TestVar".to_string());
        var_characteristic.var_address = Some(var_address);

        let mut characteristics = ItemList::new();
        characteristics.push(Characteristic::new(
            "TestVar".to_string(),
            "".to_string(),
            CharacteristicType::Value,
            0x5678,
            "".to_string(),
            0.0,
            "NO_COMPU_METHOD".to_string(),
            0.0,
            0.0,
        ));

        let axis_pts = ItemList::new();

        let result =
            update_var_characteristic(&mut var_characteristic, &characteristics, &axis_pts);
        assert_eq!(result, UpdateResult::Updated);
        assert_eq!(
            var_characteristic.var_address.unwrap().address_list,
            vec![0x5678, 0x6678, 0x7678]
        );
    }
}
